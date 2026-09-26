# ai-chatbox 启用失败（activate trap）——根因与最终方案

2026-09-25 ~ 09-26 · 桌面端：宿主诊断增强 + **插件侧适配**（p3 统一构建保持不变）

## 现象

启用 `com.bedcode.ai-chatbox` → 立即 `state=Error`，后续每次“启用”都在同一错误上打转
（`wasm trap: cannot enter component instance`），插件永久不可用（需重启应用）。
首次失败真实错误（见下）只出现在第一次调用上。

## 根因（wasmtime 源码级确证）

```
ERROR WASM plugin export call trapped plugin_id=com.bedcode.ai-chatbox export="activate"
  trap=error while executing at wasm backtrace:
    1: bedcode_plugin_ai_chatbox.wasm!filesystem_method_descriptor_metadata_hash_at
    2: __wasilibc_nocwd_fstatat → 3: stat → 4: std::sys::fs::metadata
    5: <AiChatboxPlugin as bedcode:plugin/lifecycle::Guest>::activate
Caused by:
    wasm trap: cannot block a synchronous task before returning
```

**WASI 0.3 的 filesystem 方法是 `async func`**（`wasmtime-wasi` 的 `src/p3/wit/deps/filesystem.wit`：
`stat-at: async func(...) -> result<descriptor-stat, error-code>`），而**插件导出是 sync-lifted**
（WIT `export activate: func() -> result<_, string>`）。

wasmtime 在导入调用 trampoline 里对这种情况生成硬检查
（`wasmtime-environ/src/fact/trampoline.rs:772-789`）：

```rust
let old_task_may_block = if self.module.tunables.concurrency_support {
    let old = if self.types[adapter.lift.ty].async_ {
        // 被调用 import 是 async：要求当前 task may_block，否则 trap
        GlobalGet(task_may_block); I32Eqz; If; trap(Trap::CannotBlockSyncTask); End
    } else {
        // 否则（sync import）：进入时把 may_block 清 0
        GlobalGet(task_may_block); local_set(old); I32Const(0); GlobalSet(task_may_block)
    };
```

进入 **sync 导出**时 `may_block` 被清 0 → guest 一旦等待 async import，运行时检查
（`wasmtime/src/runtime/component/concurrent.rs::check_blocking`）即抛 `CannotBlockSyncTask`。

**推论**：wasip3 目标下，**任何**“在 sync 导出里做 WASI 文件操作”的插件都必然 trap ——
与宿主侧配置无关（`allow_blocking_current_thread(true)` 实测无效：trap 发生在 guest 等待点，
不是 host 是否真阻塞）。

### 诊断缺口（已补）

宿主 `log_trap` 原来只打 `wasmtime::Error` 的 **Display**（顶层 context
`error while executing at wasm backtrace: …`），真正的 `Caused by:` 被吞，只能看到 backtrace 帧。
已改为同时打 **Debug 全链**（新增 `trap_detail` 字段）——这条改动正是本次定位的关键。

## 走过的弯路（记录以免重犯）

1. 先按“宿主实现侧阻塞导致”假设加 `WasiCtxBuilder::allow_blocking_current_thread(true)`
   → **无效**（trap 在 guest 侧，见上），已回退。
2. 再把 ai-chatbox 改成 `wasm32-wasip2` 构建（WASI 0.2 的 fs 是 sync func，实测可激活）
   → **用户裁决否决**：「wasm app 应全部以 p3 构建」。已回退，统一 p3 保持不变。

## 最终方案（插件侧适配，核心契约零改动）

ai-chatbox 放弃 WASI 直连，**文件访问改走宿主 `host-fs` 原语**（sync import，p3 下正常可用）：

| 文件 | 改动 |
| --- | --- |
| `wasm-apps/ai-chatbox/rust/src/store.rs` | IO 层全部改 `host.fs_read/fs_write/fs_exists/fs_delete`（泛型 `H: HostLog + HostFs`）；`create_dir_all` 取消（宿主 `fs_write` 自动建父目录）；测试 `TestHost` 实现 `HostFs`（std::fs 直连临时目录） |
| `.../rust/src/lib.rs` | 删 `DATA_ROOT = "/data"` 与 WASI 预打开自检；数据根 = `{HomeDir}/.bedcode/ai-chatbox`；activate 用 `fs_request_auth` 集中授权一次 + 缓存路径（`OnceLock`）+ `store::init` |
| `.../rust/src/commands.rs` | `data_dir()` 改读缓存并返回 `Result`（未激活显性报错，不静默给空路径） |
| `.../plugin.json` | 删 `wasiPreopenDirs`（permissions 已有 `fs:read` / `fs:write`） |
| `scripts/dev-run.js` | 修 ai-chatbox / file-transfer 的 `wasmFile` 过时路径（wasip2 / unknown-unknown → **wasip3**），使 dev 启动的产物新鲜度检查对准真实产物 |
| `src-tauri/.../runtime/component.rs` | 仅保留 `log_trap` 的 `trap_detail`（Debug 全链）诊断增强 |

## 验证

- 插件 native 单测：**15/15 通过**（`cd wasm-apps/ai-chatbox/rust && cargo test`）。
- 标准 p3 链构建产物（`node scripts/build.js --rust-only`，wasm32-wasip3）部署后，宿主真机：
  ```
  [plugin:com.bedcode.ai-chatbox] Plugin activated (wasm, host-fs access)
  [PluginHost] Plugin activated successfully plugin_id=com.bedcode.ai-chatbox
  ```
  并在**用户 UI 手动启用**（`persist=true`）下复现成功；同一宿主上 agent-hub / file-transfer 也正常。
- 宿主 `cargo check --lib` 通过（回退配置后无残留）。

## 遗留 / 建议

1. **spec D6 取向需修订**：`.scratch/2026-09-25-wasip3-host-api-optimization/spec.md` D6 写的是
   「声明型文件访问优先使用标准 P3 filesystem」——该取向在 **sync-lifted 导出**约束下不成立
   （D7 的 “P3 filesystem E2E” 验收项同样应先回答“在哪个上下文调用”）。建议在专项文档中登记该限制，
   明确「p3 插件文件访问一律走 `host-fs`；P3 filesystem 仅在 async 导出（WIT `async func`）可用后重启评估」。
   → **让 P3 filesystem 真正可用所需的完整改造路径（wasmtime 源码依据、WIT/SDK/宿主三层清单、P0 探针、
   成本与顺序）已记录于 `.scratch/2026-09-26-wasip3-async-export/plan.md`；用户 2026-09-26 裁决「先不实施」。**
2. **插件开发检查清单**建议增判据：wasip3 插件禁止 `std::fs` + `wasiPreopenDirs`（会 trap），
   文件访问用 `host-fs`。
3. **可选宿主护栏**（未实施）：加载期扫描组件 import，若含 `wasi:filesystem/types@0.3.0` 则
   `error!` 提示“sync 导出内无法安全使用 WASI fs”，把该坑变成 fail-visible 而不是运行期 trap。
4. **activate 失败后不重建实例**（本次未处理）：该路径绕过 trap 自动恢复调度，Error 态复用被污染的
   Store 重试必然失败，与插件侧「重新启用可重试」的承诺不符。建议单独立项（重建须在 `plugins` map 锁外）。
