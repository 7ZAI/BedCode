# WASM 插件运行时：Component Model 迁移方案记录

> 状态：**两端均已实施**（桌面端 2025-07；移动端 2025-08，tickets 01–09）。
> 本文为迁移调研记录（历史文档）：背景方案、迁移路径与决策依据仍有效，
> 但「现状：自研 ABI」等描述已过时——自研 ABI 残留已在两端清理删除。
> 实施细节与坑记录：桌面端 `.scratch/wasmtime-component-migration/`，
> 移动端 `.scratch/mobile-wasmtime-component-migration/issues/`。
>
> 版本对齐：宿主 wasmtime 两端均为 47（硬约束，R6）；**SDK wit-bindgen / wit-component
> 已两端对齐 =0.60.0 / =0.256.0（桌面端 2025-08-14 从 0.41/0.255 升级，源码零改动兼容，
> 四插件 wasm32 构建 + 宿主 511 测试全绿验证）**。

---

## 1. 现状：自研 ABI 全景

### 1.1 契约结构（`bedcode_plugin_api::abi`）

| 层面 | 实现 | 位置 |
|------|------|------|
| 命名空间 | 所有 host function 注册在 `"bedcode"` 命名空间 | `host_functions/mod.rs` `register!` 宏 |
| 字符串传递 | 参数以 `(ptr, len)` 对传递，指向 wasm 线性内存 | `wasm_runtime.rs` `write/read_string_to_memory` |
| 结果传递 | `(ptr, len)` 写入 8 字节 `out_ptr`（`RESULT_PAIR_SIZE`） | `read_result_from_out_ptr` |
| 内存分配 | 宿主通过 `__bedcode_allocate` 在 guest 侧分配，`__bedcode_deallocate` 配对回收 | `allocate_memory` / `dealloc_plugin_memory` |
| 版本协商 | 插件导出 `__bedcode_abi_version`，宿主校验 ≤ `ABI_VERSION`（当前 v6） | `instantiate()` |
| 契约校验 | `HOST_FN_SIGNATURES` / `PLUGIN_EXPORT_SIGNATURES` 签名表，测试期比对 | `abi.rs` + `wasm_runtime` 测试 |

### 1.2 Host Function 规模（27 个，按域分 13 组）

storage ×3、database ×4、plugin_database ×4、terminal ×1、session ×5、
events ×2、http ×1、fs ×4、config ×1、log ×4、bus ×3、file_service ×5、timer ×1（含 transfer）

插件导出：`__bedcode_activate / deactivate / invoke_command / on_terminal_input / on_terminal_output / on_startup / on_shutdown / on_message / on_session_lifecycle / on_input_submitted / on_upload_request / __bedcode_manifest / __bedcode_allocate / __bedcode_deallocate`

### 1.3 关键架构决策

- 插件 target：`wasm32-unknown-unknown`（**无 WASI**），SDK 双 feature（`native` / `wasm`）复用同一业务代码
- 插件侧无状态：`WasmHost` 是 unit struct，插件身份（plugin_id）由宿主 `Caller` state 注入
- 宿主状态：每插件独立 `Store<WasmPluginState>`，`Mutex` 串行化调用
- 生命周期：activate → invoke_command / 事件回调 → deactivate，全部同步调用
- 类型化 JSON：命令参数统一 `serde_json::Value`，宿主/插件两侧序列化

---

## 2. 迁移目标形态

迁移后插件为**组件（Component）**，接口用 WIT 定义，绑定用 `wit-bindgen` 生成：

```wit
package bedcode:plugin;

// 插件导出：命令入口 + 生命周期（对应现有 __bedcode_* 导出）
world plugin {
    export bedcode:command/invoke;      // invoke_command 的类型化替代
    export bedcode:lifecycle;           // activate/deactivate/on_startup/on_shutdown
    export bedcode:events;              // on_message/on_session_lifecycle/on_input_submitted
    import bedcode:host/storage;        // 宿主能力 → import 接口（原 host function）
    import bedcode:host/database;
    import bedcode:host/terminal;
    // ... 13 组能力逐一映射
}
```

宿主侧对应：

```rust
// wasmtime::component 路径（替代 wasmtime::Linker<WasmPluginState> 手写注册）
use wasmtime::component::{Component, Linker, ResourceTable};

let mut linker = Linker::new(&engine);
// 每个接口一个 add_to_linker（参考 wasmtime_wasi::p2::add_to_linker 模式）
bedcode_host::storage::add_to_linker(&mut linker, |s| &mut s.storage)?;
bedcode_host::database::add_to_linker(&mut linker, |s| &mut s.database)?;
// ...
```

---

## 3. 迁移路径（分 3 阶段）

### 阶段 A：协议共存（双向兼容，零风险窗口）

- 宿主 `WasmRuntime` 同时支持 core module（现状）与 component（`Component::from_file`）
- 按插件产物格式自动选择：`Module::from_file` 失败则尝试 `Component::from_file`（或按文件头魔法字节区分）
- 现有插件（wasm32-unknown-unknown 产物）继续运行，新 SDK 产物逐步切换
- **不需要** ABI v7 大版本：版本协商表增加"component"形态字段即可

### 阶段 B：SDK 切换（插件侧改造）

1. `plugin-sdk-desktop/rust` 增加 WIT 文件 + `bindgen!` 生成绑定
2. `wasm_entry!` 宏改为基于组件模型的导出（世界 `plugin` 的实现）
3. `WasmHost` 从"extern C 函数调用"改为"bindgen 生成的 import trait 实现"（**实现层唯一大改**）
4. `serde_json` 载荷保留：WIT 用 `list<u8>` / `string` 承载 JSON，命令层（`CommandArgs`/`serde_json::Value`）**完全不动**
5. 插件 target 从 `wasm32-unknown-unknown` 切换为 `wasm32-wasip2`（或 `wasm32-wasip1` + 组件化工具链）
6. 内置插件（ai-chatbox、auto-task、file-transfer）逐个切换并回归

### 阶段 C：宿主侧清理

- 删除 `host_functions/` 13 组手写注册 + `(ptr,len)` 内存搬运代码
- `LoadedWasmPlugin` 改为持有 `wasmtime::component::Instance` + `Store<新状态>`
- 签名表测试（`HOST_FN_SIGNATURES`）替换为 WIT 契约 + bindgen 生成的类型（编译期即保证）
- 现有 ResourceLimiter / AOT 缓存机制**全部保留**（组件模型完全兼容）；
  看门狗由 epoch 中断替换为**燃料（fuel）**机制：燃料只计 guest 指令数，
  宿主调用阻塞期间零消耗（慢宿主调用永不误杀），死循环烧完预算必被 trap；
  每次导出调用前重置预算（见 `FUEL_PER_CALL`）

---

## 4. 收益 vs 成本

### 收益

| 维度 | 现状（自研 ABI） | 组件模型 |
|------|-----------------|---------|
| 类型安全 | 签名表 + 测试期比对（运行前才暴露） | WIT + bindgen 编译期校验，接口漂移无法编译 |
| 内存搬运 | 手写 `(ptr,len)` + alloc/dealloc 配对，易漏 | 绑定层自动处理，杜绝泄漏 |
| 多语言插件 | 仅 Rust（SDK 双 feature 绑定） | 任意语言（C/JS/Rust 等，wit-bindgen 生态） |
| WASI 能力 | 无（全部自实现） | 可直接用 `wasmtime_wasi` 的现成接口 |
| 生态工具 | 自研 | wasm-tools / jco / componentize 等成熟工具链 |
| 异步 | 同步 + block_on_async 桥接 | `wasmtime::component` 原生 async 支持 |

### 成本与风险

| 项目 | 评估 |
|------|------|
| SDK 改造量 | **主要成本**：`wasm_host.rs` 全部 host trait 实现重写（约 10 个文件、几百行 extern 调用） |
| 宿主改造量 | 中：注册方式改为 per-interface `add_to_linker`，27 个函数签名搬运到 WIT（机械性工作） |
| 命令层兼容 | **低风险**：JSON 载荷保留，`invoke_command` 语义 1:1 映射 |
| 插件产物 | 需要重新编译所有插件（升级工具链），旧产物靠阶段 A 兜底 |
| 移动端 | 移动端 SDK（plugin-sdk-mobile）与桌面端存在 ABI 差异（log 参数等），需同步迁移 |
| 性能 | 组件模型有 trampoline 开销（毫秒级，插件调用频率下可忽略）；`(ptr,len)` 搬运的消除是净收益 |
| 学习成本 | wit-bindgen 新工具链（build.rs 集成） |

---

## 5. 决策建议

**结论：当前不建议迁移，保留自研 ABI；在以下任一条件出现时启动阶段 A：**

1. **需要支持非 Rust 插件**（第三方生态、社区插件市场）——组件模型是唯一现实路径
2. **宿主能力面持续膨胀**（host function 超过 ~50 个）——手写注册与签名表维护成本开始超过绑定生成
3. **需要 WASI 现成能力**（如网络、时钟、随机数标准接口）——自研重复造轮子不划算
4. **async host function 需求落地**——组件模型 + async 是官方推荐组合，`block_on_async` 桥接可整体移除

**若决定迁移，严格按阶段 A→B→C 推进**，阶段 A 的共存设计保证任何时刻可回滚。

---

## 6. 参考

- wasmtime-guide.md §7（Component Model 与插件系统，含 WIT/bindgen!/实例化代码）
- 官方插件示例：wasmtime 仓库 `examples/wasip2-plugins/`（本方案形态的直接参照）
- Sy Brand《Building Native Plugin Systems with WebAssembly Components》
- Component Model 规范：https://component-model.bytecodealliance.org/
