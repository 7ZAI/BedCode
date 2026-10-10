//! 宿主上下文注册表（wasm-core-whole-crate §4.4，crate 内机制，单入口）
//!
//! mdns adapter 的零大小类型（`HostDiscoveryPorts`）需要按调用取
//! `WasmHostContext`（原经 lib `AppContext::try_global()`）。迁入本 crate 后
//! 改为本注册表：`OnceLock<Weak<WasmHostContext>>`。
//!
//! **装配（唯一入口）**：在 `host_api::install_capability_domain_ports` 里写入
//! （被 `PluginHost::new` 与两个测试夹具共同调用）。单入口纪律（2026-10-05
//! 实测教训：装配链曾三份拷贝导致顺序依赖假绿）——本注册表禁止出现第二个装配点。
//!
//! **语义**：`None`（未装配 / 弱引用已失效）= 无头，与既有
//! `AppContext::try_global() → None` 逐字一致（fail-safe 拒绝 / 跳过 /
//! `HEADLESS_UNAVAILABLE` 文案不变）。
//!
//! **幂等**：`OnceLock::set`，重复装配忽略而非替换（与
//! `install_capability_domain_ports` 同款语义）。

use crate::host_api::mobile_context::WasmHostContext;
use std::sync::Arc;
use std::sync::Weak;

static HOST_CTX: std::sync::OnceLock<Weak<WasmHostContext>> = std::sync::OnceLock::new();

/// 写入注册表（**唯一装配点**，只允许 `host_api::install_capability_domain_ports` 调用）
pub(crate) fn install(host_ctx: &Arc<WasmHostContext>) {
    let _ = HOST_CTX.set(Arc::downgrade(host_ctx));
}

/// 取宿主上下文；未装配 / 弱引用已失效 → `None`（无头语义）
pub(crate) fn get() -> Option<Arc<WasmHostContext>> {
    HOST_CTX.get().and_then(|w| w.upgrade())
}
