//! BedCode Desktop - Library Entry Point

// 私有项单测必须留在 crate 内（集成测试只见 pub API）：
// `mark_first_context_menu_connect` / `forget_context_menu_label` 是本文件的私有 fn。
// 另两个源码扫描锁（能力层 / CSP 锁、热路径日志锁）不触碰任何 crate 内部符号，
// 已下沉到 `tests/capabilities_lock.rs` / `tests/hot_path_logging_lock.rs` —— 搬得动
// 的搬走，这里只留搬不动的。
#[cfg(all(test, target_os = "linux"))]
mod native_context_menu_test;

// ==================== Domain Modules ====================

pub mod commands;
pub mod crypto;
// 第一方免弹窗归属清单（产品数据真源，票 08/P0-2；判定逻辑在 wasm-core）
pub mod first_party_dirs;
// 宿主侧插件能力面（能力域端口 adapter / 强制引用行 / 白名单条目同处）——
// 票 02 批次 02 起：域实现搬进 `packages/` 能力 crate 后，宿主侧的接线落这里
pub mod plugin;
pub mod server;
pub mod system;
pub mod utils;

// ==================== 整核抽出垫片（wasm-core-whole-crate） ====================
// wasm_core / db / pty 已迁入 `bedcode-wasm-core` crate（.scratch/
// 2026-10-06-wasm-core-whole-crate/spec.md）；以下 `pub use` 垫片保持既有
// `crate::wasm_core::*` / `crate::db::*` 路径零改动编译通过（spec §4.3 D3）。
// 反双份锁见 tests/（整核抽出结构锁）。`enums` 垫片已随无消费者整体退役
// （2026-10-10：真源在 SDK `bedcode-plugin-api::wire`）。
//
// **`pty` 不在再导出名单里**：host-pty 能力域（引擎 + WIT 接线 + 域机制）已整面迁到
// `bedcode-pty-engine`（pty-capability-domain 票 D1/D3），内核不再有 PTY 面可垫；
// 调用点一律写显式路径 `bedcode_pty_engine::plugin_binding::*`。宿主侧端口 adapter
// 自票 02 批次 02 起落 `crate::plugin::pty`（原 wasm-core `host_api/pty.rs`）。
pub use bedcode_wasm_core as wasm_core;
pub use bedcode_wasm_core::db;

// 桥接基准工程的 Channel 传输面（**仅 debug 构建**：release 产物不含本命令面，
// 闸门锁见本模块 tests::bench_channel_surface_stays_debug_only）
#[cfg(debug_assertions)]
mod bench_channel;

// ==================== Re-exports ====================

use crate::server::peer_net_cmds;
pub use system::{AppConfig, AppContext, AppError, Result};

// ==================== Application Setup ====================

use db::Database;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::Mutex;

/// 删除当天已存在的日志文件（仅 dev 构建调用）
///
/// dev 启动频次高，按天追加会让同一天的日志混入多次启动的片段，难以定位；
/// 因此 dev 启动时替换当天日志（删旧建新）。release 保持按天追加轮转。
/// 必须在 RollingFileAppender 构建前调用，确保 appender 首次写入创建全新文件。
/// dev 启动同时重置 `bootstrap.log`（bootstrap 通道先于本函数就绪；重置消息本身
/// 经 bootstrap 落盘即重建该文件，见 system::logging::bootstrap_log）。
#[cfg(debug_assertions)]
fn reset_today_logs(log_dir: &std::path::Path) {
    // bootstrap.log（固定文件名，bootstrap 通道产物、无轮转）：dev 重启替换，
    // 避免多次启动的启动早期片段混叠；消息经 bootstrap 通道写入即重建文件
    let bootstrap_path = log_dir.join("bootstrap.log");
    if bootstrap_path.exists() {
        match std::fs::remove_file(&bootstrap_path) {
            Ok(()) => system::logging::bootstrap_log(
                tracing::Level::INFO,
                format!("[logging] dev reset: replaced {}", bootstrap_path.display()),
            ),
            Err(e) => system::logging::bootstrap_log(
                tracing::Level::ERROR,
                format!(
                    "[logging] dev reset: failed to replace {}: {e}",
                    bootstrap_path.display()
                ),
            ),
        }
    }
    // tracing_appender 的 rolling 文件名日期用 UTC（与本地日期可能错位一天）
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    // 日志重置列表：runtime / error / frontend（前端 console 单独文件，见 init_logging）
    for prefix in ["runtime", "error", "frontend"] {
        let path = log_dir.join(format!("{prefix}.{today}.log"));
        if path.exists() {
            match std::fs::remove_file(&path) {
                Ok(()) => system::logging::bootstrap_log(
                    tracing::Level::INFO,
                    format!("[logging] dev reset: replaced today's log {}", path.display()),
                ),
                Err(e) => system::logging::bootstrap_log(
                    tracing::Level::ERROR,
                    format!("[logging] dev reset: failed to replace {}: {e}", path.display()),
                ),
            }
        }
    }
}

/// 初始化日志系统
///
/// 接受 LogConfig 参数，所有日志行为均可通过配置文件控制
fn init_logging(app_handle: &tauri::AppHandle, log_config: &system::config::LogConfig) -> Result<()> {
    let log_dir = app_handle.path().app_log_dir().expect("Failed to get log directory");

    // bootstrap 通道幂等兜底：setup 早期已初始化（config 复制/加载沿用），此处仅防漏
    system::logging::bootstrap_init(&log_dir)?;
    std::fs::create_dir_all(&log_dir)?;

    // dev 构建替换当天日志（release 保持追加轮转）
    #[cfg(debug_assertions)]
    reset_today_logs(&log_dir);

    // 构建日志订阅器：文件层非阻塞写盘 + 控制台层。
    // 过滤语义与旧实现一致（error 固定 ERROR、runtime 按级别、frontend 仅 dev），
    // 全部收敛于 system::logging::build_logging，便于独立单测（见该模块测试）
    let (setup, subscriber) = system::logging::build_logging(&log_dir, log_config, cfg!(debug_assertions))?;
    install_subscriber(subscriber);

    // 保存句柄到进程级全局：worker guard 存活到进程退出（drop 时 flush 剩余日志）；
    // 级别热调（file_level_reload）与容量裁剪/丢弃告警（writers）由后续模块从此读取
    system::logging::store_setup(setup);

    // 启动后台日志维护任务：容量裁剪（超限删最旧，当前在写文件除外）+ 非阻塞队列丢弃告警（03）
    system::logging::spawn_log_maintenance(
        system::logging::global_setup().expect("logging setup stored before maintenance"),
        log_config.capacity_bytes,
    );

    tracing::info!("Logging initialized. Log directory: {:?}", log_dir);
    tracing::info!(
        "Log config: file_level={}, rotation={}, max_files={}, console_in_release={}",
        log_config.file_level,
        log_config.rotation,
        log_config.max_files,
        log_config.console_in_release,
    );
    tracing::info!("BedCode Desktop v{} starting...", env!("CARGO_PKG_VERSION"));

    Ok(())
}

/// 安装 tracing 全局订阅器（容忍 log→tracing 桥已被抢占）
///
/// debug 构建下 tauri-plugin-wdio 的 `.setup()` 会先 `log::set_boxed_logger`
/// 抢占 log 全局 logger（其 setup 早于应用 setup 执行），使 `try_init()` 在
/// `LogTracer::init()` 阶段返回 `SetLoggerError` 并 panic。拆成两步：桥安装
/// 失败仅忽略（log 记录由 wdio 自带 logger 承接），tracing 全局默认仍须设置
/// （与 `try_init` 第二步 `set_global_default` 等价）。
fn install_subscriber<S>(subscriber: S)
where
    S: tracing::Subscriber + Send + Sync + 'static,
{
    let _ = tracing_log::LogTracer::init();
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");
}

/// 应用启动时间，用于计算启动耗时
pub struct AppStartTime(std::time::Instant);

/// 前端插件通道会话的生命周期钩子（审计票 06）
///
/// 页面加载重置**该 webview** 的 loader 会话密钥与全部插件令牌：宿主前端 bootstrap 会在导入
/// 任何插件模块之前重新取得密钥，而插件代码只在模块被导入后才开始运行——「首个调用者生效」
/// 因此恒由宿主前端赢得；dev 下页面刷新也能重新取得（否则刷新后插件前端全部拿不到凭证）。
///
/// **按 webview 分区**（2026-09-26）：本钩子对每个 webview（主窗口 / 终端窗口 / 未来任何
/// 独立窗口）都触发，凭证表按 label 分域——否则终端窗口加载会回收主窗口凭证，使主窗口插件面
/// 命令全数被拒（`缺少有效通道凭证`）。详见 `security::frontend_channel` 模块注释。
fn frontend_channel_session_hook() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("bedcode-frontend-channel")
        .on_page_load(|webview, payload| {
            let Some(plugin_host) = webview
                .app_handle()
                .try_state::<std::sync::Arc<crate::wasm_core::manager::host::PluginHost>>()
            else {
                // 页面加载早于 setup 装配 PluginHost（不应发生）：留痕不 panic
                tracing::warn!("[PluginChannel] page load before PluginHost managed, session not reset");
                return;
            };
            tracing::debug!(
                webview = %webview.label(),
                url = %payload.url(),
                "[PluginChannel] 页面加载，重置该窗口的前端通道会话"
            );
            plugin_host.reset_frontend_loader_session(webview.label(), "page-load");
        })
        .build()
}

/// 页面加载钩子每 webview 只应连接一次右键抑制信号
///
/// WebView 一旦创建，后续每次导航（刷新、路由跳转）都会再次触发页面加载钩子，而
/// `connect_context_menu` 是**追加**信号处理器，重复连接会在同一 WebView 上叠加。
/// 返回 `true` 表示首次见到该 label（应由本次调用去连接信号），`false` 表示已连过。
///
/// **去重键只能是 label，且必须随窗口销毁清理**：会话终端窗口的 label 是
/// `terminal-${session.id}`（`src/composables/useSessionWindows.ts`）——同一会话关窗后
/// 重开得到的是**全新 WebView 实例**，但 label 一模一样。若不随销毁清理，重开的窗口会被
/// 判成「已连过」而永不连接信号，Linux 正式版的原生右键菜单就在该窗口上复活。
/// 清理入口是 [`forget_context_menu_label`]（挂 `WindowEvent::Destroyed`）。
///
/// 锁中毒（其它线程 panic 时持有）按「未见过」处理并取回内部值：页面加载钩子跑在主线程，
/// 此处 panic 会连带拖垮窗口加载，而「抑制失败」只是退回默认右键行为，不该成为崩溃源。
#[cfg(target_os = "linux")]
fn mark_first_context_menu_connect(
    connected: &std::sync::Mutex<std::collections::HashSet<String>>,
    label: &str,
) -> bool {
    match connected.lock() {
        Ok(mut seen) => seen.insert(label.to_string()),
        Err(poisoned) => poisoned.into_inner().insert(label.to_string()),
    }
}

/// 窗口销毁 → 丢弃该 label 的去重记录（见 [`mark_first_context_menu_connect`]）
///
/// 只丢弃该 label 的条目，其他窗口（主窗口 / 其余终端窗口）不受影响。
/// 锁中毒时同样取回内部值继续清理，不 panic。
#[cfg(target_os = "linux")]
fn forget_context_menu_label(connected: &std::sync::Mutex<std::collections::HashSet<String>>, label: &str) {
    match connected.lock() {
        Ok(mut seen) => {
            seen.remove(label);
        }
        Err(poisoned) => {
            poisoned.into_inner().remove(label);
        }
    }
}

/// 右键抑制去重表：进程内单例（页面加载钩子与窗口销毁钩子共用一份）
#[cfg(target_os = "linux")]
static CONTEXT_MENU_CONNECTED: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
    std::sync::OnceLock::new();

/// 取去重表（首次调用时初始化）
#[cfg(target_os = "linux")]
fn context_menu_ledger() -> &'static std::sync::Mutex<std::collections::HashSet<String>> {
    CONTEXT_MENU_CONNECTED.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
}

/// 正式版（release）抑制 WebView 原生右键菜单 —— **Linux 专用**
///
/// Windows（WebView2）/ macOS（WKWebView）由前端 `src/main.ts` 在 capture 阶段
/// `preventDefault` 即可覆盖所有窗口；Linux 的 WebKitGTK 在 DOM `contextmenu` 的默认动作里
/// 弹原生菜单，JS `preventDefault` 拦不住（wry#30），只能连接 `context-menu` 信号并返回
/// `true` 阻止默认弹出。
///
/// **覆盖每个 webview**：钩子挂在页面加载上，主窗口与会话终端窗口（前端运行时创建的独立
/// `WebviewWindow`，见 `src/composables/useSessionWindows.ts`）等每次页面加载都会触发；
/// 早期实现只在 setup 里给 `main` 连一次信号，运行期新建的窗口仍会弹原生菜单。
///
/// dev 构建保留右键菜单（调试需要「检查元素」），故闭包内按 `debug_assertions` 提前返回；
/// 插件在 Linux 上照常注册，好让 debug 构建也做类型检查（release-only 代码路径不该是
/// 「只有发版才编译过」的黑盒）。
#[cfg(target_os = "linux")]
fn native_context_menu_guard() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use webkit2gtk::WebViewExt;

    let connected = context_menu_ledger();

    tauri::plugin::Builder::<tauri::Wry>::new("bedcode-native-context-menu-guard")
        .on_page_load(move |webview, _payload| {
            // dev 构建保留原生右键菜单，便于开发调试（检查元素等）
            if cfg!(debug_assertions) {
                return;
            }
            let label = webview.label().to_string();
            if !mark_first_context_menu_connect(&connected, &label) {
                return;
            }
            if let Err(e) = webview.with_webview(|platform_webview| {
                platform_webview.inner().connect_context_menu(|_, _, _, _| true);
            }) {
                // 仅释放开关功能失败，不影响启动；失败时该窗口保持默认右键行为
                tracing::warn!(error = %e, window = %label, "禁用右键菜单失败（WebKitGTK context-menu 信号连接失败）");
            }
        })
        .build()
}

pub fn run() {
    use tauri::Emitter;

    let app_start = AppStartTime(std::time::Instant::now());
    let start = app_start.0;

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(frontend_channel_session_hook());

    // Linux：正式版抑制 WebView 原生右键菜单的页面加载钩子（dev 注册但空转）
    #[cfg(target_os = "linux")]
    {
        builder = builder.plugin(native_context_menu_guard());
        // 窗口销毁即丢弃其去重记录：同一会话关窗后重开得到的是全新 webview 实例，
        // 但 label 复用（`terminal-${session.id}`），不清理则新窗口永不被连接信号。
        builder = builder.on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                forget_context_menu_label(context_menu_ledger(), window.label());
            }
        });
    }

    // WDIO 测试插件仅 debug 构建注册（release 不编译该依赖、不注册该插件）
    #[cfg(debug_assertions)]
    {
        builder = builder.plugin(tauri_plugin_wdio::init());
    }

    let app = builder
        .setup(move |app| {
            app.manage(app_start);

            // 平台条件默认窗口尺寸：tauri.conf.json 全局默认 1300×900 适配
            // Windows/macOS（字符渲染密度与 DPI 匹配），Linux（WebKitGTK）在
            // 此放大到 1560×1080（曾全局改大导致 Windows 默认窗口过大，见
            // b33e5d99 反例，改为仅 Linux 生效）。set_size 失败仅降级为全局
            // 默认小窗，不阻断启动。
            #[cfg(target_os = "linux")]
            {
                if let Some(win) = app.get_webview_window("main") {
                    // 目标窗口尺寸（逻辑像素）：Linux 下才放大，Windows/macOS 沿用全局默认
                    let target = tauri::LogicalSize::new(1560.0f64, 1080.0f64);
                    // 居中位置用目标尺寸一次性原子计算，替代 set_size 后 center()：
                    // center() 内部读 outer_size() 缓存，在 set_size 异步请求（tao
                    // window_requests 通道）执行前仍是旧值 1300×900，且 GTK resize
                    // 左上角锚定，导致窗口扩大后中心点偏向右下、补偿失效。
                    // 位置与尺寸同源于 target，与请求执行顺序无关。
                    let monitor = win
                        .current_monitor()
                        .ok()
                        .flatten()
                        .or_else(|| win.primary_monitor().ok().flatten());
                    if let Some(monitor) = monitor {
                        let scale = monitor.scale_factor();
                        let work_area = *monitor.work_area();
                        // 物理像素计算，语义与 tauri 内部 calculate_window_center_position
                        // 一致（work_area 居中，避开 Dock / 任务栏）
                        let target_phys = target.to_physical::<u32>(scale);
                        let x = (work_area.size.width as i32 - target_phys.width as i32) / 2 + work_area.position.x;
                        let y = (work_area.size.height as i32 - target_phys.height as i32) / 2 + work_area.position.y;
                        if let Err(e) = win.set_size(target) {
                            tracing::warn!(error = %e, "Linux 默认窗口尺寸调整失败，沿用全局默认");
                        }
                        if let Err(e) = win.set_position(tauri::PhysicalPosition::new(x, y)) {
                            // 定位失败不阻断启动：仅记录，窗口回落 WM 默认放置
                            tracing::warn!(error = %e, "Linux 默认窗口居中定位失败，沿用窗口管理器默认位置");
                        }
                    } else {
                        // 无显示器信息（极端环境）：仅调整尺寸，位置交给窗口管理器
                        if let Err(e) = win.set_size(target) {
                            tracing::warn!(error = %e, "Linux 默认窗口尺寸调整失败，沿用全局默认");
                        }
                    }
                }
            }

            // 注：正式版禁用右键原生菜单（Linux WebKitGTK context-menu 信号）由
            // native_context_menu_guard 插件的页面加载钩子统一处理，覆盖每个 webview。

            let app_handle = app.handle();

            // 启动早期日志通道：build_logging 之前（config 复制/加载、dev reset）的日志
            // 写入 bootstrap.log（release 构建启动失败证据不丢），init_logging 之后由
            // runtime 文件接管；初始化失败仅降级为控制台（eprintln），不阻断启动
            let log_dir = app_handle.path().app_log_dir().expect("Failed to get log directory");
            if let Err(e) = system::logging::bootstrap_init(&log_dir) {
                eprintln!("[logging] bootstrap channel init failed: {e}");
            }

            let config_path = app_handle
                .path()
                .app_data_dir()
                .expect("Failed to get app data dir")
                .join("config.properties");

            // 首次启动时从打包资源复制默认配置到 AppData
            // 后续启动直接使用 AppData 中的配置，用户修改不会丢失
            if !config_path.exists() {
                if let Ok(resource_path) = app_handle
                    .path()
                    .resolve("resources/config.properties", tauri::path::BaseDirectory::Resource)
                {
                    if resource_path.exists() {
                        if let Some(parent) = config_path.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        match std::fs::copy(&resource_path, &config_path) {
                            Ok(_) => system::logging::bootstrap_log(
                                tracing::Level::INFO,
                                format!("Default config copied from resource to {}", config_path.display()),
                            ),
                            Err(e) => system::logging::bootstrap_log(
                                tracing::Level::ERROR,
                                format!("Failed to copy default config: {e}, using built-in defaults"),
                            ),
                        }
                    }
                }
            }

            // 先加载配置，再初始化日志系统，使日志行为可配置
            let app_config = crate::system::config::AppConfig::load(&config_path).unwrap_or_else(|e| {
                system::logging::bootstrap_log(
                    tracing::Level::ERROR,
                    format!("Failed to load config, using defaults: {e}"),
                );
                crate::system::config::AppConfig::default()
            });

            // 初始化日志系统（依赖已加载的 LogConfig）
            init_logging(app.handle(), &app_config.log)?;

            // 初始化全局配置单例
            crate::system::config::AppConfig::init(app_config.clone());

            // 同步 PowerManager 开关状态到配置值
            crate::system::power::power_manager().set_enabled(app_config.network.prevent_sleep);

            // 启动电源唤醒监听：Windows 显示器长时间熄灭/锁屏后，WebView2 可能黑屏且不自愈，
            // 系统唤醒时强制窗口重绘（详见 system::power_wake 模块文档）
            crate::system::power_wake::spawn_wake_monitor(app_handle.clone());

            // 保存 resource_dir 供 AppContext 与插件资源加载使用
            let resource_dir = app_handle.path().resource_dir().expect("Failed to get resource dir");

            // 解析桌面端插件目录
            // dev 模式下 resolve 指向 target/debug/resources/...（Tauri 不自动复制资源）
            // 生产模式下 resolve 指向安装目录的 resources/...（打包时已包含）
            // 因此 dev 模式回退到源码目录
            let plugins_dir = {
                let resolved = app_handle
                    .path()
                    .resolve("resources/plugins/desktop", tauri::path::BaseDirectory::Resource)
                    .expect("Failed to resolve plugins directory");
                if resolved.exists() {
                    resolved
                } else {
                    // dev 模式 fallback：使用源码目录
                    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
                    let fallback = std::path::PathBuf::from(manifest_dir)
                        .join("resources")
                        .join("plugins")
                        .join("desktop");
                    tracing::info!(
                        "Plugin resolved path not found, falling back to source dir: {:?}",
                        fallback
                    );
                    fallback
                }
            };

            let ws_port = app_config.network.port;

            // 检查端口可用性
            let ws_port = match server::host_port::check_and_resolve_port(&app_handle, ws_port) {
                Ok(port) => port,
                Err(e) => {
                    tracing::error!("Port check failed: {}", e);
                    ws_port // 使用原端口，服务器启动会失败并记录日志
                }
            };

            let db_path = app_handle
                .path()
                .app_data_dir()
                .expect("Failed to get app data dir")
                .join("bedcode.db");

            if let Some(parent) = db_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // 对等网络节点身份：目录与 DB 同源解析自 app_data_dir（决策 D3 宿主只
            // 注入目录），node_identity.json 与 bedcode.db 并列存放；错误经 ? 上抛
            // 走既有启动失败路径——静默换身份会让对端可信列表全部失效（D3）
            let peer_net_data_dir = app_handle.path().app_data_dir().expect("Failed to get app data dir");
            bedcode_server_peer_net::init_node_identity(&peer_net_data_dir)?;

            // 对等网络节点状态容器（ticket 03）：节点生命周期按**属主**随插件启停
            // （审计票 12——插件经 host-peer.start-node / stop-node 自行请求，
            // 内核只记账；见 peer_net::start_node_owned / release_node_for。
            // 旧 setup 无条件自启与旧的按产品 id 开关外壳都已退役）
            // 状态以 Arc 托管（server-lib-split）：peer-net 引擎派生任务需要
            // 克隆句柄跨 await 存活（drive_gate / session_watch / 入站桥 /
            // 发现刷新订阅）；命令壳与 host_api 经 peer_ctx 一次装配
            app.manage(Arc::new(bedcode_server_peer_net::PeerNetState::default()));
            // peer-engine 状态仅保存当前引擎会话控制句柄与事件快照，不是业务持久化真源；
            // 任务、历史、设置均由 file-transfer 插件私有库持有。
            app.manage(Arc::new(bedcode_server_peer_net::peer_engine_transfer::PeerTransferState::default()));
            app.manage(Arc::new(bedcode_server_peer_net::peer_engine_receive::PeerReceiveState::default()));
            app.manage(Arc::new(bedcode_server_peer_net::peer_engine_remote::PeerRemoteState::default()));

            let db = Database::new(&db_path)?;
            db.init_schema()?;

            let db = Arc::new(Mutex::new(db));

            // v33（ADR 0033）：宿主**不再持有**入场签发密钥（签发/验签均在认证
            // 中心），故此处无密钥预生成。旧的 `plugin_secrets` 属主 `host` 行
            // （`('host','jwt.key')`）由 `db::run_migrations` 幂等清理（见
            // migrations 的 v33 条目）——那是不可达的旧密钥材料，留着只是白给的面。

            // ==================== 创建所有全局单实例 ====================

            // 采集系统基本信息（OS / 设备名称 / IP），挂载到 AppContext 供全局引用
            let system_info = Arc::new(system::info::SystemInfo::collect());

            let resource_dir_arc = Arc::new(resource_dir);
            // 票 11：内核会话管理器 / 配置管理器不再装配——会话真源在
            // `com.bedcode.terminal-session` 登记域，宿主只保留 `session_gateway`
            // 窄转发层（互调 api + `host-pty` 引擎）。
            // app_handle_arc 需在 plugin_host 之前创建，因为 PluginHost::new() 需要它构建 HostContextFns
            let app_handle_arc = Arc::new(app_handle.clone());
            // 用户插件目录（app_data_dir/plugins）：zip 安装的插件所在地（可卸载），
            // 与只读的内置目录（resource_dir/resources/plugins/desktop）分离
            let user_plugins_dir = app_handle
                .path()
                .app_data_dir()
                .expect("Failed to get app data dir")
                .join("plugins");
            // PluginHost::new 返回 Arc<Self>（属主失败回报端口需要宿主弱引用）
            // 整核抽出 §3.3：对等网络上下文装配端口由 lib 注入（peer_net_cmds::peer_ctx
            // 读 tauri managed state + server 端口装配；无头 harness 不传 → HEADLESS_UNAVAILABLE）
            let peer_ctx_provider: Option<Arc<crate::wasm_core::host_api::context::PeerCtxProvider>> =
                Some(Arc::new(crate::server::peer_net_cmds::peer_ctx));
            // 票 08/P0-2：第一方免弹窗归属清单（产品数据）由 lib 装配时注入——
            // 判定逻辑在 wasm-core（机制），清单真源在本文件即可（first_party_dirs.rs）
            let first_party_dirs = crate::first_party_dirs::first_party_dirs();
            let plugin_host = tauri::async_runtime::block_on(wasm_core::PluginHost::new(
                db.clone(),
                &plugins_dir,
                &user_plugins_dir,
                Some(app_handle_arc.clone()),
                peer_ctx_provider,
                first_party_dirs,
            ));
            // 注入消息总线 dispatcher（两阶段初始化）
            tauri::async_runtime::block_on(plugin_host.init_message_bus());
            let mdns_advertiser = Arc::new(tokio::sync::RwLock::new(
                bedcode_discovery_engine::advertiser::MdnsAdvertiser::new(),
            ));

            // 终端同步事件通道已随 websocket 业务下沉票 08 删除（插件事件改 bus+emit，
            // 宿主不再持有 HostSyncEvent / broadcast-sync 广播面）

            // ==================== 注册到 AppContext 全局容器 ====================

            let _ctx = system::app_context::AppContextBuilder::new()
                .db(db.clone())
                .plugin_host(plugin_host.clone())
                .mdns_advertiser(mdns_advertiser.clone())
                .app_handle(Some(app_handle_arc.clone()))
                .resource_dir(resource_dir_arc.clone())
                .system_info(system_info.clone())
                .build_and_init();

            // server-lib-split：装配服务器端口（宿主壳实现注入；supervisor /
            // ws / http / peer-net 的「无 AppContext」保守分支依赖本注册表）。
            // 走组合根的单一装配点：无头 harness（cross-end-tests）调同一函数。
            // 句柄直传：插件激活发生在上面的 `PluginHost::new` 内部，早于 AppContext
            // 注册，提前装配的端口面靠它解析路径（2026-10-07 修：此前窗口内的
            // `host-peer.start-node` 恒定报 `resolve app data dir failed: no runtime
            // context`，peer 节点起不来 → 桌面端不广播 → 移动端发现不到）
            crate::server::composition::install_server_ports_with(Some(app_handle_arc.clone()));

            // 同时注册到 Tauri State（前端 invoke 可用）
            app.manage(db.clone());
            app.manage(mdns_advertiser.clone());
            app.manage(plugin_host.clone());
            app.manage(plugin_host.wasm_runtime().fs_auth().clone());
            // 网络出站授权应答通道（票 05）：与 fs 应答同一形态——前端弹窗经
            // 宿主面凭证代答，故应答器必须是进程级单例（与事件广播的询问一一对应）
            app.manage(plugin_host.wasm_host_ctx().net_auth().clone());
            // peer-net 节点的启动不再需要 boot 对账（审计票 12）：旧实现要在装配末尾
            // 按硬编码插件 id 补一次状态对账，因为 activate 外壳经
            // `AppContext::try_global()` 取句柄，而 boot 装配期全局尚未注册
            // （2026-09-06 实机实证：已激活插件的节点不随 boot 启动）。现在改由插件
            // 在自己的 activate 里调 `host-peer.start-node`，宿主从 WasmHostContext
            // 的 app_handle 字段取句柄（PluginHost::new 之前就早已就位，不依赖全局），
            // 所以 boot 期直接起得来，那条对账连同它的产品耦合一起退役
            // ==================== 开发模式：启动插件文件监听 ====================
            // 仅 debug 构建启用，监听插件产物变化触发热重载
            // notify 回调在非 Tokio 线程中运行，必须通过 Handle::spawn 而非 tokio::spawn
            // setup 闭包不在 Tokio runtime 上下文中，需通过 block_on 获取 Handle
            #[cfg(debug_assertions)]
            {
                let runtime_handle = tauri::async_runtime::block_on(async { tokio::runtime::Handle::current() });
                let _dev_watcher = wasm_core::watcher::PluginDevWatcher::start(
                    plugins_dir.to_path_buf(),
                    runtime_handle,
                    // 整核抽出：watcher 不再经 AppContext::global() 取宿主，改由
                    // bootstrap 注入弱引用（plugin_host 在此处已创建，见上）
                    Arc::downgrade(&plugin_host),
                );
                // dev_watcher 需要 hold 住生命周期，存入 AppContext 或 leak
                // 使用 Box::leak 使 watcher 生命周期与进程一致（开发模式可接受）
                Box::leak(Box::new(_dev_watcher));
                tracing::info!("Plugin dev watcher enabled (debug build)");
            }

            // ==================== 启动服务器（通过 ServerSupervisor）====================

            // 链路加密装配句柄（issue 01）：init_at_startup 需访问数据目录与 DB 状态
            let link_crypto_app_handle = app_handle.clone();
            let supervisor = bedcode_server_core::supervisor::ServerSupervisor::global();
            let ws_port_for_spawn = ws_port;
            // 产品决策：服务器永久自启动，不再可配置（本地功能依赖此服务，
            // 见 ServerSupervisor 类注释；config 中 network.auto_start 已废弃）
            let auto_start = true;
            tauri::async_runtime::spawn(async move {
                // 链路加密先于服务器启动装配：第一条流量就要被开关裁决（spec §6）；
                // 身份损坏时强制回退全关，不阻断启动。core 不认 AppHandle/Database，
                // 数据目录与主库连接由宿主组合根薄壳解析后传入
                server::composition::init_link_crypto_at_startup(&link_crypto_app_handle).await;

                supervisor.init_config(ws_port_for_spawn, auto_start).await;

                let ws_manager = bedcode_server_websocket::WebSocketManager::global();
                ws_manager.init().await.expect("Failed to initialize WebSocketManager");

                if auto_start {
                    tracing::info!("[BedCode] Auto-starting server on port {}", ws_port_for_spawn);
                    match supervisor.start(ws_port_for_spawn).await {
                        Ok(_) => tracing::info!("[BedCode] Server started successfully"),
                        Err(e) => tracing::error!("[BedCode] Server failed to start: {}", e),
                    }
                } else {
                    tracing::info!("[BedCode] Server auto-start disabled, waiting for manual start");
                }
            });

            // 写入端口文件
            let app_handle_clone = app_handle.clone();
            let ws_port_copy = ws_port;
            tauri::async_runtime::spawn(async move {
                let port_file = app_handle_clone
                    .path()
                    .app_data_dir()
                    .ok()
                    .map(|p| p.join("bedcode-port.txt"));

                if let Some(port_file) = port_file {
                    if let Some(parent) = port_file.parent() {
                        let _ = tokio::fs::create_dir_all(parent).await;
                    }
                    if let Err(e) = tokio::fs::write(&port_file, ws_port_copy.to_string()).await {
                        tracing::warn!("Failed to write port file: {}", e);
                    } else {
                        tracing::info!("Wrote port file: {}", port_file.display());
                    }
                }
            });

            // 票 09：原先在这里启动的「SessionManager 状态事件 → 前端
            // `session-status-changed`」转发器已退役（订阅源对插件会话无流量、
            // 前端零消费方），见 `events.rs` 模块头。

            setup_tray(app_handle)?;

            let window = app_handle
                .get_webview_window("main")
                .expect("Failed to get main window");
            let close_window = window.clone();
            let close_app_handle = app_handle.clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    // 始终先阻止默认关闭，避免 block_on 死锁
                    // 在同步回调中使用 block_on 会在 Tokio 运行时繁忙时死锁，
                    // 因此改为先阻止关闭，再 spawn 异步任务检查钩子
                    api.prevent_close();

                    let win = close_window.clone();
                    let ah = close_app_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        let should_close = system::lifecycle::lifecycle_registry().run_window_close_hooks().await;

                        if should_close {
                            // 无运行中会话，直接关闭
                            if let Err(e) = win.destroy() {
                                tracing::error!("Failed to destroy window: {}", e);
                            }
                        } else {
                            // 有运行中会话，通知前端弹窗确认。payload = 插件登记域视图
                            // 数组原样透传（`session-list {filter:"running"}`）——「哪些状态
                            // 算运行中需要确认」的判据在插件会话域（`ops::needs_close_confirmation`，
                            // 2026-09-25 下沉），宿主零字段读取只取 `sessions` 数组做 emit；
                            // **失败回退空列表**——关闭路径不能被插件异步调用阻塞
                            // （2026-09-23 用户定案）
                            let ctx = system::app_context::AppContext::global();
                            let host_ctx = ctx.plugin_host().wasm_host_ctx();
                            let running = match crate::utils::session_gateway::running_views(host_ctx).await {
                                Ok(reply) => reply.get("sessions").and_then(|s| s.as_array()).cloned().unwrap_or_else(|| {
                                    tracing::warn!(error = %reply, "window close payload: reply missing sessions array, fallback to empty list");
                                    Vec::new()
                                }),
                                Err(e) => {
                                    tracing::warn!(
                                        error = %e,
                                        "window close payload: plugin running list unavailable, fallback to empty list"
                                    );
                                    Vec::new()
                                }
                            };

                            tracing::info!(
                                count = running.len(),
                                "Window close requested with {} running session(s), emitting to frontend",
                                running.len()
                            );

                            if let Err(e) = ah.emit(system::constants::WINDOW_CLOSE_REQUESTED, &running) {
                                tracing::error!(error = %e, "Failed to emit window-close-requested");
                            }
                        }
                    });
                }
            });

            let init_elapsed = start.elapsed();
            tracing::info!(
                "BedCode Desktop initialized - WebSocket server on port {} (后端初始化耗时: {}ms)",
                ws_port,
                init_elapsed.as_millis()
            );

            // 注册核心模块的生命周期钩子（Shutdown/WindowClose）
            system::lifecycle::register_core_lifecycle_hooks();
            system::lifecycle::register_window_close_hooks();

            // 触发 Startup 钩子
            tauri::async_runtime::spawn(async move {
                system::lifecycle::lifecycle_registry().run_startup_hooks().await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Session / PTY Input：**会话命令面已整体注销**（票 08）——
            // 列表 / 单查 / 尺寸 / 写入 / 特殊键五条连壳删除，消费方改指插件
            // 命令面（`session.list` / `session.get` / `session.action.resize` /
            // `session.input`），见 commands.rs 会话命令节头部注释
            // 终端输出面（票 05 摘除）：旧 Channel 传输命令
            // （subscribe_terminal_channel / unsubscribe_terminal_channel /
            // terminal_channel_ack，commands/terminal_stream.rs）已随宿主前端
            // 消费方摘除——插件输出改经 host-session.output-ring-fetch 原语
            // （插件命令面 session.output.pull）拉取，宿主不留降级输出传输
            // 配对 / QR / 连接历史命令面已注销：产品面归 com.bedcode.terminal-session 插件的
            // session.pairing.* / session.qr.* / session.devices.* / session.history.*
            // （凭据签发与 `pairings` 表仍在内核 auth 模块，宿主只留原语与记录面；
            // 见 .scratch/2026-09-21-host-rust-residue/issues/05）
            commands::set_log_level,
            commands::save_log_settings,
            commands::open_log_dir,
            commands::open_external_url,
            // Updater（外壳命令：updater:default 权限已撤除，发起权收归 Rust）
            commands::check_for_update,
            commands::install_update,
            // Settings
            commands::get_app_settings,
            commands::save_app_settings,
            commands::set_terminal_bg_image,
            // Utility
            commands::ping,
            commands::get_app_version,
            commands::get_startup_time,
            commands::confirm_window_close,
            // Dev Console Relay（仅 dev：前端 console 日志转发，写 runtime.*.log + frontend.*.log 单独文件，见 commands.rs Dev Console Log Relay 节）
            #[cfg(debug_assertions)]
            commands::report_frontend_log,
            // 桥接基准 · Channel 传输面（仅 debug：AGENTS §5.1 的 debug-only 测试面，
            // 零业务语义、不碰既有能力；见 src/bench_channel.rs 头注释与 bench/README.md）
            #[cfg(debug_assertions)]
            bench_channel::bench_channel_stream_bytes,
            #[cfg(debug_assertions)]
            bench_channel::bench_channel_stream_text,
            #[cfg(debug_assertions)]
            bench_channel::bench_channel_stream_raw,
            // Plugin
            commands::plugin_list_loaded,
            commands::plugin_get_info,
            commands::plugin_preauthorize,
            commands::plugin_activate,
            commands::plugin_deactivate,
            commands::plugin_approve,
            commands::plugin_frontend_loader_session,
            commands::plugin_channel_token,
            commands::plugin_install_from_file,
            commands::plugin_uninstall,
            commands::plugin_mark_error,
            commands::plugin_frontend_load_report,
            commands::plugin_get_activated_state,
            commands::plugin_storage_get,
            commands::plugin_storage_set,
            commands::plugin_storage_delete,
            // `plugin_terminal_send_input` 随票 08 注销：终端输入改走插件
            // 命令通道 `session.input`（宿主不再替插件导流输入）
            commands::plugin_list_commands,
            commands::plugin_list_views,
            commands::plugin_find_file_handler,
            commands::plugin_invoke,
            commands::plugin_list_rust_commands,
            commands::plugin_dev_reload,
            commands::plugin_fs_auth_respond,
            commands::plugin_network_auth_respond,
            commands::plugin_auth_overview,
            commands::plugin_auth_set_strategy,
            commands::plugin_auth_revoke,
            commands::plugin_auth_remove_record,
            // Server
            commands::server_start,
            commands::server_stop,
            commands::server_restart,
            commands::get_server_status,
            commands::get_server_metrics,
            commands::get_server_network_config,
            commands::update_server_port,
            commands::update_server_auto_start,
            commands::update_server_network_config,
            commands::get_traffic_encryption_config,
            commands::set_traffic_encryption_config,
            commands::get_link_crypto_fingerprint,
            commands::reset_server_network_config,
            // Peer Net（命令壳在 peer_net_cmds——引擎实现不持有 AppHandle，
            // server-lib-split）
            peer_net_cmds::start_peer_node,
            peer_net_cmds::stop_peer_node,
            // Phase 4（issue 13）：对等网络产品面已整体迁入 file-transfer 插件
            // （WIT host-peer 13 原语），主前端命令面退役——仅保留生命周期、
            // 首连确认与信任管理（宿主级兜底路径）。其余查询/管理命令的函数体
            // 暂留一版（部分仍为 host_impl 内部簿记调用），下版本删除。
            peer_net_cmds::respond_peer_consent,
            peer_net_cmds::list_trusted_peers,
            peer_net_cmds::revoke_trusted_peer,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    // 使用 .build() + .run() 替代 .run()，以接入 Tauri RunEvent 循环
    // RunEvent::ExitRequested 是执行优雅关闭的最后时机
    app.run(move |_app_handle, event| match event {
        tauri::RunEvent::ExitRequested { .. } => {
            tracing::info!("BedCode Desktop exit requested, running shutdown hooks...");
            tauri::async_runtime::block_on(async {
                system::lifecycle::lifecycle_registry().run_shutdown_hooks().await;
            });
        }
        tauri::RunEvent::Exit { .. } => {
            tracing::info!("BedCode Desktop exited");
        }
        _ => {}
    });
}

/// Setup system tray
fn setup_tray(app: &tauri::AppHandle) -> Result<()> {
    use tauri::{
        menu::{Menu, MenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    };

    let show_item = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
    let hide_item = MenuItem::with_id(app, "hide", "隐藏窗口", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&show_item, &hide_item, &quit_item])?;

    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "hide" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            "quit" => {
                // 尝试关闭主窗口（触发 CloseRequested → 生命周期钩子 → 确认弹窗）
                // 如果窗口已隐藏，先显示再关闭
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                    let _ = window.close();
                } else {
                    // 无窗口时直接退出
                    app.exit(0);
                }
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    tracing::info!("System tray initialized");
    Ok(())
}
