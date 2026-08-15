# Handoff — 移动端 WASM 组件模型迁移（已完结）

> 状态：**全部完成（2025-08-14，tickets 01–09）**，本文件为最终交接记录。
> 迁移链路：版本锁定 0.60.0 → 宿主组件路径（11 组 Host 接线 + 全部业务方法）→
> SDK 组件绑定 + componentize 构建链 → 三个内置插件切换 + 真机回归 → **09 自研 ABI 清理 + 文档同步**。

## 一句话状态

01–09 全部完成：版本锁定 0.60.0；宿主组件路径落地；SDK 组件绑定（`wasm_entry!` 宏 +
`host/` 桩 bindgen 调用 + componentize 构建链）落地；三个内置插件全部切组件并真机回归通过
（auto-task / ai-chatbox / file-transfer，Pixel_8 模拟器，logcat + CDP 证据链完整）；
**09 清理完成**：core 路径残留（41 个 func_wrap、签名表、out_ptr 通道、内存搬运、
`LoadedWasmPlugin`）全删，运行时为组件单路径。宿主 `cargo test --lib` **293 全绿**、
SDK `--features wasm` **85 全绿**（89 − 4 个 legacy 契约锁测试）、Kotlin 编译回归通过。

## 产物（最终形态，直接引用）

| 产物 | 路径 |
|------|------|
| 迁移 spec（已标注全步骤完成） | `docs/implementation-plans/mobile-wasmtime-component-migration.md` |
| 9 个 ticket（01–09，含坑记录与证据链；09 含清理明细） | `.scratch/mobile-wasmtime-component-migration/issues/` |
| SDK 组件绑定 + componentize 构建链 | `bedcode-mobile/packages/plugin-sdk-mobile/`（rust/wasm.rs、wasm_host.rs、abi.rs（仅 ABI_VERSION）、host/、tools/componentize、bin/cli.js） |
| 宿主运行时（组件单路径） | `bedcode-mobile/src-tauri/src/plugin/{loader,manager,wasm_runtime,wasm_runtime/component}.rs` + `wasm_runtime/host_impl/`（逻辑层） |
| 移动端插件开发文档（构建链/SDK 依赖/契约差异表） | `../../bedcode-mobile/plugin-dev-mobile.md`（§4/§8） |
| 两端迁移记录（已标「两端均已实施」） | `docs/knowledge/wasmtime-component-migration.md` |

## 关键决策（勿重新论证）

1. **版本锚定**：wasmtime 47（要求 rustc ≥1.94）；wit-bindgen 0.60.0（回退预案已关）
2. **一次性切割**：无 `abi.form()`；loader 组件单路径（core 产物加载报错即检查员）；core 路径代码已在 09 删除
3. **移动端独立 WIT**（11 import / 8 export，spec §3.1）；无 session/plugin-database/params/api-call/timer/process；`events.on-bus-message` 无 sender（宏内置空）
4. **安全机制原样保留**：fuel / ResourceLimiter / granted_permissions / fail-closed / AOT `.cwasm`（组件缓存统一 `c` 前缀，与桌面端一致）
5. 插件业务代码零改动（三个内置插件均实证编译通过）

## 事实基线（已核实）

- 三个内置插件资源全部是组件产物（`00 61 73 6d 0d 00 01 00`）；`bedcode-plugin build` 内置 componentize（幂等）
- 宿主测试 293 全绿；SDK `--features wasm` 85 全绿；Kotlin `compileUniversalDebugKotlin` 通过
- 自研 ABI 残留清零：`__bedcode_allocate` / `out_ptr` / `HOST_FN_SIGNATURES` / `host_session_` 零命中
- 设备上三个插件 enabled=true（08 插件页全量同步所致）

## 遗留事项（迁移之外，不阻塞）

- **宿主前端 import timeout**（`bedcode-mobile/src/plugin/loader.ts` `IMPORT_TIMEOUT=5000`）：loadAll 早期 asset.localhost 请求慢 → 后续插件前端超时；页面稳定后手动 activate 正常。三个插件的 WASM 层不受影响。修复候选：`loader.ts:12` 调大超时或按插件串行加载
- **git 误覆盖防护**：未提交关键工作随时 `git stash`（勿 drop）；误覆盖后先 `git fsck --lost-found`（stash 的 dangling commit 可恢复），恢复细节见 08 ticket「恢复记录」

## ⚠️ 真机回归必读（06/07/08 踩坑汇总，后续插件改动仍适用）

1. **组件产物会被裸 cargo build 破坏**：componentize 原地覆盖 `rust/target/.../<lib>.wasm`；裸 `cargo build --target wasm32` 后产物变回 core，且 dev-run.js 的 watch 会把 rust/target 的 wasm 同步进 resources → 真机报 parse 错误。**规则：任何裸 cargo build / touch 重编后必须重跑 `bedcode-plugin build` 再部署。**
2. **fs_auth 弹窗**：激活时目录授权 30s 超时即拒绝；弹窗出现后需立即点「允许」（CDP 轮询 + adb tap 物理坐标；dpr=2.625 换算）。
3. **dev 进程保持存活**：被 timeout 杀死后懒加载路由失效（点插件管理无响应）；tauri watch 检测到 src-tauri 文件变化会触发 rebuild（若工作区编译不过会崩 dev）——回归期间不要并行改 Rust 代码。端口 1423 被残留 vite 占用时先 `netstat -ano | grep 1423` 清理。
4. **Android 全量构建限并行度**：`CARGO_BUILD_JOBS=3 CARGO_PROFILE_DEV_CODEGEN_UNITS=4 npm run tauri:android:dev:log`（OOM 教训）。
5. 模拟器：`emulator -avd Pixel_8 -no-snapshot-save -no-boot-anim -no-audio &`（~45s）。
