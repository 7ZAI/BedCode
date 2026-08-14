# 06 — auto-task 插件切换

**What to build:** 第一个（依赖面最小：仅日志）内置插件切换到新 SDK：业务代码零改动，重新构建为组件产物，真机全流程回归（激活、命令、会话生命周期行为不变）。

**Blocked by:** 03 — 宿主 host 接线与业务方法；05 — SDK 构建链

**Status:** done — 2025-08-14 真机回归通过（含 loader 组件化接线，见「过程发现与决策」#1）

- [x] 产物为组件二进制（魔法字节 `0d 00 01 00`）且随构建链自动产出
- [x] 真机/模拟器：插件正常激活（无 Error 上报），既有命令可用
- [x] 迁移中暴露的任何残存旧 ABI 调用在编译期报错并已解决

---

## 交付物

### loader / manager 组件化接线（06 的前置，本次一并完成）

| 文件 | 改动 |
|------|------|
| `src-tauri/src/plugin/loader.rs` | `load_all` 的 WASM 加载从 core 路径（`compile_module_from_file` + `instantiate`）切到组件路径（`compile_component_from_file` + `instantiate_component`），返回 `LoadedComponentPlugin`；`load_all` 改 `pub(crate)` |
| `src-tauri/src/plugin/manager.rs` | `wasm_plugins` map 类型 `LoadedWasmPlugin` → `LoadedComponentPlugin`（两处字段 + dispatch 快照），方法名/签名零改动（03 已按同名同语义实现） |
| `src-tauri/src/plugin/wasm_runtime.rs` | `mod component` 改 `pub(crate)` + re-export `LoadedComponentPlugin` |
| `src-tauri/src/plugin/wasm_runtime/component.rs` | 测试助手（`build_host_ctx` / `build_auto_task_component`）提升 `pub(crate)`，`mod tests` 改 `pub(crate)` |
| `src-tauri/src/plugin/loader.rs` | 新增 `test_load_all_loads_component_plugin`：生产加载路径（manifest → 组件编译 → instantiate → activate → AOT `c` 前缀缓存）端到端断言 |
| `src-tauri/resources/plugins/mobile/com.bedcode.auto-task/` | `bedcode-plugin build --resources-dir` 重出组件产物（`00 61 73 6d 0d 00 01 00`） |

### 验证记录

- 宿主 `cargo test --lib`：**293 通过 / 0 失败**（292 + 新增 loader 测试）
- **真机回归（Pixel_8 模拟器，`npm run tauri:android:dev`）logcat 证据链**：
  - `[PluginLoader] WASM plugin loaded: com.bedcode.auto-task v1.0.0-beta`（组件产物经生产加载路径实例化）
  - `[plugin:com.bedcode.auto-task] Auto Task plugin activated (mobile)`（组件导出 activate + 宏内 HostLog 接线）
  - `WASM plugin activated plugin_id=com.bedcode.auto-task`（manager 状态机正常）
  - 前端 `Plugin frontend loaded: com.bedcode.auto-task`，全程无 Error 上报
- ai-chatbox / file-transfer 报 `failed to parse WebAssembly module`——**预期降级**（资源仍是旧 core 产物，loader 组件单路径当检查员；各自 07/08 切组件后恢复），manager 按「frontend-only」优雅降级

## 过程发现与决策

1. **loader 组件化接线归属 06**：02/03 只立组件路径（`instantiate_component` 等），loader/manager 仍走 core 路径——真机回归的前提是把生产加载链路切到组件。本次一并完成；core 路径代码保留（`compile_module_from_file`/`instantiate`/`verify_abi` 仍被 `init_wasm_runtime` 引用），09 清理。
2. **Android 全量构建 OOM（重要，07/08 必读）**：`npm run tauri:android:dev` 首次全量构建在默认并行度下 rustc 内存耗尽（`memory allocation of 2097152 bytes failed`，随后级联 2333 个 `can't find crate` 假错误，曾误判为 jni 0.21.1 × rustc 1.95 不兼容）。**解法：`CARGO_BUILD_JOBS=3 CARGO_PROFILE_DEV_CODEGEN_UNITS=4 npm run tauri:android:dev`**（模拟器占 ~1.1GB，并行 codegen 峰值超 14.5GB 物理内存）。后续真机回归一律按此参数。
3. **wasmtime 47 锚定新 rustc**：wasmtime 47.0.3 要求 rustc ≥1.94（`cargo tree` 实证），本机 1.95 是正确工具链；旧工具链（1.88）不可回退。`cargo update -p jni` 确认 0.21.1 为 0.21 线最新，tao 0.35.3 仍锁 jni 0.21——无需任何依赖手术（OOM 才是根因）。
4. **dev 窗口模式（非 Android）不受影响**：jni 仅 android target 编译；桌面 dev 窗口（`tauri dev`）的插件加载同样走新 loader 组件路径。

## 下一步

ticket 07（ai-chatbox 切换：重建组件产物 → 真机回归）；注意：其资源目录与前端 dist 需一并重出。
