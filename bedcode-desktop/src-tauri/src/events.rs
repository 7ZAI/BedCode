//! Events Module
//!
//! 全局事件系统：事件抽象与统一发布入口、事件匹配处理器、桌面同步事件、WS 广播转发
//!
//! 事件只有一条进入广播面的路：`AppEvent` 实现 → [`publish`] → `EventMatcher` 分发
//! → 处理器。插件事件经 [`HostSyncEvent`] 薄适配进入同一条路（会话事件下沉专项）。
//!
//! **票 09 删除 `forwarder::EventForwarder`**：「内核 `SessionManager` 状态订阅 →
//! Tauri 前端事件 `session-status-changed`」的转接通道已退役——它订阅的内核状态
//! 广播对插件会话已无流量（P1-b 真源下沉），前端那条注册名（`session:statusChange`）
//! 两侧本就不一致、且零生产消费方。会话事实的前端可见性由插件经 `host-events` /
//! `host-bus` 自己发布，宿主不再替它转接。

pub mod app_event;
pub mod host_sync_event;
pub mod matcher;
pub mod sync_handler;

pub use app_event::{publish, AppEvent, PublishError};
pub use host_sync_event::HostSyncEvent;
pub use matcher::{global_matcher, EventFilter, EventHandler, EventMatcher};
pub use sync_handler::SyncEventHandler;

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    /// 专项票 04 防回接锁：宿主事件面**不再持有**会话业务事件与载荷解释
    /// （命名对齐 `retired_kernel_session_domain_is_not_reintroduced` 先例）。
    ///
    /// 镜像枚举 `DesktopSyncEvent`、`From<SyncEvent>` 穷尽搬运、处理器里的
    /// `format!("{:?}")` 状态解读都随本专项删除；本锁挡四种回接形态：
    ///
    /// ① 再定义名字含 `SyncEvent` 的枚举（宿主重新持有一套会话事件类型）；
    /// ② 再引用被退役的镜像类型名（含改名前的字面量）；
    /// ③ 在 `src/events/**` 的实现段里按变体构造 `SyncPayload::…`
    ///    （宿主重新决定「这个变体出站放哪些字段」）；
    /// ④ 在 `src/events/**` 的实现段里出现 `SessionStatus`
    ///    （宿主重新把状态折成业务枚举再取值）。
    ///
    /// 判据只扫 `#[cfg(test)]` 之前的实现段——测试里逐变体构造是必要的覆盖面。
    ///  needle 用拼接构造，避免本锁自身被扫成违规。
    #[test]
    fn retired_session_event_mirror_is_not_reintroduced() {
        let src_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mirror_name: String = ["Desktop", "SyncEvent"].concat();
        let payload_variant: String = ["SyncPayload", "::"].concat();
        let sync_event_suffix: String = ["Sync", "Event"].concat();

        let mut violations: Vec<String> = Vec::new();
        let mut scanned = 0usize;
        let mut stack = vec![src_root.join("events"), src_root.join("enums")];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                violations.push(format!("目录不可读（锁的扫描面被搬空？）: {}", dir.display()));
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                scanned += 1;
                let Ok(content) = std::fs::read_to_string(&path) else { continue };
                let implementation = content.split("#[cfg(test)]").next().unwrap_or_default();
                for (idx, raw) in implementation.lines().enumerate() {
                    let line = raw.trim();
                    if line.is_empty() || line.starts_with("//") {
                        continue;
                    }
                    let loc = format!("{}:{}", path.display(), idx + 1);
                    if line.contains(&mirror_name) {
                        violations.push(format!("{loc}: 被退役的镜像事件类型复现 — {line}"));
                    }
                    if line.contains("enum ") && line.contains(&sync_event_suffix) {
                        violations.push(format!("{loc}: 宿主事件面定义 *SyncEvent 枚举 — {line}"));
                    }
                    if path.starts_with(src_root.join("events")) {
                        if line.contains(&payload_variant) {
                            violations.push(format!("{loc}: 宿主按变体构造 SyncPayload — {line}"));
                        }
                        if line.contains("SessionStatus") {
                            violations.push(format!("{loc}: 宿主事件面重新解读会话状态 — {line}"));
                        }
                    }
                }
            }
        }
        // 防空转：扫描面一旦小于既有源文件数，说明目录被改名/清空，锁不得静默通过
        assert!(scanned >= 8, "扫描到的源文件数异常偏少（{scanned}），锁的判据面已失效");
        assert!(
            violations.is_empty(),
            "会话事件的形状与解释权已在 com.bedcode.terminal-session + SDK wire 真源，\
             宿主只剩 HostSyncEvent 薄适配 + 瘦处理器（专项票 01–04，见 ADR 0022）：\n{}",
            violations.join("\n")
        );
    }
}
