# 移动端打开 AI Chatbox 即崩（SIGILL）——诊断与修复

日期：2026-10-15 · 状态：根因已修（宿主 + 插件 + 命令门禁），真机复验待重装 APK（签名冲突，见 §6）

## 1. 症状与真机证据

`pnpm run tauri:android:dev` 运行中，在宿主壳里**打开 AI Chatbox 应用即进程 SIGILL 退出**。

- `logcat -b crash`：`Fatal signal 4 (SIGILL), ILL_ILLOPC, fault addr 0x7153c530a8`，tid `tokio-rt-worker`；
  backtrace `#03 wasmtime ...signals12trap_handler+196` → `#05 pc 0xd40a4 <anonymous:7153b7f000>`。
- `trap_handler+196`（本机 `bedcode-mobile/src-tauri/target/aarch64-linux-android/debug/libbedcode_mobile_lib.so`
  的 `trap_handler` @0x1fd9654 → +0xc4）反汇编 = `blr x8`：**wasmtime 判定 NotWasm 后把信号交给上一个
  处理器**（WebView/crashpad）→ 进程被杀，拿不到 wasm backtrace。
- 崩溃指令定位：映射 base = .text 起始 ⇒ `text_offset = 0xd40a8`；AOT key 对上
  `/data/user/0/com.bedcode.mobile/plugins/com.bedcode.ai-chatbox/bedcode_plugin_ai_chatbox.wasm`
  （`c6ba67bb29f9c3530.cwasm`）⇒ 崩在 **ai-chatbox 插件 AOT 产物**；`.wasmtime.traps` 表里 `0xd40a8`
  已注册（trap code 9 = `UnreachableCodeReached`），符号 = `wasm[0]::function[609]::__rust_abort`
  ⇒ **guest 侧被 `abort()`（Rust panic）**。

## 2. 崩溃链（三层，均已实证）

1. **宿主装配漏注册**（根因）：`WasmHostContext::new_with_ports`（= 移动 `PluginManager` 与 `new_headless`
   共同走的构造器）没有把 `FsAuthorizer` / `ApiCallAuthorizer` 注册进 `SecurityFramework`
   （桌面构造器 `WasmHostContext::new` 注册了）。`Framework::authorize` 对未注册资源直接 `Deny`
   ⇒ **移动端 `host-fs` 全部原语恒拒**（`fs_exists` 报笼统的 `permission denied`）。
   ⇒ ai-chatbox 的 guest `activate()` 在 `store::init`（`fs_exists(index.jsonl)`）失败。
2. **插件命令面 panic**：activate 失败后 `DATA_DIR` 未初始化，但插件**前端照常装载**并在打开应用时
   调 `ai-chatbox.list-conversations`；guest 侧 `commands::data_dir()` 用
   `.expect("data_dir must be initialized during plugin activate")` ⇒ panic ⇒ `__rust_abort`。
3. **真机 trap 未被处理**：Android 上 wasmtime trap handler 判 NotWasm → delegate → SIGILL
   （残留风险，见 §5）。

宿主侧复现（宿主上跑同一产物，信号式 trap 可正常处理，把 guest 真因打出来）：

```
cargo test -p bedcode-mobile-lib --test mobile_plugin_activate_fs_e2e   # 修复后 2/2 绿
```

修复前的红（本机实测，等价真机崩溃）：

```
[probe2] command FAILED: ... wasm backtrace:
    0: ...__rust_abort
    8: ...core::option::expect_failed
    9: ...bedcode_plugin_ai_chatbox::commands::list_conversations
```

## 3. 改动清单

| # | 文件 | 内容 |
| --- | --- | --- |
| A | `packages/bedcode-wasm-core/src/host_api/context.rs` | `new_with_ports` 注册 Fs/ApiCall 仲裁器（与 `new` 同批） |
| A′ | `packages/bedcode-wasm-core/src/host_api.rs` / `host_api/fs.rs` | 夹具 `build_ports_host_ctx` + 回归锁 `ports_host_ctx_registers_fs_authorizer`（变异自检：撤注册 → 红） |
| A″ | `packages/bedcode-wasm-core/src/host_api/fs.rs` | `authorize_fs` 拒绝日志带 `AuthDecision` 原因（装配漏配可见） |
| A‴ | `packages/bedcode-wasm-core/src/test_support/bare.rs` | `MockPorts::with_app_downloads_dir`（无头夹具能驱动依赖该端事实的 activate） |
| B | `bedcode-mobile/wasm-apps/ai-chatbox/rust/src/commands.rs` | `data_dir()` 返回 `Result`（不再 `expect` panic）+ 单测 4 条命令未激活即报错 |
| B′ | 同上 `rust/src/store.rs` | 补 MockHost 缺失的 `fs_save_to_document`（该 crate `cargo test` 基线红，挡着 B 的单测） |
| D | `bedcode-mobile/src-tauri/src/plugin/manager.rs` | `invoke_command` 加状态门禁（`is_running` = Activated\|Degraded，与桌面同一道闸）+ 用例 |
| E | `bedcode-mobile/src-tauri/tests/mobile_plugin_activate_fs_e2e.rs` | 真产物闭环：activate 成功落盘 + 未激活命令返回错误而非 trap |

## 4. 验证

- `packages/bedcode-wasm-core` `cargo test --lib`：584 passed（含新回归锁；变异自检红/绿已跑）。
- `bedcode-mobile/src-tauri` `cargo test`：全量通过（含新 e2e 与门禁用例）。
- `bedcode-mobile/wasm-apps/ai-chatbox/rust` `cargo test`：17 passed。
- 移动 purity 锁 `mobile_wasm_core_purity_lock` 7/7（端名判据：新夹具命名取 `ports_host_ctx_*`）。
- 未跑：真机复验（见 §6）、cross-end-tests（零协议改动）。

## 5. 已修（第二轮，2026-10-15）：真机 trap 投递方式——移动端关信号式 trap

**问题**：真机上**任何** guest trap（插件 panic、边界越界、栈溢出、燃料耗尽……）都被
wasmtime trap handler 判成 NotWasm → delegate 给 WebView/crashpad → 进程 SIGILL 退出，
而不是一条带 wasm backtrace 的插件错误。（真机有 `libsigchain` / ART 信号链介入；
宿主 Linux 同链路正常。精确到分支的原因——`tls::with` 无状态还是 `lookup_code`/
`lookup_trap_code` 失配——未定位，但**不影响修复结论**。）

**修复（按构造消除依赖）**：移动引擎关掉信号式 trap——`EngineTuning::signals_based_traps
= Some(false)`（机制= wasm-core 配置面新 knob；取值= 移动宿主 `mobile_core_config`）。
Cranelift 在关闭时把**所有** trap 经显式 libcall（`builtin trap` + `raise`）抛出，
内存访问附带显式边界检查（wasmtime 源码 `func_environ::clif_memory_traps_enabled`、
`trap.rs` 的 `TranslateTrap::trap` 明确写了这条分支）⇒ **trap 不再经过任何信号**，
Android 的信号链根本不会参与，插件错误照旧带 `wasm backtrace`。

副产品：不再依赖 guard page 做边界省略（与 `memory_reservation` 的 256MiB 预留共存）；
关掉不影响燃料/内存/栈全部上限（只改 trap 投递方式）。代价：代码略大 / 内存访问略慢。

**已验证**：
- `Config` 接线：`manager/runtime/engine.rs::signals_based_traps_tuning_reaches_engine`
  （`Some(false)/Some(true)/None` 三分支，经 `Engine::get_signals_based_traps()` 读回；
  变异自检：撤接线 → 红）；
- 移动装配路径：`plugin::manager::tests::test_mobile_engine_disables_signals_based_traps`
  与 `tests/mobile_plugin_activate_fs_e2e.rs` 的断言（变异自检：取值翻回 `true` → 两条红）；
- 配置回落：`config::tests::load_or_default_falls_back_to_compiled_default`；
- wasm-core 586 ✓ / 移动宿主全量 ✓（含 e2e）。

**待真机复验（用户侧）**：重装后打开 AI Chatbox，预期（a）不崩溃；（b）若仍触发 trap，
日志里应是 `[plugin:…]` 前缀的 `wasm trap: … + wasm backtrace`（= 刀 1 的产物），
而不再是 `Fatal signal 4 (SIGILL)` + crashpad 墓碑。若真机上仍见到 `trap_handler+196`
的 delegate 墓碑 ⇒ 说明还有信号路径参与，回到本页继续查（下一步：自装 SIGILL handler
记录 pc/si_code 后链回原处理器，或 `[patch]` wasmtime 打点）。
注意首次启动会重编全部插件（引擎参数变 ⇒ 旧 AOT cwasm 反序列化失败后自动重编并覆写缓存，
一次性开销，之后恢复缓存命中）。

**仍未定位（保留记录）**：delegate 的精确分支。已排除：宿主 Linux 同链路正常；
wasmtime 48.0.3→48.0.5 trap 相关文件逐字相同；「Component 被 drop ⇒ `EngineCode::drop`
注销代码区间」假说（store 经 Module 持 `Arc<EngineCode>`，实测排除）；
trap 表坐标（`text_offset=0xc40a8` 未注册 / `0xd40a8` 注册为 code 9，与反汇编逐字对上）。

## 6. 真机复验阻塞：签名冲突

设备上现装的是**手工安装的 release 包**（`versionName 2.1.1`、release keystore 签名、
`flags=0x0` 不可调试），`adb install` debug APK 报
`INSTALL_FAILED_UPDATE_INCOMPATIBLE: signatures do not match`；`run-as` 亦被拒 ⇒ 无法从主机侧替换
插件文件或安装 dev 包。两条出路（需用户选择）：
1. `adb uninstall com.bedcode.mobile` 后重跑 dev（**会清掉应用数据**：AI 对话记录在外部私有目录、
   授权记录、设置）；
2. 用 release 变体安装（release keystore = `gen/android/bedcode-keystore.jks`，与在装包同一证书，
   可原地升级、数据保留）：`cd bedcode-mobile && pnpm run tauri:android:build`（release 构建耗时较长）。

另注：**在装 release 包内的 ai-chatbox 插件仍带 `data_dir must be initialized during plugin activate`**
（`unzip` 出 asset 逐字确认）⇒ 在宿主修复装进去之前，该安装仍会复现同类崩溃。
