//! Plugin Dev Watcher
//!
//! 开发模式文件监听器 — 监听插件产物目录变化，触发热重载
//! 检测 .wasm 变化触发 Rust 端 WASM 热重载
//! 检测 .js 变化通过 Tauri 事件通知前端重新加载 TS 模块
//!
//! 仅在开发模式下启用（cfg!(debug_assertions)）

use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::Emitter;
use tokio::sync::RwLock;

use crate::system::constants::plugin::PLUGIN_RELOAD_DEBOUNCE_MS;
use crate::system::constants::event;

/// 插件开发文件监听器
///
/// 持有 notify::Watcher 实例，监听插件产物目录变化。
/// 检测到变化后通过 AppContext 获取 PluginHost 触发热重载。
pub struct PluginDevWatcher {
    // Watcher 必须 hold 住生命周期，drop 后停止监听
    _watcher: Box<dyn Watcher + Send>,
}

impl PluginDevWatcher {
    /// 启动插件开发文件监听
    ///
    /// 监听 plugins_dir 下的文件变化，对 .wasm/.js 变化触发热重载。
    /// 使用防抖机制避免短时间内多次触发（如 cargo build 连续写入多个文件）
    ///
    /// # Arguments
    /// * `plugins_dir` - 插件产物目录（resources/plugins/desktop/）
    /// * `runtime_handle` - Tokio 运行时 Handle（notify 回调在非 Tokio 线程，需通过 handle spawn）
    pub fn start(plugins_dir: PathBuf, runtime_handle: tokio::runtime::Handle) -> Self {
        // 防抖状态：记录最近一次变化的插件 ID 和时间
        let pending: Arc<RwLock<Option<(String, Instant)>>> = Arc::new(RwLock::new(None));

        let pd = plugins_dir.clone();

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            let event = match res {
                Ok(e) => e,
                Err(e) => {
                    tracing::debug!("Plugin watcher error: {}", e);
                    return;
                }
            };

            // 只关注文件创建/修改事件
            if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                return;
            }

            for path in &event.paths {
                let ext = path.extension().map(|e| e.to_string_lossy().to_string());

                let plugin_id = match extract_plugin_id(path, &pd) {
                    Some(id) => id,
                    None => continue,
                };

                match ext.as_deref() {
                    // WASM 产物变化 → 触发 Rust 端热重载
                    Some("wasm") => {
                        tracing::info!(
                            "Plugin watcher: WASM changed for plugin '{}': {}",
                            plugin_id,
                            path.display()
                        );

                        let pending = pending.clone();
                        let plugin_id_clone = plugin_id.clone();
                        // notify 回调在非 Tokio 线程中运行，必须通过 Handle::spawn 而非 tokio::spawn
                        runtime_handle.spawn(async move {
                            // 防抖：500ms 内同一插件只触发一次重载
                            {
                                let p = pending.read().await;
                                if let Some((ref prev_id, ref prev_time)) = *p {
                                    if prev_id == &plugin_id_clone
                                        && prev_time.elapsed() < Duration::from_millis(PLUGIN_RELOAD_DEBOUNCE_MS)
                                    {
                                        tracing::debug!(
                                            "Plugin watcher: debounced reload for '{}'",
                                            plugin_id_clone
                                        );
                                        return;
                                    }
                                }
                            }
                            {
                                let mut p = pending.write().await;
                                *p = Some((plugin_id_clone.clone(), Instant::now()));
                            }

                            // 通过 AppContext 全局单例获取 PluginHost
                            let ctx = crate::system::app_context::AppContext::global();
                            let ph = ctx.plugin_host().clone();
                            match ph.reload_wasm_plugin(&plugin_id_clone).await {
                                Ok(()) => {
                                    tracing::info!(
                                        "Plugin watcher: WASM hot-reloaded '{}'",
                                        plugin_id_clone
                                    );
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Plugin watcher: WASM hot-reload failed for '{}': {}",
                                        plugin_id_clone,
                                        e
                                    );
                                }
                            }
                        });
                    }
                    // TS 产物变化 → 通知前端重新加载
                    Some("js") => {
                        tracing::info!(
                            "Plugin watcher: JS changed for plugin '{}': {}",
                            plugin_id,
                            path.display()
                        );

                        let ctx = crate::system::app_context::AppContext::global();
                        let _ = ctx.app_handle().emit(event::PLUGIN_DEV_RELOAD, serde_json::json!({
                            "pluginId": plugin_id
                        }));
                    }
                    _ => {}
                }
            }
        })
        .expect("Failed to create plugin file watcher");

        // 开始监听插件目录
        watcher
            .watch(&plugins_dir, RecursiveMode::Recursive)
            .expect("Failed to start watching plugin directory");

        tracing::info!(
            "Plugin dev watcher started: watching '{}'",
            plugins_dir.display()
        );

        Self {
            _watcher: Box::new(watcher),
        }
    }
}

/// 从变化文件路径提取插件 ID
///
/// 路径格式：plugins_dir/{plugin-id}/xxx
/// 例如：resources/plugins/desktop/com.bedcode.ai-chatbox/bedcode_plugin_ai_chatbox.wasm
///       → "com.bedcode.ai-chatbox"
fn extract_plugin_id(path: &std::path::Path, plugins_dir: &std::path::Path) -> Option<String> {
    path.strip_prefix(plugins_dir).ok()?.iter().next()?.to_str().map(String::from)
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    // 可独立测试的面仅 extract_plugin_id（纯路径解析，不依赖文件系统）。
    // start() 的回调闭包强耦合 notify 事件循环、tokio runtime_handle 与全局
    // AppContext（触发热重载 / 前端 reload 事件），且防抖状态被闭包捕获，
    // 需重构为可注入的处理器才能单测；事件回调行为暂不覆盖。

    fn plugins_dir() -> std::path::PathBuf {
        std::path::PathBuf::from("/tmp/bedcode-plugins")
    }

    /// 标准产物路径：plugins_dir/{plugin-id}/{filename} → 插件 ID
    #[test]
    fn test_extract_plugin_id_from_nested_file() {
        let dir = plugins_dir();
        let path = dir.join("com.bedcode.ai-chatbox").join("bedcode_plugin_ai_chatbox.wasm");
        assert_eq!(extract_plugin_id(&path, &dir), Some("com.bedcode.ai-chatbox".to_string()));
    }

    /// 路径不在 plugins_dir 下 → None（例如其他目录的产物）
    #[test]
    fn test_extract_plugin_id_path_outside_plugins_dir() {
        let dir = plugins_dir();
        let path = std::path::PathBuf::from("/other/plugin-a/x.wasm");
        assert_eq!(extract_plugin_id(&path, &dir), None);
    }

    /// 路径就是 plugins_dir 本身（无第一段子目录）→ None
    #[test]
    fn test_extract_plugin_id_plugins_dir_itself() {
        let dir = plugins_dir();
        assert_eq!(extract_plugin_id(&dir, &dir), None);
    }

    /// 深层目录（插件子目录下再嵌套目录）仍取第一段为插件 ID
    #[test]
    fn test_extract_plugin_id_deeply_nested_path() {
        let dir = plugins_dir();
        let path = dir.join("plugin-a").join("dist").join("assets").join("main.js");
        assert_eq!(extract_plugin_id(&path, &dir), Some("plugin-a".to_string()));
    }

    /// 非 UTF-8 路径段返回 None（to_str 失败）
    #[cfg(unix)]
    #[test]
    fn test_extract_plugin_id_non_utf8_segment() {
        use std::os::unix::ffi::OsStrExt;
        let dir = plugins_dir();
        let plugin_dir = dir.join(std::ffi::OsStr::from_bytes(b"plugin-\xFF"));
        let path = plugin_dir.join("x.wasm");
        assert_eq!(extract_plugin_id(&path, &dir), None);
    }
}
