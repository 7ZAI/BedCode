# 04 — SDK 组件绑定

**What to build:** 移动端插件 SDK 从自研 `(ptr,len)` ABI 切换到组件绑定：`wasm_entry!` 宏改为生成组件 world 的导出（命令/生命周期/事件回调/终端钩子/上传与传输钩子/manifest/ABI 版本）；`WasmHost` 类型名保留、内部改走 bindgen 生成的宿主 import 绑定；删除 no 对应能力的会话绑定模块（编译报错作为残存调用的检查员）；`abi.rs` 签名表与函数名常量表删除、`ABI_VERSION` 常量语义保留。插件的业务代码（`WasmPlugin` trait 实现、命令处理器）不动。

**Blocked by:** 01 — wit-bindgen 0.60 × wasmtime 47 探路 spike（版本锁定结论）

**Status:** done — 2025-08-14（含 05 构建链一并完成，见「过程发现与决策」#3）

- [x] 用新 SDK 编译的测试插件产物为组件二进制（魔法字节 `0d 00 01 00`）
- [x] 该组件能被 03（或 02）的宿主加载并完成激活/命令调用
- [x] 插件侧无代码改动即可从旧 ABI 迁移（仅依赖升级与宏入口变化）
- [x] 任何残存的会话调用在编译期报错（而非运行时空值）
- [x] SDK 单测全绿（fail-closed 默认实现、trait 默认行为）

---

## 交付物

### SDK 侧（`bedcode-mobile/packages/plugin-sdk-mobile/rust/`）

| 文件 | 改动 |
|------|------|
| `Cargo.toml` | `wasm` feature 启用 `wit-bindgen = "=0.60.0"`（macros）；版本按 ticket 01 结论锁死 |
| `src/wasm.rs` | 新增 `wit_bindgen::generate!`（`pub_export_macro: true` + `default_bindings_module: "$crate::wasm"`）；`wasm_entry!` 改为 `ident` 宏，展开 8 组 `Guest` trait impl（command/lifecycle/events/terminal-hooks/upload-hook/transfer-request-hook/manifest/abi）+ `$crate::wasm::export!`；`WasmPlugin` trait 与测试零改动 |
| `src/wasm_host.rs` | 全部 trait impl 改走 bindgen import 函数（`crate::wasm::bedcode::plugin::host_*`，0.60 的 string 参数为 `&str`）；删除 extern "C" 声明块与全部内存搬运助手（alloc/read/dealloc/out_ptr）；新增 `host_err`/`parse_json` 助手 + 单测 |
| `src/host/session.rs` | **删除**（WIT 无 session；`HostSession` 从 mod.rs/lib.rs/HostApi 全部移除，编译器当检查员） |
| `src/abi.rs` | 新契约部分仅留 `ABI_VERSION`；**legacy 段保留** NAMESPACE/MEMORY/RESULT_PAIR_SIZE/export/import 表/两张签名表——宿主 core 路径（`wasm_runtime.rs` verify_abi）在 09 清理前仍需引用（见「过程发现与决策」#2） |
| `rust/tools/componentize/` | **新增**（复制自桌面端 SDK；wit-component 锁 `=0.256.0` 与宿主 dev-dep 同版本） |

### 构建链（05 一并完成）

- `bin/cli.js`：`build` 的 WASM 步骤追加 componentize（cargo build 后编码，幂等：已是组件直接复制）；命令头注释同步
- 产物字节形态 `00 61 73 6d 0d 00 01 00`（core module 段在前、组件头随后，与 spike 实证一致）

### 宿主单测（03 侧新增 1 条验收）

- `src-tauri/src/plugin/wasm_runtime/component.rs`：`test_sdk_macro_component_loads_and_activates` —— 构建真实 auto-task 组件（新 SDK 宏产物）→ 宿主实例化 → ABI 协商 v6 → activate/deactivate → manifest（plugin.json 序列化）→ 命令错误 JSON 透传 → 默认上传/传输钩子 fail-closed 全断言。**该测试是对 `wasm_entry!` 宏展开 + `export!` 跨 crate 接线的最终证明**（区别于手写 Guest impl 的 plugin-component-test）

## 验证记录

- SDK `cargo test`（默认 79 通过）+ `cargo test --features wasm`（85 通过，含 wasm_host 新单测）
- 宿主 `cargo test --lib`：**292 通过 / 0 失败**（291 + 新增 1）
- 三个内置插件（auto-task / ai-chatbox / file-transfer）`--features wasm` wasm32 release 编译全部通过，**业务代码零改动**（仅 Cargo.lock 因新依赖变更）
- auto-task `bedcode-plugin build --rust-only` 端到端：cargo build → componentize → 产物 `00 61 73 6d 0d 00 01 00`；重复执行 componentize 幂等复制（不嵌套编码）
- 插件 SDK vitest：25 通过（CLI 改动无回归）

## 过程发现与决策

1. **`wasm_entry!` 参数从 `ty` 改为 `ident`**：0.60 的 `export!` 宏只接受 `ident`（`($ty:ident) => ...`），`ty` 片段无法透传（与桌面端 0.41 同限制）。三个内置插件调用均为裸标识符（`wasm_entry!(AutoTaskPlugin)`），零改动兼容。
2. **abi.rs 签名表暂缓删除（重要时序修正）**：ticket 04 原文要求删除 `HOST_FN_SIGNATURES` 等，但宿主 core 路径（`wasm_runtime.rs` 的 `verify_abi`/`get_export_func` 与 loader.rs）在 06–08 插件切换、09 清理之前**仍引用这些常量**——立即删除会使宿主无法编译（28 处错误）。决议：abi.rs 新契约段只留 `ABI_VERSION`，legacy 段保留全部旧常量并标注「S4/ticket 09 删除」；插件侧（宏/WasmHost）已零引用。09 删除时把 legacy 段整体删掉即可。
3. **05（构建链）随 04 一并完成**：componentize 工具 + cli.js 接线本属 05，但两者与 04 的验收强耦合（「产物为组件二进制」需构建链支撑），且 05 无额外 Blocked-by。未拆开做，报告按 04+05 合并交付。
4. **events.on-bus-message 无 sender 通道**：移动端 WIT（03 定稿）裁剪了 sender 字段，宿主侧 `call_on_bus_message` 只传 topic+payload。宏内构造 `BusMessage` 时 `sender` 置空串、`timestamp` 恒 0——旧 ABI 曾携带这两段，组件契约定稿即弃用，注释已写明。
5. **宿主测试缓存复用**：新测试复用 `COMPONENT_CACHE`（进程内 OnceLock）跨用例缓存 auto-task 组件字节，`cargo test` 只触发一次 cargo build。

## 下一步

ticket 06（auto-task 插件切换 → 真机回归）、07、08；09（清理 legacy ABI + 文档同步）时删除 abi.rs legacy 段与宿主 core 路径。
