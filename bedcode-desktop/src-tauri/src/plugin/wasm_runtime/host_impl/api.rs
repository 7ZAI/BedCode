//! 插件互调宿主原语（ADR-0017）：`host-api-call` 的实现
//!
//! 语义：发布 JSON-RPC 请求到 `bedcode.api.<api>` topic（经 [`super::bus`]
//! 的门禁校验），然后阻塞等待回复。回复 topic 为
//! `bedcode.api.reply.<caller-plugin-id>.<request-id>`（spec §9.3），
//! 其中 caller-id 取调用方插件 ID（host function 的 Caller state），
//! request-id 取请求载荷中的字符串 `id` 字段（SDK 生成，JSON-RPC 约定）。
//!
//! ## 为什么回复订阅是宿主侧静态订阅
//!
//! WASM guest 是同步执行模型：调用方阻塞等待期间，宿主无法把回复投递到
//! 调用方插件的 `on_message` —— 投递路径（`with_wasm_plugin_call`）需要锁住
//! 调用方实例，而该锁正被阻塞中的调用持有，必然自锁死锁。因此宿主在
//! 回复 topic 上注册静态订阅（Rust callback 投递，不经过调用方 wasm 实例），
//! 经 oneshot 通道唤醒等待者。消息形状（JSON-RPC）与 correlation id 生命周期
//! 归 SDK 管理，本原语只是「发布请求、等待回复」的通用管道。

use super::bus;
use crate::plugin::message_bus::{BusMessageHandler, MessageBus};
use crate::plugin::wasm_runtime::{block_on_async, WasmHostContext};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};

/// 回复订阅处理器：收到回复即投递到 oneshot 通道（仅取第一条）
///
/// Sender 包在 `Arc<Mutex<Option<_>>>` 中：`take()` 保证只消费一条回复
/// （迟到的重复回复直接丢弃），Mutex 提供 `Sync`（oneshot::Sender 非 Sync，
/// 而 BusMessageHandler 要求 Send + Sync）
struct ReplyHandler {
    tx: Arc<Mutex<Option<oneshot::Sender<String>>>>,
}

impl BusMessageHandler for ReplyHandler {
    fn on_message(&self, msg: &bedcode_plugin_api::BusMessage) -> anyhow::Result<()> {
        let mut tx = self.tx.try_lock().map_err(|_| {
            anyhow::anyhow!("api_call: reply handler lock poisoned")
        })?;
        if let Some(tx) = tx.take() {
            let _ = tx.send(msg.payload.to_string());
        }
        Ok(())
    }
}

/// 插件互调调用：发布请求（经门禁）+ 阻塞等待回复（宿主侧订阅 + 超时）
///
/// 返回回复 JSON 字符串（`{ jsonrpc, id, result | error }`），并校验回复
/// `id` 与请求一致（防错配回复串台）。超时/门禁拒绝返回 Err。
pub(crate) fn api_call(
    host_ctx: &WasmHostContext,
    caller_id: &str,
    request_topic: &str,
    payload_json: &str,
    timeout_ms: u64,
) -> Result<String, String> {
    // 1. 解析请求载荷并提取 correlation id（回复 topic 路由依据）
    let payload: Value = serde_json::from_str(payload_json)
        .map_err(|e| format!("api_call: invalid JSON payload: {}", e))?;
    let request_id = payload
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|id| !id.is_empty())
        .ok_or_else(|| "api_call: request payload must contain a non-empty string 'id'".to_string())?;

    let reply_topic = format!("bedcode.api.reply.{}.{}", caller_id, request_id);
    let bus = host_ctx.message_bus.clone();
    let (tx, rx) = oneshot::channel::<String>();
    let handler = ReplyHandler {
        tx: Arc::new(Mutex::new(Some(tx))),
    };

    // 2. 注册静态回复订阅（先订阅后发布：oneshot 缓冲保证时序安全）
    let sub_bus = bus.clone();
    let sub_topic = reply_topic.clone();
    let sub_result = block_on_async(async move {
        sub_bus
            .subscribe_static(caller_id, &sub_topic, Box::new(handler))
            .await;
        Ok::<(), String>(())
    });
    sub_result.map_err(|e| format!("api_call: subscribe failed: {}", e))?;

    // 3. 发布请求（bus_publish 内含 JSON 校验 + bedcode.api.* 门禁校验，
    //    未声明目标在此被拒，错误直接透传给调用方）
    bus::bus_publish(host_ctx, caller_id, request_topic, payload_json)?;

    // 4. 等待回复（超时）
    let wait_result = block_on_async(async move {
        tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), rx).await
    });
    let reply_json = match wait_result {
        Ok(Ok(reply)) => reply,
        Ok(Err(_)) => {
            return Err(format!(
                "api_call: reply channel dropped before response on '{}'",
                reply_topic
            ));
        }
        Err(_) => {
            // 超时路径同样清理订阅（见下方清理），保持回复 topic 无残留
            cleanup_reply_subscription(&bus, caller_id, &reply_topic);
            return Err(format!(
                "api_call: timeout after {}ms waiting for reply on '{}'",
                timeout_ms, reply_topic
            ));
        }
    };

    // 5. 校验回复 id 与请求一致（防迟到的旧回复/错配回复串台）
    let reply: Value = serde_json::from_str(&reply_json)
        .map_err(|e| format!("api_call: invalid reply JSON: {}", e))?;
    let reply_id = reply.get("id").and_then(|v| v.as_str());
    if reply_id != Some(request_id) {
        cleanup_reply_subscription(&bus, caller_id, &reply_topic);
        return Err(format!(
            "api_call: reply id mismatch (expected '{}', got {:?}) on '{}'",
            request_id, reply_id, reply_topic
        ));
    }

    // 6. 清理回复订阅（幂等）
    cleanup_reply_subscription(&bus, caller_id, &reply_topic);
    Ok(reply_json)
}

/// 移除回复 topic 上的订阅（幂等；超时/错误路径共用）
fn cleanup_reply_subscription(bus: &Arc<MessageBus>, caller_id: &str, reply_topic: &str) {
    let bus = bus.clone();
    let caller = caller_id.to_string();
    let topic = reply_topic.to_string();
    if let Err(e) = block_on_async(async move {
        bus.unsubscribe(&caller, &topic).await;
        Ok::<(), String>(())
    }) {
        tracing::warn!(
            error = %e,
            topic = %reply_topic,
            "api_call: reply subscription cleanup failed"
        );
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::wasm_runtime::host_impl::tests::build_host_ctx;
    use std::time::Duration;

    /// 静态响应者：收到请求 topic 消息后，向回复 topic 回一条 JSON-RPC 回复。
    /// 通过 channel 把收到的请求转发给测试体（断言请求内容）。
    struct Responder {
        request_tx: tokio::sync::mpsc::UnboundedSender<String>,
        /// 回复 id 覆盖（None = 原样回传；Some = 故意错配）
        reply_id_override: Option<String>,
        /// 是否回复（None = 静默不回复，模拟无响应目标）
        respond: bool,
        /// 与测试体共用的消息总线（回复发布走同一 bus）
        bus: Arc<MessageBus>,
    }

    impl BusMessageHandler for Responder {
        fn on_message(&self, msg: &bedcode_plugin_api::BusMessage) -> anyhow::Result<()> {
            let _ = self.request_tx.send(msg.payload.to_string());
            if !self.respond {
                return Ok(());
            }
            let req: Value = serde_json::from_str(&msg.payload.to_string()).unwrap();
            let id = self
                .reply_id_override
                .clone()
                .unwrap_or_else(|| req["id"].as_str().unwrap().to_string());
            let reply = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "echo": req["params"][0] },
            });
            // 回复 topic：bedcode.api.reply.<caller>.<request-id>；sender 用目标插件 ID
            // （真实流程中回复由目标插件发布 —— 总线跳过「发送者自己」，若以 caller
            // 身份发布会命中回复订阅者的插件 ID 而错失投递）
            let reply_topic = format!("bedcode.api.reply.{}.{}", msg.sender, req["id"].as_str().unwrap());
            self.bus.publish(&reply_topic, "com.bedcode.sdk-test", reply);
            Ok(())
        }
    }

    /// 注册请求 topic 的静态响应者，返回收到的请求通道
    fn setup_responder(
        ctx: &Arc<WasmHostContext>,
        request_topic: &str,
        respond: bool,
        reply_id_override: Option<String>,
    ) -> tokio::sync::mpsc::UnboundedReceiver<String> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let responder = Responder {
            request_tx: tx,
            reply_id_override,
            respond,
            bus: ctx.message_bus.clone(),
        };
        let ctx2 = ctx.clone();
        let topic = request_topic.to_string();
        block_on_async(async move {
            ctx2.message_bus
                .subscribe_static("responder", &topic, Box::new(responder))
                .await;
            Ok::<(), String>(())
        })
        .expect("subscribe responder");
        rx
    }


    /// 完整往返：请求发布（门禁放行）→ 响应者回复 → 等待解析 → id 校验
    #[test]
    fn api_call_roundtrip_success() {
        let ctx = build_host_ctx();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            ctx.api_registry().register(
                "com.bedcode.sdk-test",
                &["com.bedcode.sdk-test.echo".to_string()],
            );
            let mut rx = setup_responder(
                &ctx,
                "bedcode.api.com.bedcode.sdk-test.echo",
                true,
                None,
            );

            let reply = api_call(
                &ctx,
                "com.bedcode.caller",
                "bedcode.api.com.bedcode.sdk-test.echo",
                r#"{"jsonrpc":"2.0","id":"req-1","method":"echo","params":["hi"]}"#,
                2000,
            )
            .expect("roundtrip must succeed");

            let reply: Value = serde_json::from_str(&reply).unwrap();
            assert_eq!(reply["id"], "req-1");
            assert_eq!(reply["result"]["echo"], "hi");

            // 请求确实触达目标（响应者收到的载荷原样）
            let req = rx.recv().await.expect("responder must receive request");
            assert!(req.contains("\"method\":\"echo\""));
        });
    }

    /// 未声明 api：请求被门禁拒绝，错误透传给调用方（无等待）
    #[test]
    fn api_call_rejects_undeclared_api() {
        let ctx = build_host_ctx();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let err = api_call(
                &ctx,
                "com.bedcode.caller",
                "bedcode.api.com.bedcode.scheduler.remove",
                r#"{"jsonrpc":"2.0","id":"req-1","method":"remove","params":[]}"#,
                2000,
            )
            .unwrap_err();
            assert!(err.contains("not declared"), "got: {}", err);
        });
    }

    /// 无响应目标：超时（调用方按超时错误处理，spec 验收）
    #[test]
    fn api_call_timeout_when_no_reply() {
        let ctx = build_host_ctx();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            ctx.api_registry().register(
                "com.bedcode.sdk-test",
                &["com.bedcode.sdk-test.silent".to_string()],
            );
            // 目标订阅了但静默不回复
            setup_responder(&ctx, "bedcode.api.com.bedcode.sdk-test.silent", false, None);

            let started = std::time::Instant::now();
            let err = api_call(
                &ctx,
                "com.bedcode.caller",
                "bedcode.api.com.bedcode.sdk-test.silent",
                r#"{"jsonrpc":"2.0","id":"req-1","method":"silent","params":[]}"#,
                500,
            )
            .unwrap_err();
            assert!(err.contains("timeout"), "got: {}", err);
            assert!(started.elapsed() >= Duration::from_millis(400), "must wait at least the timeout");
        });
    }

    /// 回复 id 错配：拒绝（防迟到旧回复/错配回复串台）
    #[test]
    fn api_call_rejects_id_mismatch() {
        let ctx = build_host_ctx();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            ctx.api_registry().register(
                "com.bedcode.sdk-test",
                &["com.bedcode.sdk-test.echo".to_string()],
            );
            setup_responder(
                &ctx,
                "bedcode.api.com.bedcode.sdk-test.echo",
                true,
                Some("wrong-id".to_string()),
            );

            let err = api_call(
                &ctx,
                "com.bedcode.caller",
                "bedcode.api.com.bedcode.sdk-test.echo",
                r#"{"jsonrpc":"2.0","id":"req-1","method":"echo","params":["hi"]}"#,
                2000,
            )
            .unwrap_err();
            assert!(err.contains("id mismatch"), "got: {}", err);
        });
    }

    /// 请求载荷缺 id：立即报错（SDK 契约，宿主不做猜测）
    #[test]
    fn api_call_requires_string_id() {
        let ctx = build_host_ctx();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let err = api_call(
                &ctx,
                "com.bedcode.caller",
                "bedcode.api.com.bedcode.sdk-test.echo",
                r#"{"jsonrpc":"2.0","method":"echo"}"#,
                2000,
            )
            .unwrap_err();
            assert!(err.contains("'id'"), "got: {}", err);
        });
    }
}
