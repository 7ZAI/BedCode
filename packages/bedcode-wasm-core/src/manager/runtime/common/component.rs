//! Component Model 支持（迁移阶段 C：唯一形态）
//!
//! 对应 docs/knowledge/wasmtime-component-migration.md：
//! - 契约定义在 `packages/plugin-sdk-desktop/rust/wit/bedcode.wit`
//!   （单一事实来源），本模块用 `bindgen!` 生成绑定：
//!   - import 接口 → `Host` trait，由本模块对 `WasmPluginState` 实现
//!   - export 接口 → `exports::bedcode::plugin::*::Guest`，宿主侧调用组件
//! - 已接线 14 组 import 接口（host-app / host-storage / host-log / host-config /
//!   host-terminal / host-database / host-plugin-database / host-session /
//!   host-timer / host-events / host-http / host-fs / host-bus /
//!   host-peer / host-process），完整 `plugin` world 可直接实例化；
//!   接线模式见本文件 `add_to_linker` 与各 `impl ... Host` 块
//! - 宿主能力实现层在 `host_impl`（阶段 C 后仅此一层，core 胶水已删）
//!
//! ## 与 core module 路径的差异（历史，见 WIT 注释）
//!
//! - export 全部为必选：core ABI 中 on_message 等可选导出在组件契约中强制
//!   （组件 world 声明即契约，阶段 B SDK 无条件导出全部）
//! - log 不带 file/line 调用点（core ABI 经 ABI 传插件源码位置；
//!   组件形态暂无传递通道，见 wit/bedcode.wit 的 host-log 注释）
//! - 内存搬运由绑定层处理，无需 (ptr,len) 配对与 alloc/dealloc

#[cfg(test)]
use super::plugin_debug_mode;
use super::{StoreSpec, WasmHostContext, WasmPluginState};
#[cfg(test)]
use crate::config::StoreLimits;
// `host_ctx()` 下转型访问器：仅内核测试替身（`host-api-call` / 桌面扩展三域）需要
// ——生产注册来自宿主自报，壳内无 impl 消费它
#[cfg(test)]
use crate::host_api::context::HostCtxOf;
// core 8 域的 `Host` impl 已迁 `runtime/core_binding.rs`（票 03 Option A，双端单份）；
// 本壳只留桌面专属面：WASI 预打开装配 / 导出调用面 / 桌面 cap 引用。
use crate::manager::runtime::engine;

/// `add_to_linker` 的 getter 形态别名（`HasSelf<T>` 让 getter 返回 `&mut T`）
///
/// 桌面 cap interface 的注册在 `#[cfg(test)]` 段（生产注册来自宿主自报，见
/// `add_to_linker` 第二段），故别名需在模块级可见而非只作函数内局部。
#[cfg(test)]
type D = wasmtime::component::HasSelf<WasmPluginState>;
use crate::monitor::LifecycleEvent;
use crate::runtime_util::block_on_async;
use crate::AppError;
use bedcode_plugin_api::abi;
use crate::manifest::WasiPreopenDir;
use std::sync::Arc;
use wasmtime::component::{bindgen, Component, Instance, Linker};
use wasmtime::Store;
use wasmtime_wasi::{p2, p3, FsPerms, WasiCtxBuilder};

bindgen!({
    // 票 03：端 `wit/` 是生成物目录（同 package 拼装），path 指目录让 wit-bindgen
    // 以 push_dir 加载合成 package（bedcode.wit 含 include 组合，单文件不可解析）。
    path: "../../bedcode-desktop/packages/plugin-sdk-desktop/rust/wit",
    world: "plugin",
    // 票 02 宿主 async 化门禁：全部导出绑定生成 async 变体（call_* → async fn，
    // 内部走 TypedFunc::call_async）。wasip3 组件实例化后 Store 为 async-required，
    // 同步 call 会报 "store configuration requires that `*_async` functions are used"；
    // 统一 async 化让既有 wasip2/unknown-unknown 插件与 wasip3 走同一条调用路径
    // （同步组件在 async 路径下行为等价，见 /tmp/wasip3-probe 场景 1/4 实证）。
    exports: { default: async },
});

// ==================== Host trait 实现（import 接口） ====================
//
// 每个接口对应 host_functions/ 中一个功能域的逻辑层函数；
// 返回值映射：WIT `result<T, string>` → `Result<T, String>`，错误内容为宿主侧
// 可读消息，跨 wasm 边界后由调用方（本模块方法）转为 AppError


// v27（票 10 删除的 `impl bedcode::plugin::host_terminal::Host`）：
// `host-terminal.send` 是「宿主替插件往交互终端注入按键」的最后一处业务面入口，
// 零生产消费者（P1-b 起属主判定查已清空的内核登记，对真实会话恒拒），随
// `host-session` 同批删除。

/// 在册连接清单（票 04）：独立 interface。
/// **票 10 起是本面唯一入口**——`host-session` 上那条同判据的旧别名随整 interface 删除。
/// 已迁宿主（票 02 批次 05，`src-tauri/src/plugin/connection.rs` 路径 B：WIT impl +
/// 域函数随域走宿主；判据 `connection:read` 不变）。

// v27（票 10 删除的 `impl bedcode::plugin::host_session::Host`）：
// 12 条原语（list-sessions / get / create-with-spec / lifecycle-register /
// input-register / close / remove / rename / resize / annotate / connections-list
// 别名 / output-ring-fetch）整块退役。会话真源在 `com.bedcode.terminal-session`
// 登记域，宿主侧不再有「会话」这一原语域的对外能力面。

// host-process（v8）/ host-app（v8）/ host-timer（v6）已迁宿主（票 02 批次 05，
// `src-tauri/src/plugin/{process,app,timer}.rs` 路径 B：宿主自带 bindgen + `Host` impl +
// 域函数同处；`ProcessUnitExecutor` 随域走宿主并经 `submit_unit_executor!` 自报注册）。


// host-api-call（互调，ADR-0017）的 **WIT impl** 已迁宿主（票 02 批次 05，
// `src-tauri/src/plugin/api_call.rs` 路径 B 薄转发：无权限门，互调门禁在注册表）。
// 回复道编排（`api_call` + `ReplyHandler` + 属主校验）**留内核** `host_api/api.rs`
// ——内核 `intercall`（通用互调客户端）与 `auth_center` 桥接（注册表留内核裁决）
// 经 `call_plugin_api_host` 消费同一条编排，与 peer/ws/http「内核仍消费引擎面」同判据。

// ==================== host-platform（ADR 0022 v2）====================
//
// host-mdns / host-peer 两个 interface 已随 wasm-core-lib-split 票 03 / 05 迁为
// 能力域（经 `inventory` 自报装配，见下节），本节只剩留 core 的 POSIX / 机制面。


// v36：`host-platform.pick-folders / wsl-distros / local-ipv4-addresses /
// reveal-in-dir` 拆入 `host-platform-desktop`（桌面独有交集切片），WIT impl 已迁
// 宿主 `src-tauri/src/plugin/platform.rs`（票 02 批次 06）。

// ==================== Component Linker 组装 ====================

// ==================== 能力模块注册 ====================

/// 内建在册的能力模块白名单（仍随本 crate 链接的能力域，wasm-core-lib-split 票 03）
///
/// **本常量随迁移批次收缩**（wasm-core 纯净性收口票 02）：批次 02 已把 pty 移出
/// （其「强制引用行 + 白名单条目」随端口 adapter 一并迁宿主 `src/plugin/pty.rs`），
/// 批次 03 把 mdns / peer / ws / http 四域一并移出（adapter 落宿主
/// `src/plugin/<domain>.rs`）。
///
/// **现存条目不是 adapter 回流**：ws / peer / http 三条目保留，因为内核仍消费这三个
/// crate 的**引擎面**（ws 帧投递窄端口与端点登记、peer 的 `release_node_for` /
/// `PeerCtx`、http 的 `HttpUnitExecutor`）⇒ 它们必在最终二进制里、模块自报必被收集，
/// 白名单条目（取值 = 能力域常量）因此保留；adapter 侧的「强制引用行 + 白名单」已迁
/// 宿主，经宿主自报装配器接入（见 [`host_module_whitelist`]）。
///
/// 仍在本表里的域，四步缺一即红（新增能力域流程）：
/// ① 能力 crate 实现 `HostModule` + `inventory::submit!`
/// ② 宿主加一行强制引用 `use <crate> as _;`（本节）
/// ③ 白名单加模块名 ④ `capability_registry_matches_whitelist` 绿
///
/// 为什么②必需：`inventory` 的注册靠 linker-section 静态，**未被引用的 rlib 不进
/// 最终二进制，静态不执行 ⇒ 注册丢失**。漏掉②会让该能力域静默从插件 import 集
/// 消失；③ 的 missing 方向断言会立即变红，不拖到插件实例化才炸。
const IN_CRATE_HOST_MODULES: &[&str] = &[
    // ws 的 adapter 已迁宿主（票 02 批次 03，`src-tauri/src/plugin/ws.rs`）；内核仍消费
    // 该 crate 的引擎面（帧投递窄端口 / 端点登记）⇒ 它必在二进制里、模块自报必被收集，
    // 白名单条目因此保留，取值直接引用能力域的常量（内核零字面量）。
    bedcode_server_websocket::plugin_binding::MODULE_NAME,
    // peer 的 adapter 已迁宿主（票 02 批次 03），但内核仍依赖该 crate 的**引擎面**
    // （`release_node_for` / `PeerCtx`）⇒ 它必在最终二进制里、模块自报必被收集，
    // 白名单条目因此保留；取值直接引用能力域的常量（内核零字面量）。
    bedcode_server_peer_net::plugin_binding::MODULE_NAME,
    // http 的 adapter 已迁宿主（票 02 批次 03，`src-tauri/src/plugin/http.rs`）；内核仅余
    // `host_api/http.rs` 的 `HttpUnitExecutor`（host-task 面执行器，边保留的原因）⇒ 条目
    // 保留，取值直接引用能力域的常量（内核零字面量）。
    bedcode_server_http::plugin_binding::MODULE_NAME,
    // `host-database` / `host-plugin-database` / `host-storage` 不在册：ADR 0036
    // 撤销 sqlite 能力域 crate，三 interface 的 `Host` impl 回到本文件（见上）。
];

/// 白名单合并（纯函数）：内建在册 ∪ 宿主自报期望
///
/// 排序 + 去重是**集合语义**的口径：`verify_whitelist` 两侧都排序后整体比较，
/// 重复项不影响判定（宿主侧同一模块被两处声明不构成漂移）。
fn merge_host_module_whitelist(
    in_crate: &[&'static str],
    host_expected: &[&'static str],
) -> Vec<&'static str> {
    let mut whitelist: Vec<&'static str> = in_crate.to_vec();
    whitelist.extend_from_slice(host_expected);
    whitelist.sort_unstable();
    whitelist.dedup();
    whitelist
}

/// 生效白名单：内建在册（本 crate 仍链接的域）∪ 宿主自报期望
///
/// 宿主侧来源 = `bedcode_host_kit::expect_host_module!` 自报（与强制引用行同处，
/// 见 host-kit `assembly` 模块文档）。**常编译 pub**：宿主侧集成锁
/// （`src-tauri/tests/pty_wiring.rs`）要用同一份合并结果做双向比对——两侧各拼一份
/// 就是第二真源。
pub fn host_module_whitelist() -> Vec<&'static str> {
    merge_host_module_whitelist(IN_CRATE_HOST_MODULES, &bedcode_host_kit::expected_host_modules())
}

// 强制引用行（inventory 的 linker-section 静态必须被真正链接才执行；见上）。
// 与 `IN_CRATE_HOST_MODULES` 同处，两者不漂移——漏掉这行 ⇒ `verify_whitelist` 的
// missing 方向立即变红。
// 能力域脱绑 P2：自报只在 `common-host`（绑定层装配）下需要——无该 feature 的
// 宿主（移动端 / 无头）拿纯引擎、**不应**注册（它没有插件宿主机制）。
// （`bedcode-discovery-engine` 的强制引用行与白名单条目自票 02 批次 03 起随端口
//  adapter 迁宿主 `src-tauri/src/plugin/mdns.rs`；本 crate 不再依赖该 crate。）
// `bedcode-server-websocket` 本就是宿主依赖（传输面）；此处强制引用只为让该
// rlib 的能力模块自报静态进入最终二进制（见上）。
// 能力域脱绑 P3：自报只在 `common-host`（绑定层装配）下需要——无该 feature 的
// 宿主（移动端 / 无头）拿纯引擎、**不应**注册（它没有插件宿主机制）。
#[cfg(feature = "common-host")]
use bedcode_server_websocket as _;
// `bedcode-server-peer-net` 同理（宿主命令面 / 生命周期已在用它）。
// 能力域脱绑 P4：自报只在 `common-host`（绑定层装配）下需要——无该 feature 的
// 宿主（移动端 / 无头）拿纯引擎、**不应**注册（它没有插件宿主机制）。
#[cfg(feature = "common-host")]
use bedcode_server_peer_net as _;
// `bedcode-server-http` 同理（宿主 HTTP 服务器面 + 内核 `HttpUnitExecutor` 都在用它）；
// 该能力域（入站 2 + 出站 1 原语）的自报静态靠这行进入最终二进制——adapter 已迁宿主
// `src-tauri/src/plugin/http.rs`（票 02 批次 03），此处保留的是**引擎面消费**这一半。
// 能力域脱绑 P1：自报只在 `common-host`（绑定层装配）下需要——无该 feature 的
// 宿主（移动端 / 无头）拿纯引擎、**不应**注册（它没有插件宿主机制）。
#[cfg(feature = "common-host")]
use bedcode_server_http as _;
// 票 02 批次 02：`bedcode-pty-engine` 的强制引用行与白名单条目已随端口 adapter
// 迁宿主 `src-tauri/src/plugin/pty.rs`（两者同处——host-kit `assembly` 模块的纪律）。
// 本 crate 不再依赖该 crate，也不再出现它的任何路径。

/// 收集已自报的能力模块并与白名单双向比对
fn host_module_registry() -> crate::Result<bedcode_host_kit::ModuleRegistry> {
    verify_host_module_registry(&host_module_whitelist())
}

/// 白名单校验的可测入口（生产传入 [`host_module_whitelist`]：内建在册 ∪ 宿主自报）
///
/// **为什么多这一层**：失败路径本身必须可测。“能力模块没链上”唯一的信号就是这
/// 条错误，而它只会在真实错配时才出现（平时恒绿）——若只能经 `add_to_linker`
/// 的固定白名单触发，就永远没有用例能证明它真的会红、而且报得可读。
fn verify_host_module_registry(whitelist: &[&str]) -> crate::Result<bedcode_host_kit::ModuleRegistry> {
    let registry = bedcode_host_kit::ModuleRegistry::collected();
    registry
        .verify_whitelist(whitelist)
        .map_err(|e| AppError::Plugin(format!("host capability module whitelist check failed: {e}")))?;
    Ok(registry)
}

/// 将已接线的 import 接口注册到 component linker
///
/// **两段式装配**：
///
/// 1. **core 本地表**：下方那一列 `add_to_linker`——留内核的 interface 逐行列举。
///    域迁出时，其那一行与对应实现一起搬进能力 crate，从此由第 2 段接管。
///    （「逐接口硬编码不可接受」的判据针对**可独立组合的能力实现**，那类一律走
///    第 2 段；core 这些是 POSIX 原生面与通信机制，与宿主进程同生命周期。）
/// 2. **能力模块自动注册**：迁出的域经 `inventory` 自报，这里一次遍历装完。
///
/// 每个接口一个 `add_to_linker`（`HasSelf<T>` 让 getter 返回 `&mut T`）。
pub(crate) fn add_to_linker(linker: &mut Linker<WasmPluginState>) -> crate::Result<()> {
    // 第一段：core 交集 8 域（票 03 Option A 收口进 `core_binding`——双端唯一一份，
    // 绑契约真源 `wit/core.wit` 的 `world core`，不经端生成物）。本壳不再逐行列举。
    crate::manager::runtime::core_binding::add_to_linker(linker)?;
    // 第二段：自动收集到的能力模块（迁出 wasm_core 的域）。当前白名单为空 ⇒ 空转，
    // mdns 域迁出后此处一次装完，**宿主装配代码不再改动**。
    host_module_registry()?
        .install_all(linker)
        .map_err(|e| AppError::Plugin(format!("host capability module install failed: {e}")))?;
    // WASI preview2（wasm32-wasip2 插件经 std::fs 直接访问文件所需的全部接口：
    // clock/random/cli/filesystem/io/sockets）。
    // 不导入 wasi 的插件（unknown-unknown 既有产物 / 未来无 wasi 需求者）不受影响——
    // linker 中无对应 import 的注册是惰性的。
    // 票 02：同步 adapter（add_to_linker_sync）在 async store 下调用线进入
    // tokio runtime 上下文后其内部 ambient block_on 会重入 panic
    // （"Cannot start a runtime from within a runtime"，实证 wasi preopen e2e），
    // 改使 async adapter（fiber 内原生 await，与 wasip3 p3 同机制）。
    p2::add_to_linker_async(linker)
        .map_err(|e| AppError::Plugin(format!("Failed to register WASI preview2 interfaces: {}", e)))?;
    // WASI 0.3（wasm32-wasip3 插件）：p3 模块提供 wasi:cli@0.3.0 / clocks / random /
    // filesystem / sockets，接口版本与 p2（wasi:cli@0.2.0 等）不同名可共存；
    // 内部以 func_wrap_async 注册（CM_ASYNC 并发模型），仅 wasip3 组件消费，
    // 既有插件不导入 wasi0.3 则注册惰性无效。
    p3::add_to_linker(linker)
        .map_err(|e| AppError::Plugin(format!("Failed to register WASI 0.3 (p3) interfaces: {}", e)))?;
    // 内核测试替身（票 02 批次 05，**仅内核测试二进制**）：SDK 夹具经互调 client
    // 静态 import `host-api-call`，WIT impl 迁宿主后本二进制不再注册该 interface
    // ⇒ 夹具实例化失败。替身把 guest 调用原样转发给**仍留内核的编排**（
    // `host_api::api::api_call`，caller = 插件 id），与宿主 `plugin/api_call.rs`
    // 的薄转发逐字同语义。生产二进制不含 cfg(test) 分支——宿主薄转发是唯一
    // 注册来源（与 `test_support::kernel_test_domain_ports` 同范式同隔离保证）。
    #[cfg(test)]
    bedcode::plugin::host_api_call::add_to_linker::<WasmPluginState, D>(linker, |s| s)
        .map_err(|e| AppError::Plugin(format!("Failed to register component host interface: {}", e)))?;
    // 内核测试替身（票 02 批次 06，**仅内核测试二进制**）：v36 切片后 SDK 夹具
    // 静态 import 三个桌面扩展 interface（host-fs-desktop / host-platform-desktop /
    // host-events-desktop），其 WIT impl 已迁宿主 ⇒ 本二进制不再注册 ⇒ 夹具
    // 实例化失败。替身语义 = headless 显性拒绝（引擎测试不触发调用；sdk_e2e 的
    // `test_notify` 断言无头上下文报错文案含 headless/app_handle，见各 impl）。
    // 生产二进制不含 cfg(test) 分支——宿主 plugin/{fs,platform,events}.rs 是唯一
    // 注册来源（与 host-api-call 替身同范式同隔离保证）。
    #[cfg(test)]
    bedcode::plugin::host_events_desktop::add_to_linker::<WasmPluginState, D>(linker, |s| s)
        .map_err(|e| AppError::Plugin(format!("Failed to register component host interface: {}", e)))?;
    #[cfg(test)]
    bedcode::plugin::host_fs_desktop::add_to_linker::<WasmPluginState, D>(linker, |s| s)
        .map_err(|e| AppError::Plugin(format!("Failed to register component host interface: {}", e)))?;
    #[cfg(test)]
    bedcode::plugin::host_platform_desktop::add_to_linker::<WasmPluginState, D>(linker, |s| s)
        .map_err(|e| AppError::Plugin(format!("Failed to register component host interface: {}", e)))?;
    Ok(())
}

/// 内核测试替身（**仅 cfg(test)**）：host-api-call 的 WIT impl——转发内核编排。
///
/// 为什么不是「回流」：本 impl 只存在于内核测试二进制（cfg(test) 不进生产），
/// 让 30+ 个以 SDK 夹具为载体的引擎测试（sdk_e2e / owner_e2e / engine_limits /
/// component 往返）在 host-api-call 迁宿主后照常实例化夹具；编排本体
/// （`host_api/api.rs`）本就留内核，替身与宿主薄转发消费同一条编排、零第二实现
/// 语义。若编排迁出内核（未来批次），本替身与那些测试须同批裁决去向。
#[cfg(test)]
impl bedcode::plugin::host_api_call::Host for WasmPluginState {
    fn call(&mut self, request_topic: String, payload_json: String, timeout_ms: u64) -> Result<String, String> {
        crate::host_api::api::api_call(
            self.host_ctx(),
            self.host_ctx(),
            self.host_ctx(),
            &self.plugin_id,
            &request_topic,
            &payload_json,
            timeout_ms,
        )
    }
}

// ==================== 内核测试替身：v36 切片桌面扩展三接口（仅 cfg(test)） ====================
//
// 票 02 批次 06 把四个交集接口的桌面独有函数拆为新 interface 并迁宿主（
// `src-tauri/src/plugin/{fs,platform,events}.rs`；`abi-form` 是 guest 导出：
// SDK 宏按新契约导出，宿主 verify_abi 读取，`abi` 本体也已只有 version）。内核
// 测试二进制不再注册这三个扩展 interface ⇒ SDK 夹具（import 整个 world）在内核
// 测试实例化即失败。替身与 host-api-call 替身同范式：**仅 cfg(test) 存在**，
// 生产二进制不含 ⇒ 宿主 plugin/*.rs 的真实现是唯一注册来源（隔离保证见
// `test_support::kernel_test_domain_ports`）。
//
// 语义：headless 显性拒绝。桌面扩展在无头内核测试中不可用是**环境事实**（无
// AppHandle / 无宿主平台依赖），不是伪装可用；sdk_e2e 的 `test_notify` 断言
// 无头上下文报错文案含 headless/app_handle，故 `notify` 替身按生产 headless
// 文案逐字返回（`host_api/events.rs` 旧 notify 的同一文案）。

#[cfg(test)]
impl bedcode::plugin::host_events_desktop::Host for WasmPluginState {
    fn notify(&mut self, title: String, body: String) -> Result<(), String> {
        // 与生产宿主 headless 路径同一文案：sdk_e2e 断言 error 含 headless / app_handle
        let _ = (title, body);
        Err("notify error: app_handle not available in headless context".to_string())
    }
}

#[cfg(test)]
impl bedcode::plugin::host_fs_desktop::Host for WasmPluginState {
    fn read_dir(&mut self, path: String) -> Result<String, String> {
        Err(format!(
            "fs error: read dir '{}' failed: host-fs-desktop not registered in kernel test binary",
            path
        ))
    }

    fn canonicalize(&mut self, path: String) -> Result<Option<String>, String> {
        Err(format!(
            "fs error: canonicalize '{}' failed: host-fs-desktop not registered in kernel test binary",
            path
        ))
    }

    fn stat(&mut self, path: String) -> Result<Option<String>, String> {
        Err(format!(
            "fs error: stat '{}' failed: host-fs-desktop not registered in kernel test binary",
            path
        ))
    }
}

#[cfg(test)]
impl bedcode::plugin::host_platform_desktop::Host for WasmPluginState {
    fn pick_folders(&mut self) -> Result<String, String> {
        Err("platform unavailable in headless context (no app_handle)".to_string())
    }

    fn wsl_distros(&mut self) -> Result<String, String> {
        Err("wsl distro list failed: host-platform-desktop not registered in kernel test binary".to_string())
    }

    fn local_ipv4_addresses(&mut self) -> Result<String, String> {
        Err("local ipv4 list failed: host-platform-desktop not registered in kernel test binary".to_string())
    }

    fn reveal_in_dir(&mut self, path: String) -> Result<(), String> {
        Err(format!(
            "reveal failed: host-platform-desktop not registered in kernel test binary (path: {})",
            path
        ))
    }
}

// ==================== 组件插件实例 ====================

/// 实例元数据：宿主侧只读投影（票 06 §5.1，与 Store/Instance 的所有权分离）
///
/// 拆分动机（调用模型无关）：`mutex` 与 `event-loop` 两种模型下宿主侧都需要
/// **不进实例**就能读到这些量——导出能力探测结果、WASI 预打开目录、创建时刻
/// 都在实例化期一次确定后不再变化。`event-loop` 模型的 Store 归属主任务独占，
/// 宿主侧若还要「锁实例读元数据」就等于开出第二个 Store 入口（破坏 I1）。
#[derive(Clone)]
pub(crate) struct InstanceMeta {
    /// 插件 ID
    pub(crate) plugin_id: String,
    /// 实例化时实际预打开成功的主机目录（manifest 声明 ∩ 当时已授权 ∩ 创建成功）
    ///
    /// 激活时宿主据此判断「新授权目录是否已纳入本次实例」：声明了 WASI 预打开
    /// 目录的插件首次启用时授权发生在 activate() 内部（fs_request_auth 弹窗），
    /// 早于实例化；重试激活时若当前实例未覆盖新授权目录则需重建（见 host.rs
    /// `rebuild_wasm_instance`），使 /data 挂载与授权状态一致。
    pub(crate) preopened_dirs: Vec<String>,
    /// 实例化时探测到的可路由能力接口导出（core-plugin-manager，如
    /// `host-storage`）；空 = 纯应用插件（不提供能力），非空者激活时由
    /// PluginHost 注册进能力注册表（系统组件装配）
    pub(crate) exported_capabilities: Vec<String>,
    /// 实例创建时刻（Drop 日志计算存活时长）
    pub(crate) created_at: std::time::Instant,
}

/// 可选导出句柄快照（实例化期动态探测结果，Copy；属主任务取用）
///
/// 与 store 内同一批句柄同源：`mutex` 模型在调用栈内直接读 store 数据，
/// `event-loop` 模型在进入属主作用域前一次性取出（作用域内只能拿到 `&Accessor`）。
#[derive(Clone, Copy)]
pub(crate) struct OptionalExports {
    pub(crate) on_message_binary: Option<wasmtime::component::TypedFunc<(String, String, Vec<u8>), ()>>,
    pub(crate) on_ws_message: Option<wasmtime::component::TypedFunc<(String, String, Vec<u8>), ()>>,
    pub(crate) on_ws_client_message: Option<wasmtime::component::TypedFunc<(String, String, String, Vec<u8>), ()>>,
    pub(crate) on_task_event: Option<wasmtime::component::TypedFunc<(String,), ()>>,
}

/// 已加载的组件形态 WASM 插件（阶段 C 后唯一形态）
///
/// 持有 component Instance + Store，全部调用走 bindgen 生成的类型化接口
/// （无 (ptr,len) 内存搬运）。Store 必须与 Instance 一起持有，
/// 否则导出函数无法调用。
///
/// 调用模型（票 06）：本结构是 `mutex` 模型下的实例锁内容物；`event-loop`
/// 模型下整个结构被移动进属主任务（持有 `&mut Store` 的唯一位置），宿主侧
/// 只保留 [`InstanceMeta`] 副本。
pub struct LoadedWasmPlugin {
    meta: InstanceMeta,
    instance: Instance,
    store: Store<WasmPluginState>,
}

impl Drop for LoadedWasmPlugin {
    /// 实例死亡日志：Store 被 drop（停用 / 热重载替换 / 应用退出 / 异常清理）时记录，
    /// 与创建日志（`WasmRuntime::instantiate_component`）成对，构成实例生命周期观测。
    /// Drop 内无锁操作，tracing 安全。日志实现见 [`engine::log_instance_dropped`]。
    fn drop(&mut self) {
        engine::log_instance_dropped(&self.meta.plugin_id, self.meta.created_at.elapsed());
    }
}

impl LoadedWasmPlugin {
    /// 实例化组件
    ///
    /// 与 core 路径相同的防护：资源限制、燃料看门狗、ABI 版本协商
    /// （组件必须声明 form=1，版本号语义不变）
    pub(crate) fn new(
        engine: &wasmtime::Engine,
        component_linker: &Linker<WasmPluginState>,
        component: &Component,
        plugin_id: &str,
        host_ctx: Arc<WasmHostContext>,
        declared_preopen_dirs: &[WasiPreopenDir],
        spec: StoreSpec,
    ) -> crate::Result<Self> {
        // WASI 上下文：按 manifest 声明（wasiPreopenDirs，展开+授权过滤）预打开
        // 目录 /data…；无声明/未授权时为空上下文
        let (wasi_ctx, preopened_dirs) = build_wasi_ctx(&host_ctx, plugin_id, declared_preopen_dirs);
        let state = WasmPluginState::new(plugin_id.to_string(), host_ctx, wasi_ctx, spec);
        // Store 装配（limiter 接线 + 首调用燃料注入）走引擎模块的唯一一份
        let mut store = engine::new_store(engine, state)?;

        // 票 02：CM_ASYNC 引擎下实例化必须走 async 入口（wasip3 组件导入 async
        // wasi 0.3 函数，同步 instantiate 报 async-required）；同步组件在
        // instantiate_async 下行为等价。形态差异经 `InstantiateMode` 表达，机制
        // 本体在引擎模块（fiber 驱动见 `block_on_async`）。
        let instance = engine::instantiate(
            &mut store,
            component_linker,
            component,
            engine::InstantiateMode::Async,
        )
        .map_err(|e| {
                // v27（票 10）：**旧 SDK 产物的失败形态必须可诊断**。ABI 破坏性变更
                // （删 `host-session` / `host-terminal` 两个 import、删 `terminal-hooks`
                // 导出）后，旧产物在**实例化**阶段就会失败（找不到 import 的实现），
                // 早于 `verify_abi` 的版本协商——wasmtime 的报错本身点名了缺失的
                // interface，这里补一句「按哪个版本重建」，避免运维把它当成 trap 或
                // 插件损坏。
                AppError::Plugin(format!(
                    "Failed to instantiate WASM component for plugin '{}': {}{}",
                    plugin_id,
                    e,
                    Self::stale_artifact_rebuild_hint(&e.to_string())
                ))
            })?;

        Self::verify_abi(&mut store, &instance)?;

        // core-plugin-manager：探测能力接口导出（系统组件据此注册为
        // 能力提供者；含票 12 auth-policy 仅探测能力——所有新 SDK 插件默认
        // 导出（拒绝实现），宿主中间件只消费认证中心实例）
        let exported_capabilities =
            crate::manager::capability::probe_exported_capabilities(&instance, &mut store);
        if !exported_capabilities.is_empty() {
            tracing::info!(
                plugin_id = %plugin_id,
                capabilities = ?exported_capabilities,
                "WASM component exports capability interface(s)"
            );
        }

        Ok(Self {
            meta: InstanceMeta {
                plugin_id: plugin_id.to_string(),
                preopened_dirs,
                exported_capabilities,
                created_at: std::time::Instant::now(),
            },
            instance,
            store,
        })
    }

    /// 实例元数据引用（宿主侧只读投影；调用模型两侧共用）
    pub(crate) fn meta(&self) -> &InstanceMeta {
        &self.meta
    }

    // ==================== 属主化访问器（票 06，`event-loop` 模型用） ====================
    //
    // 属主任务在 `run_concurrent` 之外需要预先取出这些量（进入作用域后只能经
    // `&Accessor` 触达 store，无法再 `&mut self`）：Instance 句柄（Copy）、指标句柄、
    // 燃料规格、可选导出句柄（Copy）。

    /// 组件实例句柄（Copy；`get_typed_func` / `start_call_concurrent` 用）
    pub(crate) fn instance_handle(&self) -> Instance {
        self.instance
    }

    /// Store 可变引用（属主任务以 `&mut store` 调 `run_concurrent`，常驻作用域）
    pub(crate) fn store_mut(&mut self) -> &mut Store<WasmPluginState> {
        &mut self.store
    }

    /// 插件指标句柄（调用计时 / 生命周期记账 / 燃料记账入口）
    pub(crate) fn metrics(&self) -> Arc<crate::monitor::PluginMetrics> {
        self.store.data().metrics.clone()
    }

    /// 燃料规格快照 `(enabled, budget)`（Store 内已固化，实例生命周期内不变）
    pub(crate) fn fuel_spec(&self) -> (bool, u64) {
        let state = self.store.data();
        (state.fuel_enabled, state.limits.fuel_budget())
    }

    /// 可选导出句柄快照（v11 / v14 / v20 动态探测结果；未导出 → None）
    pub(crate) fn optional_exports(&self) -> OptionalExports {
        let state = self.store.data();
        OptionalExports {
            on_message_binary: state.on_message_binary,
            on_ws_message: state.on_ws_message,
            on_ws_client_message: state.on_ws_client_message,
            on_task_event: state.on_task_event,
        }
    }

    /// 本次实例实际预打开成功的主机目录（空 = 无声明 / 未授权 / 创建失败）
    ///
    /// 宿主激活时的预打开目录漂移判定自票 06 起直接读装配条目的
    /// [`InstanceMeta::preopened_dirs`]（属主模型下不允许「锁实例读元数据」——
    /// 那是第二个 Store 入口）；本访问器保留给直接持有实例的测试。
    /// **仅 worker 类别可达**（ADR 0034）：非 worker 声明 preopen 已被加载期闸门拒绝。
    #[allow(dead_code)]
    pub(crate) fn preopened_dirs(&self) -> &[String] {
        &self.meta.preopened_dirs
    }

    /// 旧 SDK 产物的实例化失败判据 → 重建指引（空串 = 不是这个原因，不加噪音）
    ///
    /// 本项目迄今的破坏性契约变更（均在**实例化阶段**失败——比 `verify_abi`
    /// 的版本协商更早）：
    /// - **v27**：删了两个 import interface（`host-session` / `host-terminal`）与一个
    ///   export interface（`terminal-hooks`）；
    /// - **v28**：`host-events.broadcast-sync` 函数退役（websocket 业务下沉票 08——
    /// 旧 SDK 产物的实例化失败判据 → 重建指引（空串 = 不是这个原因，不加噪音）
    ///
    /// 项目不承担 ABI 向后兼容义务（研发起步期无外部公开契约，AGENTS.md §0）：破坏性
    /// 契约变更（删 import/export interface）在**实例化阶段**即失败——比 `verify_abi`
    /// 的版本协商更早。组件模型不提供「向后兼容的缺省 import」，失败本身不可避免，
    /// 本条只负责把失败**可诊断**：wasmtime 原文点名缺失的 import/export（文案形态
    /// 「unknown import \`…\` has not been defined」、「… matching implementation
    /// was not found in the linker」、「component does not export …」），这里补一句
    /// 统一的「按当前 SDK 重建」指引。
    ///
    /// 不再按历史 ABI 版本枚举判据矩阵（v27/v28/v31/v33/v36/v37… 逐版文案与「升级
    /// BedCode」反向分支）：研发起步期无契约旧产物在库，「哪个版本、重建到哪个版本」
    /// 的细节没有诊断对象，保留矩阵只会随每次删项累积维护负担。判据收敛为 wasmtime
    /// 的「缺失 import/export」通用文案，命中则给通用重建指引；与契约无关的失败
    /// （WASI 缺目录 / trap）返回空串，避免噪音掩盖真因。
    ///
    /// 抽成自由函数是为了可单测：判据是「错误文本 → 是否附指引」，与 Store 无关。
    pub(super) fn stale_artifact_rebuild_hint(instantiate_error: &str) -> String {
        // 判据锚 = wasmtime 组件实例化期的缺失 import/export 文案：
        // - `unknown import \`...\` has not been defined`（单函数/import 缺实现形态）
        // - `... matching implementation was not found in the linker`（import instance 形态）
        // - `component does not export ...`（缺必选导出形态）
        // 三者之外的失败（WASI 缺目录、trap、认证文案等）与旧契约无关，不加噪音。
        let is_stale_contract = instantiate_error.contains("has not been defined")
            || instantiate_error.contains("not found in the linker")
            || instantiate_error.contains("component does not export");
        if !is_stale_contract {
            return String::new();
        }
        "（该产物由其他插件协议（ABI）的插件 SDK 构建，请用当前 SDK 重建插件产物）".to_string()
    }
    /// - `abi.version()` 语义与 `abi::ABI_VERSION` 完全一致
    /// - `abi.form()` 必须为 1（component 形态）；0 是 core 形态的自研 ABI
    fn verify_abi(store: &mut Store<WasmPluginState>, instance: &Instance) -> crate::Result<()> {
        // 本路径不经 exports()（实例化后立即校验），独立重置燃料
        engine::inject_call_fuel(store)?;
        let exports = Plugin::new(&mut *store, instance)
            .map_err(|e| AppError::Plugin(format!("WASM component missing required exports: {}", e)))?;
        // v36（票 02 批次 06）：`form` 自 `abi` 拆入独立接口 `abi-form`（桌面独有
        // cap 导出），SDK 按新契约导出两接口（旧 v35 产物缺 abi-form 导出 ⇒ 此处
        // 实例化期被拒，fail-visible 形态②，见 stale_artifact_rebuild_hint 判据扩展）
        let abi_form_guest = exports.bedcode_plugin_abi_form();

        // `abi.version` 走 core 绑定的原生探测（票 03）：`abi` 虽在 core.wit 却是
        // cap 追加的导出，不在 `world core` 表内 —— 绑进内核等于让内核对「谁的
        // abi」表态。原生 `TypedFunc` 探测零绑定零端差异，缺失返回 None 由本处
        // 按「旧 SDK 产物」判据处理。
        let version = crate::manager::runtime::core_binding::abi_version(store, instance)?
            .ok_or_else(|| {
                AppError::Plugin(
                    "WASM component missing required export `abi.version` \
                     （该产物由其他插件协议（ABI）的插件 SDK 构建，请用当前 SDK 重建插件产物）"
                        .to_string(),
                )
            })?;
        let form = block_on_async(async { abi_form_guest.call_form(&mut *store).await })
            .map_err(|e| AppError::Plugin(format!("WASM component abi-form.form() call failed: {}", e)))?;

        if form != abi::FORM_COMPONENT {
            return Err(AppError::Plugin(format!(
                "WASM component for plugin declares abi form {} (expected {})",
                form,
                abi::FORM_COMPONENT
            )));
        }
        if version > abi::ABI_VERSION {
            return Err(AppError::Plugin(format!(
                "Plugin requires ABI v{} but host supports v{} — please upgrade BedCode",
                version,
                abi::ABI_VERSION
            )));
        }

        // v11：动态探测可选导出 events-binary#on-message-binary。该接口不声明进
        // plugin world（旧插件必选导出会因缺失而实例化失败），此处按名探测，
        // 缺失容忍为 None——旧插件只收 JSON，二进制消息由总线按格式不匹配拒绝。
        // 必须用 ItemName 路径语法（`iface.func` 点号）：组件的接口导出是嵌套
        // 实例形态，`iface#func` 平名字符串的 str 查找恒不命中（wasmtime 47 实证，
        // 票据 06 修复——此前平名探测恒返回 None，二进制导出从未真正被发现）
        let on_message_binary = "bedcode:plugin/events-binary.on-message-binary"
            .parse::<wasmtime::component::wit_parser::ItemName>()
            .ok()
            .and_then(|item| {
                instance
                    .get_typed_func::<(String, String, Vec<u8>), ()>(&mut *store, &item)
                    .ok()
            });
        store.data_mut().on_message_binary = on_message_binary;

        // v14：动态探测可选导出 events-ws（两条回调分别探测，缺失容忍为 None）。
        // 语义与 events-binary 同构：未导出 → 状态事件照收（bus），消息帧丢弃 +
        // 首次 warn + 计数（宿主不缓存，spec §2.2 D2）；旧插件加载零回归。
        // 同样必须用 ItemName 路径语法（`iface.func` 点号），理由同上
        let on_ws_message = "bedcode:plugin/events-ws.on-message"
            .parse::<wasmtime::component::wit_parser::ItemName>()
            .ok()
            .and_then(|item| {
                instance
                    .get_typed_func::<(String, String, Vec<u8>), ()>(&mut *store, &item)
                    .ok()
            });
        let on_ws_client_message = "bedcode:plugin/events-ws.on-client-message"
            .parse::<wasmtime::component::wit_parser::ItemName>()
            .ok()
            .and_then(|item| {
                instance
                    .get_typed_func::<(String, String, String, Vec<u8>), ()>(&mut *store, &item)
                    .ok()
            });
        let data = store.data_mut();
        data.on_ws_message = on_ws_message;
        data.on_ws_client_message = on_ws_client_message;

        // v20：动态探测可选导出 events-task#on-task-event（宿主并发任务进度/终态
        // 回调）。语义同 events-binary / events-ws：旧 SDK 产物未导出 → None，
        // 消费任务内降级（事件丢弃 + 首次 warn + 计数，宿主不缓存）；新 SDK
        // 产物由 wasm_entry! 无条件导出默认空实现 → 探测命中。
        // 同样必须用 ItemName 路径语法（`iface.func` 点号），理由同 events-ws
        let on_task_event = "bedcode:plugin/events-task.on-task-event"
            .parse::<wasmtime::component::wit_parser::ItemName>()
            .ok()
            .and_then(|item| instance.get_typed_func::<(String,), ()>(&mut *store, &item).ok());
        store.data_mut().on_task_event = on_task_event;
        Ok(())
    }

    /// 单次调用燃料预算续费（世界导出与能力转发调用共用）：
    /// 实现见 [`engine::refill_call_fuel`]（双端唯一一份）
    fn refill_call_fuel(&mut self) -> crate::Result<()> {
        engine::refill_call_fuel(&mut self.store)
    }

    /// 获取 world 导出绑定（每次调用重新索引导出，开销可忽略）
    ///
    /// 所有导出调用都经过此处：顺带重置燃料预算（单次调用预算，
    /// 宿主调用阻塞不消耗燃料，见 core-config FUEL_PER_CALL 说明）
    fn exports(&mut self) -> crate::Result<Plugin> {
        self.refill_call_fuel()?;
        Plugin::new(&mut self.store, &self.instance)
            .map_err(|e| AppError::Plugin(format!("WASM component exports access failed: {}", e)))
    }

    /// 获取 **core 交集** 导出绑定（command / lifecycle / manifest）
    ///
    /// 与 [`Self::exports`] 的分工：core 交集导出走 `runtime::core_binding` 的
    /// 绑定（双端单份，绑契约真源 `world core`），cap 导出（`events` 等，各端
    /// 形状不同）仍走本壳自有绑定。两者对**同一个** `Instance` 建视图，
    /// 不重复实例化、互不影响。
    ///
    /// 燃料续费语义与 [`Self::exports`] 同（单次调用预算，宿主阻塞期不消耗）。
    fn core_exports(&mut self) -> crate::Result<crate::manager::runtime::core_binding::CoreExports> {
        self.refill_call_fuel()?;
        crate::manager::runtime::core_binding::exports(&mut self.store, &self.instance)
    }

    /// 能力导出调用（core-plugin-manager 装配框架）：按名动态获取组件的
    /// 能力接口导出函数并调用，燃料预算续费语义同 [`Self::exports`]。
    ///
    /// 宿主侧转发专用：能力注册表命中系统组件提供者时，host_impl 宿主函数
    /// 把应用插件的 import 调用经此方法转发到提供者的同形导出。
    /// 外层 Err = trap/导出缺失等传输层错误；内层 `Results` 元组含 WIT
    /// `result<T, string>` 本体（guest 自报错误），两层语义分离。
    ///
    /// `export_name` 用 `ItemName` 路径语法（`pkg:ns/iface.func`，组件接口
    /// 导出为嵌套实例形态，平名 `iface#func` 无法命中）
    /// pub（票 05c）：lib 集成测试（auth_center_perf 探针）直接驱动能力导出
    pub fn call_capability_export<Params, Results>(
        &mut self,
        export_name: &str,
        params: Params,
    ) -> crate::Result<Results>
    where
        Params: wasmtime::component::ComponentNamedList + wasmtime::component::Lower + Send,
        // async 调用经 block_on_async 驱动：未来输出（Results）需 'static + Send，
        // Params 移入未来需 Send（能力转发代调用方均为 'static 元组，见 capability.rs）
        Results: wasmtime::component::ComponentNamedList + wasmtime::component::Lift + Send + 'static,
    {
        let _timer = self.track_call();
        self.refill_call_fuel()?;
        let item: wasmtime::component::wit_parser::ItemName = export_name
            .parse()
            .map_err(|e| AppError::Plugin(format!("invalid capability export name: {} ({})", export_name, e)))?;
        let func = self
            .instance
            .get_typed_func::<Params, Results>(&mut self.store, &item)
            .map_err(|e| AppError::Plugin(format!("capability export not found: {} ({})", export_name, e)))?;
        block_on_async(async { func.call_async(&mut self.store, params).await })
            .map_err(|e| AppError::Plugin(format!("capability call failed: {} ({})", export_name, e)))
    }

    /// 实例化时探测到的可路由能力接口导出（core-plugin-manager）；
    /// 空 = 纯应用插件（不提供能力）
    ///
    /// 宿主侧（认证中心候选 / 系统组件装配）自票 06 起直接读装配条目的
    /// [`InstanceMeta::exported_capabilities`]，本访问器保留给测试。
    #[allow(dead_code)]
    /// pub（票 05b）：lib 集成测试断言实例导出能力集（system_component_test）
    pub fn exported_capabilities(&self) -> &[String] {
        &self.meta.exported_capabilities
    }

    /// 导出调用计时起点（core-monitor 埋点）：返回的 RAII 计时器在
    /// 方法返回时落账（次数/耗时直方图）。每导出方法首行调用；实现见
    /// [`engine::start_call_timer`]
    fn track_call(&self) -> crate::monitor::CallTimer {
        engine::start_call_timer(&self.store)
    }

    /// 调用插件的 activate 导出
    /// pub（票 05b）：lib 集成测试（session_e2e）直接驱动真实产物激活
    pub fn activate(&mut self) -> crate::Result<i32> {
        let _timer = self.track_call();
        let exports = self.core_exports()?;
        let lifecycle = exports.bedcode_plugin_lifecycle();
        match block_on_async(async { lifecycle.call_activate(&mut self.store).await }) {
            Ok(Ok(())) => {
                self.store.data().metrics.record_lifecycle(LifecycleEvent::ActivateOk);
                Ok(0)
            }
            Ok(Err(msg)) => {
                self.store.data().metrics.record_lifecycle(LifecycleEvent::ActivateFail);
                Err(AppError::Plugin(format!("WASM activate() failed: {}", msg)))
            }
            Err(e) => {
                self.store.data().metrics.record_lifecycle(LifecycleEvent::ActivateFail);
                Err(engine::trapped(&self.store, "activate", e))
            }
        }
    }

    /// 调用插件的 deactivate 导出
    /// pub（票 05b）：lib 集成测试直接驱动真实产物停用
    pub fn deactivate(&mut self) -> crate::Result<i32> {
        let _timer = self.track_call();
        let exports = self.core_exports()?;
        let lifecycle = exports.bedcode_plugin_lifecycle();
        match block_on_async(async { lifecycle.call_deactivate(&mut self.store).await }) {
            Ok(Ok(())) => {
                self.store.data().metrics.record_lifecycle(LifecycleEvent::Deactivate);
                Ok(0)
            }
            Ok(Err(msg)) => Err(AppError::Plugin(format!("WASM deactivate() failed: {}", msg))),
            Err(e) => {
                Err(engine::trapped(&self.store, "deactivate", e))
            }
        }
    }

    /// 调用插件的 invoke_command 导出（JSON 载荷保留，语义与 core 路径 1:1）
    /// pub（票 05b）：lib 集成测试经插件命令面驱动被测行为
    pub fn invoke_command(&mut self, command_name: &str, args_json: &str) -> crate::Result<String> {
        let _timer = self.track_call();
        let exports = self.core_exports()?;
        let cmd = exports.bedcode_plugin_command();
        match block_on_async(async { cmd.call_invoke(&mut self.store, command_name, args_json).await }) {
            Ok(v) => Ok(v),
            Err(e) => {
                Err(engine::trapped(&self.store, "invoke_command", e))
            }
        }
    }

    // v27（票 10 删除的 `on_terminal_input` / `on_terminal_output`）：
    // 对应的 `terminal-hooks` interface 已从 WIT world 删除（票 03 起它已无派发源：
    // 逐帧输入修饰链与终端输出修饰链都不再由宿主调用）。宿主要求的导出集因此少一个
    // interface，SDK 侧对应绑定与缺口说明见 `packages/plugin-sdk-desktop/rust`。

    /// 调用插件的 on_startup 导出
    ///
    /// 双层 Result 语义：外层 = 调用本身失败（trap / 导出缺失 / 燃料耗尽），
    /// 内层 = guest 报告的启动初始化结果（v8 契约 `result<_, string>`）。
    /// 宿主据此区分「插件自报启动失败 → Degraded」与「调用故障」
    pub(crate) fn on_startup(&mut self) -> crate::Result<std::result::Result<(), String>> {
        let _timer = self.track_call();
        let exports = self.core_exports()?;
        let lifecycle = exports.bedcode_plugin_lifecycle();
        match block_on_async(async { lifecycle.call_on_startup(&mut self.store).await }) {
            Ok(v) => Ok(v),
            Err(e) => {
                Err(engine::trapped(&self.store, "on_startup", e))
            }
        }
    }

    /// 调用插件的 on_shutdown 导出
    ///
    /// 双层 Result 语义同 [`Self::on_startup`]；停用流程对 guest 报告的
    /// 清理失败仅记录，不影响状态机
    pub(crate) fn on_shutdown(&mut self) -> crate::Result<std::result::Result<(), String>> {
        let _timer = self.track_call();
        let exports = self.core_exports()?;
        let lifecycle = exports.bedcode_plugin_lifecycle();
        match block_on_async(async { lifecycle.call_on_shutdown(&mut self.store).await }) {
            Ok(v) => Ok(v),
            Err(e) => {
                Err(engine::trapped(&self.store, "on_shutdown", e))
            }
        }
    }

    /// 调用插件的消息总线消息接收导出
    pub(crate) fn on_message(&mut self, topic: &str, sender: &str, payload: &serde_json::Value) -> crate::Result<()> {
        let _timer = self.track_call();
        let payload_str = serde_json::to_string(payload).unwrap_or_default();
        let exports = self.exports()?;
        let events = exports.bedcode_plugin_events();
        match block_on_async(async {
            events
                .call_on_message(&mut self.store, topic, sender, &payload_str)
                .await
        }) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(msg)) => {
                tracing::warn!("WASM on_message() failed: {}", msg);
                Ok(())
            }
            Err(e) => {
                Err(engine::trapped(&self.store, "on_message", e))
            }
        }
    }

    /// 调用插件的消息总线二进制消息接收导出（v11，可选导出动态探测）
    ///
    /// 仅当实例在实例化时探测到 `events-binary#on-message-binary` 导出才可调用；
    /// 总线侧已按订阅者格式偏好过滤，正常不会对无导出的实例发起本调用，
    /// 此处防御性拒绝（旧插件二进制 → 格式不匹配拒绝的语义由总线保证）
    pub(crate) fn on_message_binary(&mut self, topic: &str, sender: &str, payload: &[u8]) -> crate::Result<()> {
        let _timer = self.track_call();
        // 燃料续费：本方法不经 `exports()`（直接持探测句柄调用），必须自带
        // 续费，否则本回调的燃料跨调用累积耗尽 → trap → Store 中毒 → 自动重载
        self.refill_call_fuel()?;
        // TypedFunc 先 clone 再调用，避免与 &mut self.store 的借用冲突
        let Some(func) = self.store.data().on_message_binary.clone() else {
            return Err(AppError::Plugin(format!(
                "WASM plugin has no events-binary export (v11 required)"
            )));
        };
        // 无返回值（观察型回调）：guest 内部失败经 host-log 记录；此处仅 trap 上抛
        block_on_async(async {
            func.call_async(
                &mut self.store,
                (topic.to_string(), sender.to_string(), payload.to_vec()),
            )
            .await
        })
        .map_err(|e| {
            engine::trapped(&self.store, "on_message_binary", e)
        })
    }

    /// 调用插件的 WS 帧接收导出（v14，可选导出动态探测）
    ///
    /// 返回 `Ok(true)` = 已投递；`Ok(false)` = 该回调未导出——调用方按 spec §2.2
    /// 降级（丢弃 + 首次 warn + 计数，宿主不缓存）。无返回值（观察型回调）：
    /// guest 内部失败经 host-log 记录；此处仅 trap 上抛
    pub(crate) fn on_ws_frame(&mut self, frame: &crate::bus::WsFrameDispatch) -> crate::Result<bool> {
        use crate::bus::WsFrameDispatch;
        let _timer = self.track_call();
        // 燃料续费：同 on_message_binary——不经 exports()，须自带续费
        self.refill_call_fuel()?;
        match frame {
            WsFrameDispatch::Client { handle, kind, payload } => {
                // TypedFunc 先 clone 再调用，避免与 &mut self.store 的借用冲突
                let Some(func) = self.store.data().on_ws_message.clone() else {
                    return Ok(false);
                };
                block_on_async(async {
                    func.call_async(&mut self.store, (handle.clone(), kind.clone(), payload.clone()))
                        .await
                })
                .map_err(|e| {
                    engine::trapped(&self.store, "on_ws_message", e)
                })?;
                Ok(true)
            }
            WsFrameDispatch::EndpointClient {
                endpoint_id,
                client_id,
                kind,
                payload,
            } => {
                let Some(func) = self.store.data().on_ws_client_message.clone() else {
                    return Ok(false);
                };
                block_on_async(async {
                    func.call_async(
                        &mut self.store,
                        (endpoint_id.clone(), client_id.clone(), kind.clone(), payload.clone()),
                    )
                    .await
                })
                .map_err(|e| {
                    engine::trapped(&self.store, "on_ws_client_message", e)
                })?;
                Ok(true)
            }
        }
    }

    /// 调用插件的宿主并发任务事件导出（v20，可选导出动态探测）
    ///
    /// 返回 `Ok(true)` = 已投递；`Ok(false)` = 该回调未导出——消费派发任务按
    /// spec §5.3 降级（事件丢弃 + 首次 warn + 计数，宿主不缓存；`status`/
    /// `list-jobs` 自愈）。无返回值（观察型回调，同 `on_ws_frame`）。
    ///
    /// pub（票 02 批次 05）：宿主集成测试 `tests/task_e2e.rs` 直驱 SDK 回调链
    /// （dispatch → on_task_event → events-task 导出 → fixture 收集）——迁移前
    /// 内核 task_e2e 在 lib 内直调同款；与 `activate` / `invoke_command` 的
    /// 集成测试公开面先例同口径。
    pub fn on_task_event(&mut self, event_json: &str) -> crate::Result<bool> {
        let _timer = self.track_call();
        // 燃料续费：同 on_message_binary——不经 exports()，须自带续费
        self.refill_call_fuel()?;
        // TypedFunc 先 clone 再调用，避免与 &mut self.store 的借用冲突
        let Some(func) = self.store.data().on_task_event.clone() else {
            return Ok(false);
        };
        block_on_async(async { func.call_async(&mut self.store, (event_json.to_string(),)).await }).map_err(|e| {
            engine::trapped(&self.store, "on_task_event", e)
        })?;
        Ok(true)
    }

    // v27（票 10 删除的 `on_session_lifecycle` / `on_input_submitted`）：
    // `events` interface 里的这两个导出已从 WIT world 删除。派发源在票 03 就没了
    // （宿主侧观察注册表 + 派发点退役），留着「实现了但永不触发」的导出正是票 03
    // 明确禁止的状态。会话生命周期事实由插件自驱、跨插件事件走 `host-events` /
    // `host-bus`。`on-message` / `on-process-done` 仍是必选导出，保留。

    /// 调用插件的进程执行完成事件导出（host-process，v8）
    pub(crate) fn on_process_done(&mut self, payload_json: &str) -> crate::Result<()> {
        let _timer = self.track_call();
        let exports = self.exports()?;
        let events = exports.bedcode_plugin_events();
        match block_on_async(async { events.call_on_process_done(&mut self.store, payload_json).await }) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(msg)) => {
                tracing::warn!("WASM on_process_done() failed: {}", msg);
                Ok(())
            }
            Err(e) => {
                Err(engine::trapped(&self.store, "on_process_done", e))
            }
        }
    }

    /// 获取插件的 manifest JSON
    #[allow(dead_code)] // 测试覆盖,生产侧 manifest 走其他加载路径
    /// pub（票 05b）：lib 集成测试读插件 manifest（api 清单比对）
    pub fn get_manifest(&mut self) -> crate::Result<String> {
        let _timer = self.track_call();
        let exports = self.core_exports()?;
        let manifest = exports.bedcode_plugin_manifest();
        match block_on_async(async { manifest.call_get(&mut self.store).await }) {
            Ok(v) => Ok(v),
            Err(e) => {
                Err(engine::trapped(&self.store, "manifest", e))
            }
        }
    }

    /// 测试访问器：直接获取 Store/Instance（燃料断言与耗尽 trap 测试用）
    #[cfg(test)]
    pub(crate) fn raw_store(&mut self) -> (&mut Store<WasmPluginState>, &Instance) {
        (&mut self.store, &self.instance)
    }
}

// ==================== WASI 预打开（仅 worker 类别，ADR 0034） ====================

/// 构建 WASI 上下文：将 manifest 声明（`wasiPreopenDirs`，经展开+授权过滤）的
/// 目录逐项预打开到 guest 路径 `/data`、`/data1`、…；无声明时为空上下文。
/// 单项失败仅告警不阻断（该目录 guest 不可见，由插件激活时自检并引导用户）。
///
/// **仅 worker 类别（`lifecycle: ephemeral`，ADR 0032 L3.b 预留）可用**（ADR 0034）：
/// 主 wasm-app 文件访问一律走宿主 `host-fs` 授权机制，非 worker 声明 preopen 在
/// 构建期与加载期双侧显性拒绝——本函数是 worker 启用后的预留能力装配点，
/// 当前对一切可加载 manifest 不可达（worker 未实现）。
///
/// 挂载档由条目声明决定（审计票 07 只读档）：`readonly: true` → `FsPerms::ReadOnly`，
/// 其余（裸路径 / `readonly` 缺省或 false）→ `FsPerms::ReadWrite`，与改造前逐字一致。
/// 档位**不影响是否放行**——未授权目录两档都建不出 preopen（`resolve_preopen_dirs`）。
///
/// 返回 `(WasiCtx, 实际预打开成功目录)`：后者供激活时判定「新授权目录是否已
/// 纳入当前实例」（见 [`LoadedWasmPlugin::preopened_dirs`]）。
///
/// 目录创建是宿主职责：授权即代表用户同意插件在该路径写数据；而
/// `preopened_dir` 要求目录已存在（wasmtime 语义），首次启用前数据目录通常
/// 尚未创建，缺失会导致 preopen 静默失败、/data 挂不上。故此处先幂等
/// `create_dir_all` 再挂载——修复首次启用「WASI 预打开目录未就绪」死循环。
/// 只读档同样先建目录：宿主建空目录只是给 guest 一个可挂载的锚点，guest 自己
/// 仍写不进去——写能力在 WASI 层由 `OpenMode` 拒掉（见 wasmtime-wasi 的
/// `preopened_dir`：`ReadOnly` → `OpenMode::READ`）。
pub(crate) fn build_wasi_ctx(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    declared_dirs: &[WasiPreopenDir],
) -> (wasmtime_wasi::WasiCtx, Vec<String>) {
    let mut builder = WasiCtxBuilder::new();
    let mut preopened = Vec::new();
    for (i, dir) in resolve_preopen_dirs(host_ctx, plugin_id, declared_dirs)
        .into_iter()
        .enumerate()
    {
        // 首个声明挂载到 /data（WASI 插件约定根），后续依次 /data1、/data2…
        let guest_path = if i == 0 {
            "/data".to_string()
        } else {
            format!("/data{}", i)
        };
        let host_path = dir.path().to_string();
        let (perms, perms_label) = if dir.readonly() {
            (FsPerms::ReadOnly, "read-only")
        } else {
            (FsPerms::ReadWrite, "read-write")
        };
        if let Err(e) = std::fs::create_dir_all(&host_path) {
            tracing::warn!(
                plugin_id = %plugin_id,
                dir = %host_path,
                error = %e,
                "WASI preopen dir creation failed, directory not visible to plugin"
            );
            continue;
        }
        match builder.preopened_dir(&host_path, &guest_path, perms) {
            Ok(_) => {
                tracing::info!(
                    plugin_id = %plugin_id,
                    dir = %host_path,
                    perms = %perms_label,
                    "WASI preopened dir at {} (manifest declared)",
                    guest_path
                );
                preopened.push(host_path);
            }
            Err(e) => {
                tracing::warn!(
                    plugin_id = %plugin_id,
                    dir = %host_path,
                    error = %e,
                    "WASI preopen failed, directory not visible to plugin"
                );
            }
        }
    }
    (builder.build(), preopened)
}

/// 展开 manifest `wasiPreopenDirs` 声明为主机路径候选列表（挂载档原样带过）
///
/// 纯字符串处理（`${home}` 展开 + 剥尾部分隔符 + 滤空），不查授权、不依赖
/// tokio 运行时——preauthorize 阶段用它收集「需弹窗授权」的路径候选
/// （`resolve_preopen_dirs` 在此基础上再过滤未授权项）。
pub(crate) fn expand_preopen_declarations(plugin_id: &str, declared_dirs: &[WasiPreopenDir]) -> Vec<WasiPreopenDir> {
    expand_preopen_declarations_with_home(plugin_id, declared_dirs, None)
}

/// [`expand_preopen_declarations`] 的可注入 home 变体（测试用临时目录构造伪 HOME，
/// 避免依赖真实 `$HOME` 导致的无主目录环境静默跳过，dev 合入）
fn expand_preopen_declarations_with_home(
    plugin_id: &str,
    declared_dirs: &[WasiPreopenDir],
    home_override: Option<&std::path::Path>,
) -> Vec<WasiPreopenDir> {
    let home = home_override
        .map(|p| p.to_string_lossy().trim_end_matches(['/', '\\']).to_string())
        .or_else(|| dirs::home_dir().map(|p| p.to_string_lossy().trim_end_matches(['/', '\\']).to_string()));
    declared_dirs
        .iter()
        .filter_map(|decl| {
            let dir = match home.as_ref() {
                Some(home) => decl.path().trim().replacen("${home}", home, 1),
                None => {
                    tracing::warn!(plugin_id = %plugin_id, "wasiPreopenDirs: home_dir unavailable, skipping");
                    return None;
                }
            };
            // 只剥尾部分隔符：trim_matches 会连 POSIX 绝对路径的起始 '/' 一起
            // 剥掉（/tmp/x → tmp/x），相对化后授权匹配必然失败——Windows 盘符
            // 前缀掩盖了此问题，Linux 首次跑通前从未暴露
            let dir = dir.trim().trim_end_matches(['/', '\\']).trim_end().to_string();
            (!dir.is_empty()).then(|| decl.clone().with_path(dir))
        })
        .collect()
}

/// 解析 manifest `wasiPreopenDirs` 声明为可预打开的主机路径列表（含挂载档）
///
/// **仅 worker 类别可用**（ADR 0034，机制保留为 worker 预留能力）；主 wasm-app
/// 不经过此路径（非 worker 声明 preopen 已被加载期闸门拒绝）。
///
/// - 先经 [`expand_preopen_declarations`] 展开
/// - 仅保留已授权目录（is_granted 无弹窗校验）：manifest 路径可能指向任意
///   主机位置，不得绕过授权机制建立预打开。只读档同样要授权——档位只收紧
///   guest 能力，不放宽宿主边界（票 07 裁决 3）
/// - **所需能力按档位取**（票 02 操作拆分）：只读档要读授权，读写档要读 + 写授权。
///   预打开把这两项能力直接交给 guest（WASI 层不再过宿主闸门），所以授权记录必须
///   覆盖对应能力——否则「授权读」的目录换来一个可写 preopen，操作拆分被原地架空
/// - 无 tokio 运行时上下文（无头场景）无法查询授权 → 返回空（不阻断加载）
pub(crate) fn resolve_preopen_dirs(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    declared_dirs: &[WasiPreopenDir],
) -> Vec<WasiPreopenDir> {
    resolve_preopen_dirs_with_home(host_ctx, plugin_id, declared_dirs, None)
}

/// [`resolve_preopen_dirs`] 的可注入 home 变体（测试用临时目录构造伪 HOME，dev 合入）
fn resolve_preopen_dirs_with_home(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    declared_dirs: &[WasiPreopenDir],
    home_override: Option<&std::path::Path>,
) -> Vec<WasiPreopenDir> {
    if tokio::runtime::Handle::try_current().is_err() {
        return Vec::new();
    }
    use crate::security::fs_auth::FsOps;
    expand_preopen_declarations_with_home(plugin_id, declared_dirs, home_override)
        .into_iter()
        .filter(|dir| {
            let needed = if dir.readonly() { FsOps::READ } else { FsOps::READ_WRITE };
            // 票 02 B5：`is_granted` 是 FsAuthChecker 具体机制方法（不在窄
            // FsAuthGate trait 上）——经下转型访问器取回（桌面装配恒 Some）
            let checker = host_ctx
                .fs_auth_checker();
            block_on_async(checker.is_granted(plugin_id, dir.path(), needed))
        })
        .collect()
}

// ==================== 测试 ====================

#[cfg(test)]
mod tests {

    use super::*;
    // 复用 host_impl 测试基建（host_impl::tests 为 pub(super)，同子树可访问）
    use crate::host_api::tests::build_host_ctx;
    use crate::host_api::grant_permissions;

    /// 测试用插件 ID（与 wasm_runtime.rs 测试一致，主库表前缀校验依赖它）
    const TEST_PLUGIN_ID: &str = "com.bedcode.test";

    /// 造一个**已装配 sqlite 域端口**的无头上下文
    ///
    /// 构建测试引擎：**直接走生产构建函数**（`engine::build_engine_config` +
    /// 编译期默认 `CoreConfig`），不再在测试里手工重摆一遍 fuel / backtrace /
    /// 内存预留——手工副本会与生产配置漂移（历史上移动形态就因为各钉一份字面量
    /// 而漏掉了 debug 放大倍率与 backtrace 帧数），且 `Store::set_fuel` 在
    /// consume_fuel 未开启时会在实例化直接报错，trap 错误串形态也会与生产不一致。
    fn test_engine() -> wasmtime::Engine {
        let core_config = crate::config::CoreConfig::default();
        let config = engine::build_engine_config(&core_config, crate::config::plugin_debug_mode());
        wasmtime::Engine::new(&config).expect("create test engine")
    }

    /// 构建测试用组件插件并编码为组件
    ///
    /// 测试插件为独立 crate（packages/plugin-component-test），基于
    /// WIT 契约（packages/plugin-sdk-desktop/rust/wit）生成绑定；
    /// 源码变更检测与 wasm_runtime.rs 测试同策略（产物存在且源码未更新
    /// 时直接复用，避免每次跑测试都触发 cargo build）
    ///
    /// `BEDCODE_PLUGIN_DEBUG=1`（dev 构建下）时以 debug profile 构建（保留
    /// DWARF 行号，供行号冒烟测试断言 trap 错误串含 file:line）
    /// 构建测试用组件插件（合集 `packages/plugin-sdk-fixtures` `feature = "sdk"`）
    ///
    /// 原先指向 `packages/plugin-component-test`（手写 wit-bindgen 绑定的独立夹具），
    /// 夹具删除后改用 SDK 夹具——宿主加载组件的代码路径与客体绑定方式无关。
    /// profile 分档（`BEDCODE_PLUGIN_DEBUG=1` 走 debug 取 DWARF 行号）由
    /// `fixture_build::build_sdk_fixture` 内部处理。
    fn build_test_component() -> Vec<u8> {
        crate::manager::runtime::fixture_build::build_sdk_fixture("sdk")
    }

    // ==================== 分片测试（common/component/tests/） ====================
    //
    // 自本文件的 `mod tests` 拆出（共享脚手架在 `mod tests`，经 `use super::*` 可见）：
    // - whitelist.rs：能力模块白名单双向校验 + 描述符零产品名词锁
    // - linker.rs：add_to_linker 注册闭环 + 组件完整往返
    // - preopen.rs：WASI 预打开目录解析与 build_wasi_ctx 装配
    // - stale_hint.rs：旧 SDK 产物实例化失败的 ABI 版本重建指引判据
    mod whitelist;
    mod linker;
    mod preopen;
    mod stale_hint;
}
