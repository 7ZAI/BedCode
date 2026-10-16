//! 错误信封跨边界集成测试（票 01；票 05 启用；server-lib-split 票 08 曾迁入
//! `bedcode-server-base`，crate 单测纯净性轮次迁回宿主）
//!
//! 验证对象：Rust `AppError` → Tauri IPC rejection 的真实序列化链路，模拟前端 `invoke`
//! rejection 收到的形状（ADR 0030 契约单一事实源 `docs/adr/0030*`）。
//!
//! 链路依据（tauri 2.11 `ipc/`）：命令返回 `Err(AppError)` → `From<T: Serialize> for InvokeError`
//! 做 `serde_json::to_value` → `InvokeResponse::Err(InvokeError(pub Value))` → body =
//! `serde_json::to_vec(&e.0)`。故前端 rejection 收到的就是 AppError Serialize 的裸信封对象。
//!
//! 本文件用例在前端单元测试（信封序列化 `system/error.rs` mod tests）之外，额外锁两件事：
//! ① InvokeError 包裹路径不改变信封形状（code / request_id / params 直达 rejection）；
//! ② 老字符串形状（如参数解析类错误，tauri 走 `InvokeError::from_error` 的 to_string）
//!    仍需被前端 parseInvokeError 兜底为 host.internal —— 形状矩阵锁在前端测试。
//!
//! ## 为什么住在宿主 `tests/` 而不是 `bedcode-server-base`（crate 单测纯净性）
//!
//! 本文件是 **Cargo 集成测试 target**（`tests/*.rs` = 独立二进制，只能经 `pub` API
//! 访问被测 crate），锁的又是**跨越 crate 边界的契约**——`AppError`（定义在
//! `bedcode-server-base`）× Tauri IPC 序列化层（宿主框架侧）。按「拆分产物 crate 只保留
//! 单元测试、跨 crate 集成测试归宿主」的纯净性口径，它住在 crate 里本身就是越线形态：
//! crate 根多一个 `tests/` 目录 = 该 crate 多一个只测对外行为的可编译面，而它的对外
//! 行为面理应由宿主来验。
//!
//! 票 08 当初把它迁进 crate 的理由（宿主 `system/error.rs` 只是
//! `pub use bedcode_server_base::error::{…}` 的再导出，经 `bedcode_desktop_lib::system::error`
//! 取类型等于开一条「只在测试里存在」的宿主→server-lib 依赖方向）**已不成立**：宿主清单
//! 本来就直接声明 `bedcode-server-base`，本文件直接 `use bedcode_server_base::error::AppError`
//! 打定义处，不再经任何再导出垫片，也不再给 crate 引入一个集成测试二进制。
//!
//! 防复发：`tests/capability_crates_unit_tests_only.rs`（拆分产物不得有 crate 根
//! `tests/`，也不得有 dev-only 内部依赖）。
//!
//! 依赖面零新增：`tauri` / `serde_json` / `bedcode-server-base` 都是宿主 `[dependencies]` 已有项。

use bedcode_server_base::error::AppError;
use serde_json::json;

/// 模拟整条「命令 Err → IPC rejection 响应体」：信封形状必须直达前端。
#[test]
fn invoke_rejection_body_is_envelope_object() {
    // 未映射变体 → host.internal 兜底
    let err = AppError::Internal("connection refused".into());
    let invoke_err = tauri::ipc::InvokeError::from(err);
    let body = serde_json::to_value(&invoke_err.0).expect("InvokeError body 必须可序列化");
    assert!(body.is_object(), "rejection 必须是信封对象: {body:?}");
    assert_eq!(body["code"], json!("host.internal"));
    let rid = body["request_id"].as_str().expect("request_id 必须存在");
    assert_eq!(rid.len(), 8);
    assert!(!body.as_object().unwrap().contains_key("detail"), "信封不得携带 detail");
}

/// UserFacing 的 code / params 必须原样到达 rejection。
#[test]
fn user_facing_fields_reach_rejection_unmodified() {
    let err = AppError::user_facing(
        "host.plugin.trap",
        json!({ "name": "ai-chatbox" }),
        "wasm trap detail (should not leak)",
    );
    let invoke_err = tauri::ipc::InvokeError::from(err);
    let body = serde_json::to_string(&invoke_err.0).unwrap();
    assert!(body.contains("\"code\":\"host.plugin.trap\""), "code 透传: {body}");
    assert!(body.contains("\"name\":\"ai-chatbox\""), "params 透传: {body}");
    assert!(!body.contains("wasm trap"), "detail 泄漏进 rejection: {body}");
}

/// 入站方向形状锁：老字符串 rejection（遗留/参数解析类）由前端 parseInvokeError 兜底，
/// 该兜底矩阵在前端 `src/__tests__/utils/userError.test.ts`；此处锁字符串确实可达边界。
#[test]
fn legacy_string_rejection_reaches_boundary() {
    let legacy =
        tauri::ipc::InvokeError::from_error(std::io::Error::new(std::io::ErrorKind::Other, "legacy plain string"));
    let body = serde_json::to_value(&legacy.0).unwrap();
    assert_eq!(body, json!("legacy plain string"));
}
