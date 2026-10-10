//! host-connection 宿主侧接线（路径 B：WIT 绑定 + `Host` impl + 域函数 + 自报，四件同处）
//!
//! wasm-core 纯净性收口票 02 批次 05 自内核迁出（`host_api/connection.rs` 整文件
//! 删除，内核反向锁 `path_b_domains_must_not_return_to_wasm_core` 防回接）。与
//! auth / crypto 同款：**没有独立能力 crate**——装配方就是宿主本 crate（孤儿规则
//! 见 [`super::bindings`] 模块文档）。
//!
//! 在册连接域（票 04，自会话域迁入）：返回的是宿主 WS 服务的连接注册表原始条目
//! （`WsSessionRegistry`），与会话真源（`com.bedcode.terminal-session` 登记域）
//! 无关。权限判据 `connection:read`（票 04 起替代 `session:read`）。**票 10 起本面
//! 是唯一入口**：旧别名已随 `host-session` interface 删除（`session:read` 这把第二
//! 钥匙彻底不存在）。

use bedcode_host_kit::ports::downcast_host;
use bedcode_host_kit::{HostModule, HostModuleDesc, ModuleEntry, WasmPluginState};
use bedcode_plugin_api::permission::PERMISSION_CONNECTION_READ;
use bedcode_wasm_core::host_api::check_permission;
use bedcode_wasm_core::host_api::context::{PermissionScope, WasmHostContext};
use bedcode_wasm_core::runtime_util::block_on_async;

use crate::plugin::bindings::bedcode;

// 能力模块白名单条目（宿主自报；生效白名单 = 内核 IN_CRATE ∪ 宿主自报，见
// `host_module_whitelist`）。路径 B 域的自报静态住在本 crate（宿主 lib 即最终
// 二进制）⇒ 无需能力 crate 那样的 `use <crate> as _;` 强制引用行。
bedcode_host_kit::expect_host_module!(MODULE_NAME);

/// 能力模块名（白名单键即装载期日志与错误文案里的模块名）
pub const MODULE_NAME: &str = "connection";

/// 本域提供的 WIT 接口（必须与 `bedcode.wit` 逐字一致；改错即 guest import 失配）
pub const MODULE_INTERFACES: &[&str] = &["bedcode:plugin/host-connection"];

/// 本域的权限位（必须与 `bedcode.wit` / SDK 权限表逐字一致）
pub const MODULE_PERMISSIONS: &[&str] = &["connection:read"];

/// 能力模块描述符（机制面：接口路径 / 权限位 / ABI 下界；**禁带产品名词**）
///
/// `abi_min = 12`：host-connection 在 ABI v12 引入（ADR 0022 v12 裁决 5「迁独立
/// 原语」，票 04 自 `host_api/session.rs` 拆出）。
const DESC: HostModuleDesc = HostModuleDesc {
    name: MODULE_NAME,
    interfaces: MODULE_INTERFACES,
    permissions: MODULE_PERMISSIONS,
    abi_min: 12,
};

/// host-connection 能力模块
pub struct ConnectionModule;

impl HostModule for ConnectionModule {
    fn desc(&self) -> HostModuleDesc {
        DESC
    }

    fn register(&self, linker: &mut wasmtime::component::Linker<WasmPluginState>) -> wasmtime::Result<()> {
        bedcode::plugin::host_connection::add_to_linker::<WasmPluginState, HasSelf>(linker, |s| s)
    }
}

/// getter：让 guest 侧 import 取到可变的状态引用（与内核接线同款）
type HasSelf = wasmtime::component::HasSelf<WasmPluginState>;

/// 静态单例（供 `inventory::submit!` 取址）
static MODULE: ConnectionModule = ConnectionModule;

// 能力模块自报（linker-section 静态；收集点在 host-kit）
inventory::submit! {
    ModuleEntry { module: &MODULE }
}

// ==================== WIT 层（import 接口 → 域函数转发） ====================

/// 取本实例的宿主上下文（与内核 `HostCtxOf::host_ctx` 同一转型；类型不符即 panic
/// ——装配期编程错误，fail-visible，不静默降级）
fn ctx_of(state: &WasmPluginState) -> &WasmHostContext {
    downcast_host::<WasmHostContext>(state.host.as_ref())
}

impl bedcode::plugin::host_connection::Host for WasmPluginState {
    fn connections_list(&mut self) -> Result<String, String> {
        connection_list(ctx_of(self), &self.plugin_id)
    }
}

// ==================== 域函数（自 `host_api/connection.rs` 迁入，语义逐字保留） ====================

/// 连接注册表原始记录清单（票 04，权限 `connection:read`）
///
/// **无排序无解读**：直取宿主 WS 连接注册表（`WebSocketManager::list_clients`）的
/// 全部原始条目序列化返回，不排序（保留注册表存储序）、不过滤（含未认证连接）、
/// 不合并（不关联配对记录）、不加派生字段。JSON 数组，元素字段名 = 注册表原始
/// 字段（camelCase）：`{clientId, deviceName?, fingerprint?, addr, authenticated,
/// connectedAt}`。排序 / 在线判定 / 会话数 / 任务状态合并是插件侧派生视图的职责
/// （spec D3「派生视图（在线判定 + 会话数 + 任务状态合并）」）。
pub(crate) fn connection_list(perm: &dyn PermissionScope, plugin_id: &str) -> Result<String, String> {
    if !check_permission(perm, plugin_id, PERMISSION_CONNECTION_READ, "host_connection_list") {
        return Err("permission denied".to_string());
    }
    let manager = bedcode_server_websocket::WebSocketManager::global();
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
    use bedcode_plugin_api::permission::PERMISSION_SESSION_READ;
    use bedcode_wasm_core::host_api::grant_permissions;
    use bedcode_wasm_core::test_support::build_host_ctx_at;

    const PLUGIN: &str = "com.test.connection";

    /// 权限门：缺 `connection:read` → 显性拒绝；授权后可读，
    /// 无头上下文注册表为空 → 合法空数组（形状恒定）
    #[tokio::test]
    async fn connection_list_permission_and_empty_shape() {
        let ctx = build_host_ctx_at(None);
        let err = connection_list(ctx.as_ref(), PLUGIN).unwrap_err();
        assert_eq!(err, "permission denied");

        grant_permissions(&ctx, PLUGIN, &[PERMISSION_CONNECTION_READ]);
        let raw = connection_list(ctx.as_ref(), PLUGIN).expect("connections list");
        let parsed: serde_json::Value = serde_json::from_str(&raw).expect("json array");
        assert!(parsed.is_array(), "必须为 JSON 数组（无头注册表为空）");
        assert_eq!(parsed, serde_json::json!([]));
    }

    /// **单钥匙锁**：只授 `session:read` 读不到连接清单。
    ///
    /// 票 04 换判据时的判据是「新面只认 `connection:read`，旧别名同判据不留后门」；
    /// 票 10 起旧别名随 `host-session` interface 删除，本锁的**更强形态**成立：
    /// 那条入口已经不存在（`session:read` 这把钥匙连门都没有了）。
    ///
    /// 扫描根**跟着域走**（票 02 批次 05）：域迁宿主后锁定双根——内核
    /// `packages/bedcode-wasm-core/src`（原判定目标）与本 crate `src/`（新判定
    /// 目标），跳过本文件（锁自身，避免自匹配）。
    #[tokio::test]
    async fn session_read_alone_no_longer_reads_connections() {
        let ctx = build_host_ctx_at(None);
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_SESSION_READ]);
        let err = connection_list(ctx.as_ref(), PLUGIN).unwrap_err();
        assert_eq!(err, "permission denied", "本面只认 connection:read");

        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let roots = [
            manifest_dir.join("../../packages/bedcode-wasm-core/src"),
            manifest_dir.join("src"),
        ];
        let mut hits: Vec<String> = Vec::new();
        for root in roots {
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
                    let Ok(content) = std::fs::read_to_string(&path) else {
                        continue;
                    };
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
        }
        assert!(
            hits.is_empty(),
            "旧别名入口不得复活（票 10 已随 host-session 删除）：\n{}",
            hits.join("\n")
        );
    }

    /// 权限五同步点：新位确实进了 CLI 与前端两份**生成物**（漏跑 gen:permissions 即红）
    ///
    /// 与内核版判据同源（`host_api::tests::generated_vocabulary_know`），路径按
    /// 宿主 manifest 基准换算（2026-10-08 迁根后 SDK 真源在
    /// `bedcode-desktop/packages/plugin-sdk-desktop/`，前端在 `bedcode-desktop/src/`）。
    #[test]
    fn connection_read_bit_is_in_generated_vocabulary() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let cli =
            std::fs::read_to_string(manifest_dir.join("../packages/plugin-sdk-desktop/bin/permission-vocabulary.json"))
                .expect("CLI 权限词汇生成物可读");
        let frontend = std::fs::read_to_string(manifest_dir.join("../src/plugin/permission-vocabulary.ts"))
            .expect("前端权限词汇生成物可读");
        assert!(
            cli.contains(&format!("\"{PERMISSION_CONNECTION_READ}\"")),
            "CLI 权限词汇生成物缺 {PERMISSION_CONNECTION_READ}（重跑 SDK 的 pnpm run gen:permissions）"
        );
        assert!(
            frontend.contains(&format!("'{PERMISSION_CONNECTION_READ}'")),
            "前端权限词汇生成物缺 {PERMISSION_CONNECTION_READ}（重跑 SDK 的 pnpm run gen:permissions）"
        );
    }

    // ==================== 白名单 / 自报三件一致（与 crypto/auth 样板同款） ====================

    /// 白名单声明、接口路径、权限位三件与本域常量逐字一致
    #[test]
    fn host_module_declaration_matches_domain_constants() {
        assert!(
            bedcode_host_kit::expected_host_modules().contains(&MODULE_NAME),
            "能力模块白名单缺 {MODULE_NAME}（expect_host_module! 行被删 / 未收集）"
        );
        assert_eq!(MODULE_NAME, "connection", "白名单键即装载期日志与错误文案里的模块名");
        assert_eq!(
            MODULE_INTERFACES,
            &["bedcode:plugin/host-connection"],
            "接口路径必须与 WIT 契约逐字一致（改错即 guest import 失配）"
        );
        assert_eq!(
            MODULE_PERMISSIONS,
            &["connection:read"],
            "权限位必须与 `bedcode.wit` / SDK 权限表逐字一致（装载期一致性核对用）"
        );
    }
}
