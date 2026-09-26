# 让 wasip3 插件直接使用 WASI 0.3 `async func`（filesystem）——实施路径

> **状态：未实施**（2026-09-26 记录；用户裁决「先不实施，记录下来」）
> 触发场景：`.scratch/2026-09-25-ai-chatbox-activation-trap/fix.md`
> —— ai-chatbox 在 wasip3 下用 `std::fs` + `wasiPreopenDirs` 必 trap，当前已改用 `host-fs` 原语。
> 本文件回答：**若要让插件回到 WASI 直连，需要做什么**。

## 1. 目标

让 wasip3 组件在**现有 sync 业务代码路径**里直接调用 `wasi:filesystem/types@0.3.0`
（其方法在 WIT 里是 `async func`），即：

- 插件可恢复 `std::fs` + WASI preopen 直连（ai-chatbox 可无痛把 `host-fs` 换回 `std::fs`）；
- 不依赖宿主 `fs_auth` 逐调用授权（走 preopen 声明式授权）。

## 2. 机制依据（wasmtime 48 源码级）

**唯一判据**：guest「能否等待 async import」由当前 task 的 `may_block` 决定，而

```rust
// wasmtime-48 src/runtime/component/concurrent.rs:5635
fn may_block(&mut self, task: TableId<GuestTask>) -> Result<bool> {
    let task = self.get_mut(task)?;
    Ok(task.async_function || task.returned_or_cancelled())
}
// 同文件 :5845 —— 该标志直接取自 WIT 函数类型
let async_function = ty.async_;
```

⇒ **只有「WIT 里声明为 `async func` 的导出」被调用时，guest 才被允许等待 async import。**
否则导入 trampoline 会在进入 sync 导出时清掉标志
（`wasmtime-environ-48 src/fact/trampoline.rs:772-789`），一等待即
`trap: cannot block a synchronous task before returning`（`CannotBlockSyncTask`），
运行时检查见 `concurrent.rs::check_blocking`（:1985）。

**已被排除的两条"伪解法"**（实测/源码）：

| 做法 | 为什么不行 |
| --- | --- |
| `WasiCtxBuilder::allow_blocking_current_thread(true)` | 只改 host 内部是否真阻塞，不改 guest 是否需要等待（trap 发生在 guest 等待点）。已实测无效，代码已回退 |
| `bindgen!{ exports: { default: async } }`（宿主现状，`component.rs:41-50`） | 只是用 `TypedFunc::call_async` 驱动 **sync 导出**，不改变 `ty.async_` |
| `Config::wasm_component_model_async(true)`（已开） | 仅启用 CM_ASYNC 特性，不豁免 sync 导出的阻塞限制 |

## 3. 三层改造清单（缺一不可）

### ① 契约层 — `packages/plugin-sdk-desktop/rust/wit/bedcode.wit`（起点）

- 把**需要在其中做 WASI 文件操作**的导出声明为 `async func`：
  `bedcode:plugin/lifecycle.activate` / `deactivate`、`invoke_command`、
  `events*` 回调（`on_startup` / `on_message` / `on_ws_message` …）——凡在宿主调用栈内可能触达 `std::fs` 的全部导出。
- 按 **D4 纪律**（`.scratch/2026-09-25-wasip3-host-api-optimization/spec.md`）：
  - bump desktop ABI；
  - 更新 SDK 生成绑定与插件侧 trait/调用链；
  - 旧产物在**实例化期**点名缺失 interface / 提示按目标 SDK 重建（不得静默 polyfill）。
- **双端评估**：SDK 与移动端对齐（wit-bindgen 锁 `=0.60.0`；桌面 wasmtime 48 / 移动 47 分叉，ADR 0019）。

### ② SDK 层 — `packages/plugin-sdk-desktop/rust`

- `WasmPlugin` trait 的对应方法 async 化（**所有插件**的 Rust 实现随之改造，因导出 world 共享）。
- `wasm_entry!` 宏需生成 async 导出绑定；wit-bindgen 配置用 `async: [...]` 逐函数开启
  （0.60 的 `generate!` 支持 async import/export 配置，**macro 模式对 async 导出的支持需实测**）。
- 插件侧实现从 `fn activate() -> Result<()>` 变为 `async fn activate() -> Result<()>`
  （或 SDK 提供 sync→async 适配，取决于 wit-bindgen 生成形态）。

### ③ 宿主层 — `bedcode-desktop/src-tauri/src/wasm_core/`

| 项 | 现状 | 需要做什么 |
| --- | --- | --- |
| `bindgen!` async 导出绑定 | 已开（`exports: { default: async }`） | 随 WIT 变更重新评估生成形态 |
| `Config::wasm_component_model_async` | 已 `true` | 保持 |
| `Config::wasm_component_model_async_stackful`（🚟，`config.rs:1311`） | **未开** | **视 P0 探针结果**：若 Rust wasip3 的 async 导出走 stackful（栈切换）实现则必须开；若走 stackless callback 模型则可能不需要 |
| guest 调用模型 | `run_guest_call`：`tokio::task::spawn_blocking` + **无 tokio handle** 线程 + `block_on_ambient` | 需复核：async 导出挂起时宿主 event loop 必须能持续推进；现有"阻塞线程 + ambient block_on"模型能否承载 await 点，或需改造为在 runtime 线程上驱动（`block_on_async`） |
| 取消/超时/错误传播 | 基于 sync 调用的返回/异常 | async 导出的 `task.cancel` / `task.return` 语义需接入现有超时、Degraded、trap 恢复链路 |
| D3 红线（同实例串行） | 实例锁跨 await 持有 | async 挂起与锁语义必须一起核对（`enter_instance` 的 "cannot be entered again" 约束） |

## 4. 前置 P0 探针（**尚未做**，必须先做）

矛盾点：wit-bindgen 0.60 文档写明支持 async export bindings，但项目 2026-09-25 的 P0 探针结论是
**「async 导出 → wit-bindgen 0.60 abort」**（`.scratch/2026-09-25-wasip3-host-api-optimization/spec.md`）。

⇒ 立项前先写**最小探针**（不改主链路）：

1. 一个 wasip3 组件：`export foo: async func() -> result<_, string>`，内部调 `wasi:filesystem` 的 `stat-at`；
2. 宿主用 wasmtime 48 驱动，矩阵：`CM_ASYNC_STACKFUL` 开/关 × async 导出；
3. 判定：
   - 跑通 → 工具链就绪，可进入三层改造规划；
   - abort / 仍 trap → 需先升级 wit-bindgen / Rust nightly（并做双端影响评估）。

## 5. 成本与风险

- **影响面**：全部插件（导出签名 + trait 实现 + 插件 Rust 代码）+ 宿主调用链 + 双端 SDK（移动端需评估 wasmtime 47 分叉的支持度）。
- **迁移窗口**：ABI bump 后旧产物必须重建（D4 fail-visible），需要一次全插件产物重发。
- **复杂度**：penalty/挂起模型、取消语义、同实例串行锁与 await 的交互。
- **收益**：插件可用 WASI 标准文件能力（含 `metadata-hash` / stream 等 `host-fs` 未覆盖面），
  且省掉 `fs_auth` 逐调用授权（preopen 声明式）。

## 6. 建议顺序（若将来立项）

1. **P0 探针**（§4）——决定工具链可行性，先做、独立、不改主链路；
2. 探针通过 → 出 **ADR**（契约变更 + ABI bump + 双端影响 + 迁移窗口）；
3. 按 ① 契约 → ② SDK → ③ 宿主 落地，同步改所有插件导出；
4. 最后 ai-chatbox 回切 `std::fs` + `wasiPreopenDirs`（`host-fs` 方案可整体替换）。

## 7. 不做会怎样

- 现状即可（ai-chatbox 已用 `host-fs` 可用）；
- 仅当出现**必须 WASI 直连**的需求（例如需要 `wasi:filesystem` 的 stream/hash 能力、
  或希望完全声明式授权不弹窗）时，才需要启动本方案。

## 8. 关联文档

- `.scratch/2026-09-25-ai-chatbox-activation-trap/fix.md` — 现场记录（根因、host-fs 方案与验证）
- `.scratch/2026-09-25-wasip3-host-api-optimization/spec.md` — D4/D6/D7；**D6「优先 P3 filesystem」取向需据本文件修订**（待该专项 owner 处理，未擅改）
- `packages/plugin-sdk-desktop/rust/wit/bedcode.wit` — 契约单一事实来源
- `bedcode-desktop/src-tauri/src/wasm_core/manager/runtime/component.rs` — 宿主 bindgen/Config/WASI 接线
