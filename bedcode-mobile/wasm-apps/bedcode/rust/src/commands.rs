//! 插件命令面（前端经 plugin_invoke 调用；票 12 自宿主 `terminal_*` 命令面
//! 迁入——前端命令字从 Tauri invoke 换成本插件命名空间，语义逐项一致。
//! 票 14 阶段 B 追加配对 / 认证编排域，自宿主 `commands::auth` 编排面迁入；
//! 票 13 追加会话控制域，自宿主 `commands::session` + `session::http` 迁入）

use bedcode_plugin_api_mobile::wasm_host::WasmHost;

use crate::auth;
use crate::link::LinkManager;
use crate::session;

fn host() -> WasmHost {
    WasmHost
}

/// 命令分派（命令 id 前缀 `terminal-session.`）
pub(crate) fn dispatch(name: &str, args: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let h = host();
    match name {
        // ==================== 配对 / 认证编排域（票 14 阶段 B） ====================
        // 请求配对：桌面端出码并展示（事件 ws_pairing_request 由本插件广播）
        "terminal-session.request-pairing" => {
            auth::request_pairing(&h)?;
            Ok(serde_json::json!({ "ok": true }))
        }
        // 验证配对码：accepted = 桌面端受理且凭据已落地宿主（凭据零过境，
        // 前端持久化镜像经宿主 `ws_get_auth_credentials` 取数）
        "terminal-session.verify-pairing-code" => {
            let code = require_str(args, "code")?;
            let accepted = auth::verify_pairing_code(&h, &code)?;
            Ok(serde_json::json!({ "accepted": accepted }))
        }
        // QR token 认证（扫桌面端二维码）
        "terminal-session.authenticate-with-qr" => {
            let token = require_str(args, "token")?;
            let accepted = auth::authenticate_with_qr(&h, &token)?;
            Ok(serde_json::json!({ "accepted": accepted }))
        }
        // 生物认证登录（挑战-应答 + Keystore 签名全在宿主）
        "terminal-session.authenticate-with-biometric" => {
            let accepted = auth::authenticate_with_biometric(&h)?;
            Ok(serde_json::json!({ "accepted": accepted }))
        }
        // 订阅会话（进入终端页 / 预加载；fresh subscribe 语义——已运行链路
        // 重订阅回放环窗口）。幂等由 LinkManager 守卫
        "terminal-session.subscribe" => {
            let session_id = require_str(args, "sessionId")?;
            LinkManager::subscribe(&h, &session_id)?;
            Ok(serde_json::json!({ "ok": true }))
        }
        // 取消订阅（离开终端页 / 会话停止 / 手动断开）：关连接不再重连
        "terminal-session.unsubscribe" => {
            let session_id = require_str(args, "sessionId")?;
            LinkManager::unsubscribe(&h, &session_id);
            Ok(serde_json::json!({ "ok": true }))
        }
        // 全部取消订阅（设备手动断开 / 连接关闭时由前端调用）
        "terminal-session.unsubscribe-all" => {
            LinkManager::unsubscribe_all(&h);
            Ok(serde_json::json!({ "ok": true }))
        }
        // 会话删除：清理链路。宿主窄转发层的页面通道不在此清（插件无法触达
        // Tauri 表）——残留通道由转发失败自愈清槽（页面删除后转发必然失败
        // 就地清空），或下次 page_subscribe 覆盖 / 页面退出 page_unsubscribe 清理
        "terminal-session.remove" => {
            let session_id = require_str(args, "sessionId")?;
            LinkManager::remove(&h, &session_id);
            Ok(serde_json::json!({ "ok": true }))
        }
        // 发送终端输入：文本 + 特殊键（双形态可并存，帧序即写入序；投递失败
        // 上抛——半截输入护栏随迁）
        "terminal-session.send-input" => {
            let session_id = require_str(args, "sessionId")?;
            let data = args.get("data").and_then(|v| v.as_str()).unwrap_or("");
            let special_key = args.get("specialKey").and_then(|v| v.as_str());
            LinkManager::send_input(&h, &session_id, data, special_key)?;
            Ok(serde_json::json!({ "ok": true }))
        }
        // 渲染背压 ack：前端本地已渲染字节数推进（插件 64KB 阈值节流回发）
        "terminal-session.ack-rendered" => {
            let session_id = require_str(args, "sessionId")?;
            let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0);
            LinkManager::ack_rendered(&h, &session_id, offset);
            Ok(serde_json::json!({ "ok": true }))
        }
        // 链路状态快照（前端轮询/对账）
        "terminal-session.get-state" => {
            let session_id = require_str(args, "sessionId")?;
            Ok(LinkManager::get_state(&session_id))
        }
        // ==================== 会话控制域（票 13） ====================
        // list / start / stop / remove / input(HTTP) 经本插件自有 HTTP 面
        // （host-http + jwtAuth 宿主代注 Bearer）直连桌面 `/api/sessions*`；
        // 返回形状与退役前前端 `useHttpApi` 逐字段一致（{code, message, data?}）
        "terminal-session.list-sessions" => session::list_sessions(),
        "terminal-session.start-session" => session::start_session(args),
        "terminal-session.stop-session" => session::stop_session(args),
        "terminal-session.remove-session" => session::remove_session(args),
        "terminal-session.send-http-input" => session::send_http_input(args),
        _ => Err(anyhow::anyhow!("unknown command: {}", name)),
    }
}

fn require_str(args: &serde_json::Value, key: &str) -> anyhow::Result<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("missing {key}"))
}
