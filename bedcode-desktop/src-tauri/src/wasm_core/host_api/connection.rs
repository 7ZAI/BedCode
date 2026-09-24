//! 在册连接域宿主实现（票 04，自 `host_api/session.rs` 迁入）
//!
//! **为什么不属于会话域**：返回的是宿主 WS 服务的连接注册表原始条目
//! （`WsSessionRegistry`），与会话真源（`com.bedcode.terminal-session` 登记域）无关，
//! 也不随会话原语域退役（ADR 0022 v12 裁决 5「迁独立原语」）。
//!
//! 权限判据 `connection:read`（票 04 起替代 `session:read`）。**票 10 起本面是唯一
//! 入口**：`host_api::session` 里那条同判据的旧别名已随 `host-session` interface
//! 删除（`session:read` 这把第二钥匙彻底不存在）。

use crate::wasm_core::manager::runtime::WasmHostContext;
use crate::wasm_core::runtime_util::block_on_async;
use crate::wasm_core::permission::PERMISSION_CONNECTION_READ;

/// 连接注册表原始记录清单（票 04，权限 `connection:read`）
///
/// **无排序无解读**：直取宿主 WS 连接注册表（`WebSocketManager::list_clients`）的
/// 全部原始条目序列化返回，不排序（保留注册表存储序）、不过滤（含未认证连接）、
/// 不合并（不关联配对记录）、不加派生字段。JSON 数组，元素字段名 = 注册表原始
/// 字段（camelCase）：`{clientId, deviceName?, fingerprint?, addr, authenticated,
/// connectedAt}`。排序 / 在线判定 / 会话数 / 任务状态合并是插件侧派生视图的职责
/// （spec D3「派生视图（在线判定 + 会话数 + 任务状态合并）」）。
pub(crate) fn connection_list(host_ctx: &WasmHostContext, plugin_id: &str) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_CONNECTION_READ, "host_connection_list") {
        return Err("permission denied".to_string());
    }
    let manager = crate::server::websocket::WebSocketManager::global();
    let clients = block_on_async(manager.list_clients());
    let values: Vec<serde_json::Value> = clients
        .into_iter()
        .map(|c| {
            serde_json::json!({
                "clientId": c.client_id,
                "deviceName": c.device_name,
                "fingerprint": c.fingerprint,
                "addr": c.addr,
                "authenticated": c.authenticated,
                "connectedAt": c.connected_at,
            })
        })
        .collect();
    // 错误串前缀沿用既有文案（`session error: …`）：票 04 只换归属与判据，
    // 不动任何对外可见字符串——改前缀属线协议文案变更，需另案。
    serde_json::to_string(&values).map_err(|e| format!("session error: JSON serialization failed: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm_core::host_api::tests::{build_host_ctx, generated_vocabulary_know, grant_permissions};
    use crate::wasm_core::permission::{PERMISSION_CONNECTION_READ, PERMISSION_SESSION_READ};

    const PLUGIN: &str = "com.test.connection";

    /// 权限门：缺 `connection:read` → 显性拒绝；授权后可读，
    /// 无头上下文注册表为空 → 合法空数组（形状恒定）
    #[tokio::test]
    async fn connection_list_permission_and_empty_shape() {
        let ctx = build_host_ctx();
        let err = connection_list(&ctx, PLUGIN).unwrap_err();
        assert_eq!(err, "permission denied");

        grant_permissions(&ctx, PLUGIN, &[PERMISSION_CONNECTION_READ]);
        let raw = connection_list(&ctx, PLUGIN).expect("connections list");
        let parsed: serde_json::Value = serde_json::from_str(&raw).expect("json array");
        assert!(parsed.is_array(), "必须为 JSON 数组（无头注册表为空）");
        assert_eq!(parsed, serde_json::json!([]));
    }

    /// **单钥匙锁**：只授 `session:read` 读不到连接清单。
    ///
    /// 票 04 换判据时的判据是「新面只认 `connection:read`，旧别名同判据不留后门」；
    /// 票 10 起旧别名随 `host-session` interface 删除，本锁的**更强形态**成立：
    /// 那条入口已经不存在（`session:read` 这把钥匙连门都没有了）。
    #[tokio::test]
    async fn session_read_alone_no_longer_reads_connections() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_SESSION_READ]);
        let err = connection_list(&ctx, PLUGIN).unwrap_err();
        assert_eq!(err, "permission denied", "本面只认 connection:read");

        // 旧别名（`host_api::session::session_connections_list`）已随 interface 删除：
        // 宿主源码里不得再有该标识符（本文件是锁自身，跳过避免自匹配）
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut hits: Vec<String> = Vec::new();
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                if path.ends_with("connection.rs") {
                    continue;
                }
                let Ok(content) = std::fs::read_to_string(&path) else { continue };
                for (idx, raw_line) in content.lines().enumerate() {
                    let line = raw_line.trim_start();
                    if line.starts_with("//") {
                        continue;
                    }
                    if line.contains("session_connections_list") {
                        hits.push(format!("{}:{}: {}", path.display(), idx + 1, line.trim()));
                    }
                }
            }
        }
        assert!(
            hits.is_empty(),
            "旧别名入口不得复活（票 10 已随 host-session 删除）：\n{}",
            hits.join("\n")
        );
    }

    /// 权限五同步点：新位确实进了 CLI 与前端两份**生成物**（漏跑 gen:permissions 即红）
    #[test]
    fn connection_read_bit_is_in_generated_vocabulary() {
        generated_vocabulary_know("connection:read");
    }
}
