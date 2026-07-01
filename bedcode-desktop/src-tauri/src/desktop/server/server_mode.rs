//! Server-Only Mode
//!
//! 当 BedCode 以 --server-only 参数启动时，运行此模式
//! 启动 Actix Web (HTTP + WS) + IPC handler + 心跳上报

use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use actix_web::{web, App, HttpServer};
use actix_cors::Cors;
use actix_web_actors::ws as actix_ws;

use crate::desktop::server::controllers::{
    auth_controller, session_controller, config_controller, file_controller,
    plugin_controller, git_controller,
};
use crate::desktop::server::ws::terminal_ws::TerminalWs;
use crate::desktop::server::ipc::{IpcCommand, IpcResponse};
use crate::desktop::server::metrics::MetricsCollector;

/// WS 握手端点
async fn terminal_ws(
    req: actix_web::HttpRequest,
    stream: web::Payload,
) -> Result<actix_web::HttpResponse, actix_web::Error> {
    let addr = req.peer_addr().unwrap_or_else(|| "0.0.0.0:0".parse().unwrap());
    let ws_actor = TerminalWs::new(addr);
    actix_ws::start(ws_actor, &req, stream)
}

/// 构建路由配置（复用主模式的路由）
pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/ws/terminal", web::get().to(terminal_ws));

    cfg.service(
        web::scope("/api/auth")
            .route("/pairing", web::post().to(auth_controller::request_pairing))
            .route("/verify", web::post().to(auth_controller::verify_pairing_code))
            .route("/qr-connect", web::post().to(auth_controller::qr_connect))
            .route("/reauth", web::post().to(auth_controller::reauthenticate))
    );

    cfg.service(
        web::scope("/api")
            .route("/sessions", web::get().to(session_controller::list_sessions))
            .route("/sessions/start", web::post().to(session_controller::start_session))
            .route("/sessions/{id}/stop", web::post().to(session_controller::stop_session))
            .route("/sessions/{id}/resize", web::post().to(session_controller::resize_session))
            .route("/sessions/{id}/input", web::post().to(session_controller::send_session_input))
            .route("/sessions/{id}/remove", web::delete().to(session_controller::remove_session))
            .route("/configs", web::get().to(config_controller::list_configs))
            .route("/quick-actions", web::get().to(config_controller::list_quick_actions))
            .route("/file-tree", web::post().to(file_controller::get_file_tree))
            .route("/file-content", web::post().to(file_controller::get_file_content))
            .route("/diff-tree", web::post().to(file_controller::get_diff_tree))
            .route("/file-diff", web::post().to(file_controller::get_file_diff))
            .route("/git/branches", web::get().to(git_controller::get_branches))
            .route("/git/checkout", web::post().to(git_controller::checkout))
    );

    cfg.route("/plugin/task-status", web::post().to(plugin_controller::update_task_status));
    cfg.route("/plugin/session-mode", web::post().to(plugin_controller::set_session_mode));
    cfg.route("/plugin/session-mode", web::get().to(plugin_controller::get_session_mode));
}

/// 采集当前系统指标（CPU/内存/连接数）
fn collect_system_metrics(sys: &mut sysinfo::System) -> (usize, f64, u64) {
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    let cpu_percent = sys.global_cpu_usage();
    let memory_bytes = sys.used_memory();

    let connections = match tokio::runtime::Handle::try_current() {
        Ok(handle) => handle.block_on(async {
            crate::desktop::server::ws::registry::WsSessionRegistry::global()
                .client_count()
                .await
        }),
        Err(_) => 0,
    };

    (connections, cpu_percent as f64, memory_bytes)
}

/// 运行服务器模式
pub fn run_server_mode() {
    // 初始化简单日志（输出到 stderr，主进程可转发）
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter("info")
        .init();

    tracing::info!("BedCode server-only mode starting...");

    // 共享 sysinfo 采集器（心跳线程和 IPC 循环共用）
    let sys = Arc::new(Mutex::new(sysinfo::System::new()));

    // 从 stdin 读取初始命令（阻塞等待 start 命令）
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut reader = std::io::BufReader::new(stdin.lock());

    let mut port: u16 = 8765;

    // 读取第一条命令，期望是 Start
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            tracing::error!("IPC stdin closed before start command");
            return;
        }

        match serde_json::from_str::<IpcCommand>(line.trim()) {
            Ok(IpcCommand::Start { port: p }) => {
                port = p;
                break;
            }
            Ok(cmd) => {
                tracing::warn!("Unexpected IPC command before start: {:?}", cmd);
            }
            Err(e) => {
                tracing::warn!("Failed to parse IPC command: {}", e);
            }
        }
    }

    // 启动 Actix Web 服务器
    let port_for_server = port;
    let server_handle = std::sync::Arc::new(std::sync::Mutex::new(None::<actix_web::dev::ServerHandle>));

    let handle_clone = server_handle.clone();
    std::thread::spawn(move || {
        let rt = actix_rt::Runtime::new().expect("Failed to create Actix runtime");
        rt.block_on(async move {
            tracing::info!("Starting Actix Web server on port {}", port_for_server);

            let server_result = HttpServer::new(|| {
                let cors = Cors::default()
                    .allow_any_origin()
                    .allow_any_method()
                    .allow_any_header()
                    .max_age(3600);

                App::new()
                    .wrap(cors)
                    .wrap(actix_web::middleware::Logger::default())
                    .configure(configure_routes)
            })
            .bind(format!("0.0.0.0:{}", port_for_server));

            match server_result {
                Ok(s) => {
                    let server_future = s.run();
                    // 在 await 之前获取 handle，用于后续优雅停机
                    let handle = server_future.handle();
                    *handle_clone.lock().unwrap() = Some(handle);

                    // 通知主进程服务器已启动
                    let resp = IpcResponse::Started { port: port_for_server };
                    if let Ok(json) = resp.to_json_line() {
                        let mut out = std::io::stdout();
                        let _ = out.write_all(json.as_bytes());
                        let _ = out.flush();
                    }

                    if let Err(e) = server_future.await {
                        tracing::error!("Actix Web server error: {}", e);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to bind port {}: {}", port_for_server, e);
                    let resp = IpcResponse::Error { message: e.to_string() };
                    if let Ok(json) = resp.to_json_line() {
                        let mut out = std::io::stdout();
                        let _ = out.write_all(json.as_bytes());
                        let _ = out.flush();
                    }
                }
            }
        });
    });

    // 启动心跳任务（采集 sysinfo 指标 + WS 连接数，通过 IPC stdout 上报主进程）
    let sys_heartbeat = sys.clone();
    let heartbeat_stdout = std::io::stdout();
    std::thread::spawn(move || {
        let mut out = heartbeat_stdout;
        loop {
            std::thread::sleep(Duration::from_secs(5));

            let (connections, cpu_percent, memory_bytes) = {
                let mut sys = sys_heartbeat.lock().unwrap();
                collect_system_metrics(&mut sys)
            };

            let collector = MetricsCollector::global();
            let metrics = collector.sample(connections, cpu_percent, memory_bytes);

            let resp = IpcResponse::Heartbeat(Box::new(metrics));
            if let Ok(json) = resp.to_json_line() {
                let _ = out.write_all(json.as_bytes());
                let _ = out.flush();
            }
        }
    });

    // IPC 命令读取循环
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            tracing::info!("IPC stdin closed, shutting down...");
            break;
        }

        match serde_json::from_str::<IpcCommand>(line.trim()) {
            Ok(IpcCommand::Stop) => {
                tracing::info!("Received stop command, shutting down...");
                if let Some(handle) = server_handle.lock().unwrap().take() {
                    let _ = handle.stop(true);
                }
                let resp = IpcResponse::Stopped;
                if let Ok(json) = resp.to_json_line() {
                    let _ = stdout.write_all(json.as_bytes());
                    let _ = stdout.flush();
                }
                break;
            }
            Ok(IpcCommand::GetMetrics) => {
                let (connections, cpu_percent, memory_bytes) = {
                    let mut sys_guard = sys.lock().unwrap();
                    collect_system_metrics(&mut sys_guard)
                };

                let collector = MetricsCollector::global();
                let metrics = collector.sample(connections, cpu_percent, memory_bytes);
                let resp = IpcResponse::Metrics(Box::new(metrics));
                if let Ok(json) = resp.to_json_line() {
                    let _ = stdout.write_all(json.as_bytes());
                    let _ = stdout.flush();
                }
            }
            Ok(IpcCommand::Start { .. }) => {
                // 已启动，忽略重复 start
            }
            Err(e) => {
                tracing::warn!("Failed to parse IPC command: {}", e);
            }
        }
    }

    tracing::info!("BedCode server-only mode exiting");
}
