# 02 — ai-chatbox WASI 预打开目录不存在导致首启必失败 / 重试死循环

Type: task
Status: resolved
Effort: plugin-activation-gate-fix
Parent spec: `spec.md`

## 问题

日志：`WASI 预打开目录未就绪：/home/binblink/.bedcode/ai-chatbox 的授权已保存，请停用后重新启用插件完成初始化`。

根因链：
1. 宿主 `PluginHost::new` 加载 WASM 插件即调用 `build_wasi_ctx` 执行 `preopened_dir`，**要求目录已存在**（wasmtime 语义）
2. `resolve_preopen_dirs` 只做授权过滤（`is_granted`），**不创建目录**
3. 数据目录 `$HOME/.bedcode/ai-chatbox` 首次启用前不存在 → preopen 失败（os error 2）
4. 插件 `activate()` `fs_request_auth` 同意后 `metadata("/data")` 自检失败 → 报错
5. 用户停用再启用：授权只落库、目录不创建，且 WASM 实例在加载期构建、停用/启用**不重建** → 同会话内重试必失败（需重启应用才能恢复）——与 progress.md 记录的「首次启用需停用再启用一次」设计契约不符

## 修复

桌面宿主：
- `build_wasi_ctx`：`preopened_dir` 前对已授权目录 `create_dir_all`（幂等）——目录创建是宿主职责，授权即代表用户同意写数据；并把**实际预打开成功**的目录列表存到 `LoadedWasmPlugin::preopened_dirs`
- `activate_plugin` 阶段 2：声明了 `wasiPreopenDirs` 的插件，激活前比对「当前已授权目录」与「实例已预打开目录」；未覆盖 → `rebuild_wasm_instance`（`load_plugin_from_file` + 替换 map 条目，与 dev 热重载同机制）→ 再执行 activate

效果：
- 已授权（父目录前缀命中）用户：启动加载期即创建目录 + 挂载 /data，**首次启用直接成功**
- 全新未授权用户：首启用弹窗同意（授权落库）→ 按提示停用再启用 → 重建实例纳入新授权 → 激活成功（无需重启）
- 拒绝授权：授权不落库，重建不触发，重试继续弹窗——语义不变

## Answer

已修复（2026-09-05）。移动端 ai-chatbox 用 `{AppDownloadsDir}/ai-chatbox`（Android 上目录存在），无此 Bug，未改移动宿主。
新增单测：`build_wasi_ctx_creates_missing_granted_dir_and_reports_preopened`、
`build_wasi_ctx_skips_uncreatable_dir_without_panicking`；桌面宿主 `cargo test --lib` 564 passed。
