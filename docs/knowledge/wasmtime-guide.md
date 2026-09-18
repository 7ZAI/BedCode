# Wasmtime 开发使用指南

> 整理自 [docs.wasmtime.dev](https://docs.wasmtime.dev/) 官方文档（Wasmtime 30.0.0，2025-02 版）。
> 本文面向在 Rust 项目中嵌入 Wasmtime 的开发场景（如 BedCode 插件系统），重点覆盖 Rust 嵌入 API。
> **注意版本差**：BedCode 当前依赖 `wasmtime = "46"`（本文档抓取时的 30.0.0 版本）；本文描述的核心概念与 API 形态跨版本稳定，具体 API 细节以 [docs.rs/wasmtime](https://docs.rs/wasmtime) 当前版本为准。

---

## 1. 概述

**Wasmtime** 是 [Bytecode Alliance](https://bytecodealliance.org/) 出品的 WebAssembly（Wasm）、WASI 与 Component Model 独立运行时，用于在浏览器之外执行 Wasm 代码，既可作 CLI 工具，也可作为库嵌入到大型应用中。

三个核心概念：

| 概念 | 说明 |
|------|------|
| **WebAssembly (Wasm)** | 可移植的二进制指令格式（`.wasm`），是编程语言的编译目标；文本表示 `.wat` |
| **WASI** | WebAssembly System Interface，提供文件系统、网络、时钟、随机数等 OS 类能力的安全可移植接口 |
| **Component Model** | 可移植、跨语言组合的 Wasm 二进制格式，通过接口（interface）通信；WASI 以组件模型接口定义 |

设计目标：

- **Fast** — 基于优化编译器 Cranelift
- **Secure** — 开发强关注正确性与安全性
- **Configurable** — 合理默认值 + CPU/内存等细粒度配置
- **Standards Compliant** — 通过官方 WebAssembly 测试套件，深度参与标准制定

---

## 2. 安装

### 2.1 CLI 安装

Linux / macOS（安装脚本，放入 `$HOME/.wasmtime` 并更新 PATH）：

```bash
curl https://wasmtime.dev/install.sh -sSf | bash
```

Windows：从 [releases 页面](https://github.com/bytecodealliance/wasmtime/releases/latest) 下载 MSI 安装包。

Cargo 安装：

```bash
cargo install wasmtime-cli
```

验证：

```bash
wasmtime -V
# wasmtime 30.0.0 (ede663c2a 2025-02-19)
```

预编译二进制有两个通道：正式 tag 发布（稳定）与 `dev` release（main 分支每日构建，最新但不稳定）。

### 2.2 Rust 库依赖

```bash
cargo add wasmtime
```

核心 crate：

| crate | 用途 |
|-------|------|
| `wasmtime` | 主嵌入 API（Engine/Module/Store/Instance/Linker） |
| `wasmtime-wasi` | WASI 支持（preview1 `p1` / preview2 `p2` 模块） |
| `wasmtime-wasi-http` | `wasi:http` 出站 HTTP 请求嵌入支持 |
| `wasmtime-wasi-threads` | WASI threads 支持 |

API 参考文档见 [docs.rs/wasmtime](https://docs.rs/wasmtime)。

### 2.3 其他语言绑定

- **Rust**：官方首选，本文重点
- **C**：通过 `libwasmtime.a`（C API），主仓库开发维护；文档见 [docs.wasmtime.dev/c-api](https://docs.wasmtime.dev/c-api/)
- **C++**：C API 之上的 header-only 库（`*.hh`）
- **Bash**：即 `wasmtime` CLI 及其子命令
- 外部绑定（非官方、同步可能滞后）：Python（wasmtime-py）、Go、.NET、Ruby、Elixir

---

## 3. CLI 使用

### 3.1 子命令总览

| 子命令 | 用途 |
|--------|------|
| `run` | 执行 Wasm 模块（默认子命令） |
| `serve` | 以 HTTP 服务器方式运行组件（`wasi:http`） |
| `wast` | 运行 spec 测试格式 `.wast` 文件 |
| `config` | 管理 Wasmtime 配置文件 |
| `completion` | 生成 shell 补全脚本 |
| `compile` | 预编译 Wasm 为原生代码（`*.cwasm`） |
| `settings` | 列出编译与运行时设置 |
| `explore` | 浏览器中探索已编译代码 |
| `objdump` | 检查 Wasm 二进制内容 |

### 3.2 `run` 命令

```bash
wasmtime run foo.wasm     # 显式
wasmtime foo.wasm         # run 是默认子命令
wasmtime foo.wat          # 支持文本格式
```

**CLI 程序模式**（默认）：核心模块调用 `_start` 导出；组件调用 `wasi:cli/run` 接口。

**参数传递规则**：`--` 之后的参数传给 Wasm 程序本身，Wasmtime 自己的 flag 必须放在 Wasm 文件之前：

```bash
wasmtime foo.wasm --bar baz     # 把 ["foo.wasm","--bar","baz"] 传给程序
wasmtime --dir . foo.wasm       # 挂载当前目录给程序（Wasmtime 选项在前）
wasmtime foo.wasm --dir .       # 错误：--dir . 被当作程序参数
```

**调用自定义导出**：

```bash
wasmtime run --invoke initialize foo.wasm        # 核心模块
wasmtime run --invoke 'initialize()' foo.wasm    # 组件用 WAVE 语法（单引号包裹）
wasmtime run --invoke add add.wasm 1 2           # i32 参数从 CLI 解析
```

> `--invoke` 的参数解析语法目前不稳定，请勿在生产中依赖。

### 3.3 常用运行选项

| 选项 | 说明 |
|------|------|
| `--dir DIR` | 授予文件系统目录访问权限（capability 模型） |
| `--env KEY=VALUE` | 设置环境变量 |
| `--invoke NAME` | 调用指定导出而非 `_start` |
| `--fuel N` | 设置燃料量（确定性中断，见 §8.2） |
| `--epoch-interruption` | 启用 epoch 中断（见 §8.2） |
| `--wasm FEATURE` | 启用的 Wasm 特性（如 `threads`、`gc`） |
| `--cranelift / --winch` | 选择编译器策略 |
| `--config PATH` | 使用 TOML 配置文件 |

### 3.4 TOML 配置文件

Wasmtime 支持把 CLI 选项写入 TOML 文件，用 `--config` 加载：

```toml
cache = true
wasm = ["threads", "gc"]
strategy = "cranelift"
# 完整字段见 `wasmtime config --help` 与 wasmtime-cli-flags crate
```

### 3.5 日志

`WASMTIME_LOG` 环境变量控制日志级别（Rust env_logger 风格）：

```bash
WASMTIME_LOG=debug wasmtime run foo.wasm    # 全量调试日志
WASMTIME_LOG=wasmtime_cranelift=warn wasmtime run foo.wasm   # 按模块过滤
```

### 3.6 编译缓存

Wasmtime 默认在用户缓存目录使用编译缓存，可通过 TOML 配置：

```toml
[cache]
enabled = true
directory = "/path/to/cache"
```

配置项：`directory`（缓存目录）、`worker-event-queue-size`、`baseline-compression-level`、`optimized-compression-level`、`cleanup-interval`、`file-count-soft-limit`、`files-total-size-soft-limit` 等。缓存以锁 + 文件形式保证并发安全。

---

## 4. Rust 嵌入核心概念

嵌入 API 的核心类型（详见 [docs.rs/wasmtime 的 Core Concepts](https://docs.rs/wasmtime/latest/wasmtime/#core-concepts)）：

| 类型 | 职责 |
|------|------|
| `Config` | 编译/运行时配置（特性开关、策略、内存限制） |
| `Engine` | 全局编译与管理环境；编译结果缓存于此；可 `Clone` 跨线程共享 |
| `Module` | 已编译的 Wasm 模块（`Engine` 与 `Module` 绑定，不可跨 Engine 使用） |
| `Store<T>` | 实例运行时状态容器，持有宿主状态 `T`（每个实例/上下文一个，不可跨线程） |
| `Instance` | 模块 + 导入实例化的结果 |
| `Linker` | 注册宿主函数/模块导入，按命名空间解析 |
| `Func` | 函数（宿主实现或 wasm 导出），`Func::wrap` 创建宿主函数 |
| `Caller<'_, T>` | 宿主函数回调参数，可访问/修改 Store 状态 |
| `TypedFunc` | 静态类型化的函数句柄，`get_typed_func` 获取 |
| `Trap` / `Error` | 执行陷阱与错误 |

**基本生命周期**：`Config → Engine → Module（编译）→ Store → Linker 注册导入 → Instance（实例化）→ 调用导出`。

**线程模型**：`Engine`/`Module`/`Linker` 可跨线程共享（`Send`/`Sync`）；`Store` 与 `Instance` 属于单线程，需在同一个线程使用。

---

## 5. 基础示例

### 5.1 Hello World（宿主函数 + 实例化）

```rust
use wasmtime::*;

struct MyState {
    name: String,
    count: usize,
}

fn main() -> Result<()> {
    // 1. 编译模块
    let engine = Engine::default();
    let module = Module::from_file(&engine, "examples/hello.wat")?;

    // 2. 创建 Store，持有宿主状态
    let mut store = Store::new(&engine, MyState { name: "hello, world!".to_string(), count: 0 });

    // 3. 定义宿主函数（wasm 导入），通过 Caller 访问宿主状态
    let hello_func = Func::wrap(&mut store, |mut caller: Caller<'_, MyState>| {
        println!("> {}", caller.data().name);
        caller.data_mut().count += 1;
    });

    // 4. 实例化：配对编译模块与导入
    let instance = Instance::new(&mut store, &module, &[hello_func.into()])?;

    // 5. 获取并调用导出
    let run = instance.get_typed_func::<(), ()>(&mut store, "run")?;
    run.call(&mut store, ())?;
    Ok(())
}
```

对应 wasm 源（`hello.wat`）：

```wat
(module
  (import "hello" "hello" (func $hello))
  (func (export "run") (call $hello))
)
```

### 5.2 线性内存（Memory）

- `Memory` 表示 wasm 的线性内存，通过 `Memory::new` 创建或从实例导出获取
- 宿主侧读写：`memory.data(&store)` / `data_mut(&mut store)` 获得切片直接访问
- 典型模式：宿主函数接收（指针, 长度），从 `Caller::get_export` 拿 memory 后读写
- 默认静态内存 4GB 地址空间 + 2GB guard 区；`Config` 可配置 `static_memory_maximum_size`、`dynamic_memory_guard_size` 等

### 5.3 模块链接（Linking）

多个 wasm 模块间互相导入导出，用 `Linker` 统一注册：

```rust
let mut linker = Linker::new(&engine);
// 注册第一个模块，其导出成为后续模块可用的导入
linker.module(&mut store, "linking1", &module1)?;
linker.module(&mut store, "linking2", &module2)?;   // 或直接实例化
```

`Linker::module` 会把模块导出注册进命名空间；`Linker::define` / `define_func` 注册任意宿主项；`Linker::instantiate` 直接实例化。

### 5.4 externref（宿主对象引用）

- `externref` 允许 wasm 持有宿主对象引用（如 Rust 结构体），通过 `Func::wrap` 参数传递
- 宿主侧用 `Val::ExternRef(Some(ExternRef::new(obj)))` 传入
- GC：Wasmtime 跟踪宿主对象存活，无需手动管理；注意保持引用期间对象生命周期

### 5.5 多值返回

wasm multi-value proposal：函数可返回多个值。Rust 侧用元组类型化：

```rust
let f = instance.get_typed_func::<(i32, i32), (i32, i32)>(&mut store, "swap")?;
let (a, b) = f.call(&mut store, (1, 2))?;
```

### 5.6 模块序列化（跳过编译）

把编译产物序列化到磁盘，反序列化时跳过编译（配合 AOT 预编译）：

```rust
// 编译一次
let module = Module::from_file(&engine, "hello.wat")?;
let bytes = module.serialize()?;          // Vec<u8>

// 之后直接反序列化
let module = unsafe { Module::deserialize(&engine, &bytes)? };
let instance = Instance::new(&mut store, &module, &[])?;
```

注意事项：
- 序列化产物绑定创建它的 `Engine`（版本、特性、目标平台），跨版本/跨平台反序列化会失败
- CLI 对应：`wasmtime compile foo.wasm` 生成 `*.cwasm`，`wasmtime run foo.cwasm` 直接运行
- `wasmtime run --precompile` 也可生成预编译产物

### 5.7 多线程嵌入

- 每个工作线程创建自己的 `Store`，共享同一个 `Engine`（`Arc<Engine>` 或 `Engine::clone`）
- `Module` 和 `Linker` 也是跨线程共享的
- 配置线程特性：`config.wasm_threads(true)`（wasm 内 spawn）
- 完整示例：`cargo run --example threads`（wasmtime 仓库 examples/threads.rs）

---

## 6. WASI 支持

### 6.1 WASI Preview1（`wasm32-wasip1`）

```rust
use wasmtime::*;
use wasmtime_wasi::WasiCtx;

fn main() -> Result<()> {
    let engine = Engine::default();
    let mut linker = Linker::new(&engine);
    wasmtime_wasi::p1::add_to_linker_sync(&mut linker, |s| s)?;   // 注册 WASI 导入

    // WasiCtxBuilder 配置 guest 可见资源
    let wasi = WasiCtx::builder().inherit_stdio().inherit_args().build_p1();
    let mut store = Store::new(&engine, wasi);

    let module = Module::from_file(&engine, "target/wasm32-wasip1/debug/wasi.wasm")?;
    linker.module(&mut store, "", &module)?;          // 实例化并注册
    linker.get_default(&mut store, "")?               // 取 _start
        .typed::<(), ()>(&store)?
        .call(&mut store, ())?;
    Ok(())
}
```

构建 guest：

```bash
rustup target add wasm32-wasip1
cargo build --target wasm32-wasip1
```

### 6.2 WASI Preview2（组件 + `wasm32-wasip2`）

```rust
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::*;
use wasmtime_wasi::p2::bindings::sync::Command;
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

// 宿主状态：WasiView 提供 IoView/WasiView 实现
pub struct ComponentRunStates {
    pub wasi_ctx: WasiCtx,
    pub resource_table: ResourceTable,
}

impl WasiView for ComponentRunStates {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.wasi_ctx, table: &mut self.resource_table }
    }
}

fn main() -> Result<()> {
    let engine = Engine::default();
    let mut linker = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;   // 注册 p2 WASI 导入

    let wasi = WasiCtx::builder().inherit_stdio().inherit_args().build();
    let mut store = Store::new(&engine, ComponentRunStates {
        wasi_ctx: wasi,
        resource_table: ResourceTable::new(),
    });

    let component = Component::from_file(&engine, "target/wasm32-wasip2/debug/wasi.wasm")?;
    let command = Command::instantiate(&mut store, &component, &linker)?;
    let result = command.wasi_cli_run().call_run(&mut store)?;   // 执行 main
    if result.is_err() { std::process::exit(1) }
    Ok(())
}
```

构建 guest：

```bash
rustup target add wasm32-wasip2
cargo build --target wasm32-wasip2
```

> **p1 vs p2 关键区别**：p1 是核心模块 + 传统 syscall 风格；p2 是组件（Component）+ 接口化 WASI，宿主状态需实现 `WasiView` 并携带 `ResourceTable`。

### 6.3 WasiCtxBuilder 常用配置

```rust
WasiCtx::builder()
    .inherit_stdio()                     // 继承标准输入输出
    .inherit_args()                      // 继承命令行参数
    .env("KEY", "VALUE")                 // 设置环境变量
    .preopened_dir(dir, "/guest/path")?  // 预打开目录（capability 授权）
    .build()                             // p2 版；p1 用 build_p1()
```

---

## 7. Component Model 与插件系统

### 7.1 WIT 接口定义

组件接口用 `.wit` 文件定义（world = 组件实现契约）：

```wit
package docs:calculator;

interface host {}

world plugin {
    import host;
    export get-plugin-name: func() -> string;
    export evaluate: func(x: s32, y: s32) -> s32;
}
```

### 7.2 绑定生成与实例化

Rust 侧用 `bindgen!` 宏自动生成类型化绑定（等价于 wit-bindgen）：

```rust
use wasmtime::component::bindgen;

bindgen!("plugin");   // 生成 Plugin 类型（对应 world）

let engine = Engine::default();
let linker: Linker<()> = Linker::new(&engine);   // 一个 Linker 全局复用

// 每个插件独立 Store（状态隔离）
let component = Component::from_file(&engine, &path)?;
let mut store = Store::new(&engine, ());
let plugin = Plugin::instantiate(&mut store, &component, &linker)?;
let name = plugin.call_get_plugin_name(&mut store)?;
let result = plugin.call_evaluate(&mut store, 1, 2)?;
```

关键设计模式：
- **一个 `Engine` + 一个 `Linker` 全局复用**；每个插件一个 `Store`（状态隔离）
- `bindgen!` 生成的 `Plugin` 类型方法直接对应 world 的导出函数，全程类型安全
- 加载插件目录时扫描 `.wasm` 文件逐个实例化

### 7.3 插件 guest 侧

插件可用任意支持组件模型的语言编写（C/C++ 用 WASI SDK + wit-bindgen，JavaScript 用 jco，Rust 用 `wasm32-wasip2` target + `wit-bindgen`），构建为组件 `.wasm` 后由宿主动态加载。

> 更完整的插件系统示例见 Sy Brand 的文章 *Building Native Plugin Systems with WebAssembly Components*（wasmtime 仓库 `examples/wasip2-plugins/` 为简化实现）。

---

## 8. 中断与资源限制

### 8.1 中断行为可配置

被中断时二选一：

- **Raise a trap** — 终止当前程序，不可恢复（默认）
- **Async yield** — 暂停并交还控制权给宿主，宿主决定是否恢复（async 模式）

### 8.2 两种中断机制

| 机制 | 确定性 | 开销 | API |
|------|--------|------|-----|
| **Fuel（燃料）** | ✅ 完全确定：相同输入+相同燃料必然在同一位置中断 | 较高 | `Config::consume_fuel` + `Store::set_fuel` / `get_fuel` |
| **Epochs（纪元）** | ❌ 基于墙钟时间，位置不固定 | 低（约 10% 减速） | `Config::epoch_interruption` + `Engine::increment_epoch` + `Store::set_epoch_deadline` |

**Fuel 示例**：

```rust
let mut config = Config::new();
config.consume_fuel(true);
let engine = Engine::new(&config)?;
let mut store = Store::new(&engine, ());
store.set_fuel(10_000)?;
// ... 调用 wasm 函数
// 燃料耗尽时返回 Trap::OutOfFuel
```

**Epoch 示例**：

```rust
let mut config = Config::new();
config.epoch_interruption(true);
let engine = Arc::new(Engine::new(&config)?);
let mut store = Store::new(&engine, ());
store.epoch_deadline_trap();          // 或 epoch_deadline_callback / 异步 yield
store.set_epoch_deadline(1);

// 另一个线程定期推进纪元
std::thread::spawn(move || {
    std::thread::sleep(std::time::Duration::from_secs(1));
    engine_clone.increment_epoch();
});
```

> 防止 wasm 死循环卡死宿主的最直接手段。宿主侧还可用 `ResourceLimiter` 限制内存/表/实例数量（`Store::limiter`）。

---

## 9. 性能调优

三个维度，按需组合（详见 wasmtime 仓库 `examples/fast-*`）：

### 9.1 快速执行（Wasm 运行速度）

```rust
let mut config = Config::new();
config.strategy(Strategy::Cranelift);          // 默认即 Cranelift 优化编译器
config.cranelift_nan_canonicalization(true);   // 可选：NaN 规范化
config.cranelift_opt_level(OptLevel::Speed);   // 最高优化
// 显式边界检查消除、强制 ISA 扩展等见 docs.wasmtime.dev/examples-fast-execution.html
```

- Cranelift = 优化编译器（类似 V8 TurboFan），代码快但编译慢
- Winch = baseline 编译器（单遍快速生成代码），编译快但执行慢

### 9.2 快速实例化

```rust
use wasmtime::{Config, Engine, InstanceAllocationStrategy, PoolingAllocationConfig};

let mut config = Config::new();
// 1. 池化分配器：预先分配资源池，实例化免分配
config.allocation_strategy(InstanceAllocationStrategy::Pooling(
    PoolingAllocationConfig::default()
));
// 2. 写时复制堆映像：实例化时延迟复制内存页
config.memory_init_cow(true);
// 3. InstancePre：预解析导入与类型检查，实例化只剩分配+初始化
let instance_pre = linker.instantiate_pre(&mut store, &module)?;
// 之后可快速多次实例化：
let instance = instance_pre.instantiate(&mut store)?;
```

要点：
- **Pooling Allocator**：AOT 预分配大池，创建/销毁实例免去内存分配，代价是配置最大并发实例数上限
- **COW Heap Images**：内存初始化延迟到首次写入，纯读数据完全免拷贝（仅 Unix 可用）
- **InstancePre**：导入查找与类型检查提前完成，适合同一模块反复实例化（多租户）

### 9.3 快速编译

```rust
use wasmtime::{Cache, Config, Engine, Strategy};

let mut config = Config::new();
config.cache(Some(Cache::from_file(None)?));   // 编译缓存（跨进程复用）
config.strategy(Strategy::Winch);              // baseline 编译器
config.parallel_compilation(true);             // 并行编译（通常默认开启）
let engine = Engine::new(&config)?;
```

> 如果启动路径完全可接受预编译，优先用 §5.6 的序列化/AOT，彻底移出编译开销。

---

## 10. 调试与剖析

### 10.1 调试

- **Guest 侧调试**（仅 wasm）：`lldb` + wasmtime 的 DWARF 支持（`wasmtime run -g` 生成调试信息）
- **原生调试**（wasm + 运行时）：`gdb` / `lldb` 调试宿主进程，`wasmtime` 以 `--debug` 运行
- **Core dumps**：`wasmtime run --coredump-on-trap` 生成 `.core`，配合 `wasmtime explore` 分析崩溃现场（wasm 段、内存、调用栈）

### 10.2 性能剖析

| 工具 | 平台 | 说明 |
|------|------|------|
| `perf` + perfmap/jitdump | Linux | `WASMTIME_PROFILE=perfmap|jitdump`，火焰图 |
| `samply` | Linux/macOS | `samply record`，浏览器 GUI |
| `VTune` | Intel | `WASMTIME_PROFILE=vtune`，GUI 导入 |
| 内置 profiler | 跨平台 | `WASMTIME_PROFILE=1`，采样 + JSON 输出 |
| `wmemcheck` | — | 检查 wasm 内非法 malloc/read/write（内存错误检测） |

### 10.3 确定性执行

追求可复现输出时（如区块链、测试）：
- 所有导入必须确定性（宿主函数禁止使用随机性/墙钟）
- `config.cranelift_nan_canonicalization(true)` 规范化 NaN
- 禁用或约束 relaxed-SIMD 的非确定性行为
- 中断机制选 Fuel（确定性）而非 Epochs

---

## 11. 安全模型

Wasmtime 定位是**安全执行不可信代码**的沙箱：

### WebAssembly 核心安全特性

- 调用栈不可访问 — 返回地址不放在应用可见内存，传统栈溢出攻击失效
- 指针编译为线性内存偏移，所有访问做边界检查
- 所有控制流跳转目标已知且类型检查过
- 与外部世界交互全部经导入/导出，无裸系统调用
- 无未定义行为

### 纵深防御（Wasmtime 额外缓解）

- 线性内存前有 2GB guard 区（防符号扩展错误）
- 原生线程栈 guard page（命中即 abort）
- 实例结束后清零内存/表（防跨实例信息泄漏）
- Rust 语言 + 安全 API：宿主不可能让 Wasmtime 段错误，安全保证不因宿主 bug 失效
- Spectre 缓解：`call_indirect` 边界检查、`br_table` 定向、动态内存访问的 spectre 防护（aarch64 `csdb` 默认关闭，可用 `use_csdb` 开启）
- **终端输出过滤**：CLI 对连接到终端的输出流过滤 ANSI 转义序列（防逃逸注入），需要 ANSI 的应用可配置豁免
- 文件系统访问遵循 WASI **capability 模型**：只授予显式预打开的资源（`--dir` / `preopened_dir`）

> 安全漏洞披露政策与判定标准见 [security-disclosure.html](https://docs.wasmtime.dev/security-disclosure.html)。

---

## 12. 稳定性与版本

- **发布节奏**：每 4 周一个 minor 版本；patch 版本只含 bug 修复与安全修复
- **版本支持**：最新版本 + 前两个 minor 版本受支持；安全修复回移植到受支持分支
- **Tier 支持级别**：
  - Tier 1：完全支持，CI 强制（x86_64/aarch64 Linux、macOS、Windows 等）
  - Tier 2：尽力支持，有 CI 但不阻塞发布
  - Tier 3：社区维护
- **平台支持**：x86_64/aarch64 全平台 Tier 1；编译后端 Cranelift 全平台、Winch 部分、Pulley 解释器（`--strategy pulley`）用于自定义平台/`no_std`
- **Wasm proposals**：分三档支持（完全实现 ✅ / 部分 🚧 / 未实现 ❌），当前支持 threads、gc、component-model（C API 部分缺口）、relaxed-simd 等；完整矩阵见 [stability-wasm-proposals.html](https://docs.wasmtime.dev/stability-wasm-proposals.html)

---

## 13. 常见坑与建议

1. **`Store` 不跨线程** — 多线程场景每个线程独立 `Store`，共享 `Engine`/`Module`/`Linker`
2. **序列化产物绑定 Engine** — `Module::serialize` 产物只能在同版本、同特性、同目标平台的 Engine 上反序列化
3. **CLI 参数顺序** — Wasmtime 选项在 wasm 文件之前，之后的全是 guest 参数
4. **p1 vs p2** — 新项目优先 `wasm32-wasip2`（组件模型，WASI 持续演进方向）；存量 p1 代码继续用 `wasmtime_wasi::p1`
5. **宿主状态访问** — 宿主函数回调里用 `Caller::data()/data_mut()` 拿 Store 状态；拿线性内存用 `Caller::get_export` + `Memory::data`
6. **无限循环防护** — 生产环境接入 Fuel 或 Epochs 中断机制 + `ResourceLimiter`
7. **性能路径** — 编译慢用缓存/Winch；实例化慢用 Pooling+COW+InstancePre；执行慢确认 Cranelift
8. **文档版本漂移** — Wasmtime 演进快（4 周一次 release），API 细节以 [docs.rs/wasmtime](https://docs.rs/wasmtime) 当前版本为准

---

## 14. 参考链接

- 官方文档主页：https://docs.wasmtime.dev/
- Rust API 参考：https://docs.rs/wasmtime
- CLI 安装：https://docs.wasmtime.dev/cli-install.html
- 嵌入示例（Rust/C/C++）：https://github.com/bytecodealliance/wasmtime/tree/main/examples
- Component Model 文档：https://component-model.bytecodealliance.org/
- WASI 文档：https://wasi.dev/
- 稳定性与 proposals：https://docs.wasmtime.dev/stability.html
- 安全策略：https://docs.wasmtime.dev/security.html
