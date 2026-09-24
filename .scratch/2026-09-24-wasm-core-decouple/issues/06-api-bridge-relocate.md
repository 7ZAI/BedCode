# 06 — api_bridge 迁出 host_api（→ manager/host/）

**Type:** task
**Blocked by:** 04
**Status:** ✅ **done**（commit `f6d1d30c0`，2026-09-24；issue 文档此前漏标，补记于 2026-09-25）

**What to build:** 把 `host_api/api_bridge.rs`（前端命令桥）整体迁到 `manager/host/api_bridge.rs`（或 manager 下等价命令面位置）。它是 Tauri invoke 命令入口，依赖 `PluginHost` 生命周期与 `manager::registry` 数据结构——属 command 面而非宿主能力实现面，留在 host_api 会让「host_api 零 manager」硬判据无法达成。

- **迁移**：文件整体移动（`git mv`）；内部 `use crate::wasm_core::manager::host::PluginHost` → `use super::PluginHost`（或同 crate 内部相对引用）；其余逻辑零改动。
- **facade 再导出保持**：`wasm_core.rs` 的 `pub use host_api::api_bridge;` 改为 `pub use manager::host::api_bridge;`（或等价），`commands.rs:385 pub use crate::wasm_core::api_bridge::*` 与 Tauri invoke_handler 注册面**零感知**——外部消费路径不变。
- **联动**：`host_api.rs` 删除 `pub mod api_bridge;`；`api_bridge` 内若引用 `host_api::tests::build_host_ctx`（迁移前请确认是否在测试里用），随迁调整。
- 迁移后全仓验证：`rg "host_api::api_bridge|wasm_core::host_api::api_bridge"` 应只余 facade 一处（新路径），其余零命中。

**验收：**

- [ ] `rg "wasm_core::host_api::api_bridge"`（生产源码）零命中
- [ ] `cargo check` 通过；前端命令面相关测试（plugin_frontend_loader_session / list / activate / deactivate 等若存在即跑，无则 cargo check 足够）
- [ ] Tauri invoke_handler（commands.rs）无改动即编译通过（facade 再导出衔接正确）
- [ ] 与 manager::registry 的数据结构（CommandEntry/ViewEntry/FileHandlerEntry/DesktopPluginInfo）共用合法（同属 manager 域）