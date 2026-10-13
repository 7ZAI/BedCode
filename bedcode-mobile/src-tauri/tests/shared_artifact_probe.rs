//! T3 探针：移动宿主加载**桌面构建**的共享 file-transfer 产物（规格
//! `.scratch/2026-10-13-file-transfer-single-wasm/spec.md` §3 T3 / §4 R1·R2）
//!
//! ## 为什么需要它
//!
//! file-transfer 双端只构建**一份** wasm：产物源在桌面 app（按桌面 `world plugin`
//! 构建），移动端只保留前端与 manifest。这带来三个「不测就会静默腐烂」的风险点：
//!
//! 1. **import 闭合**（R1）：该产物的 import 面含两个桌面专属 interface
//!    （`host-api-call` / `host-platform-desktop`），移动 world 不提供。组件模型没有
//!    「缺省 import」——一旦 `plugin/desktop_compat.rs` 的桩漏了任一函数、或桩模块
//!    没被 inventory 收集，**实例化期**才炸，且报错只说「missing import」，很难反推
//!    是哪个桩漏了。本用例是「桩够不够」的唯一回归点。
//! 2. **导出并集视图**（R2）：产物按桌面 world 构建，其中 `events` 被扩为**七函数
//!    并集**（桌面 2 + 移动 5）。wit-bindgen 的导出视图对「本 world 不认识的多余函
//!    数」是否宽容，只能实证——本用例派发移动词汇 `on-bus-message`，走真实 dispatcher。
//! 3. **平台分支真的翻到移动侧**：桩就位只保证「能加载」，不保证「走对了分支」。
//!    用桌面专属命令 `file-transfer.reveal-in-dir` 做反向探针：移动侧必须返回
//!    「unsupported on mobile」——若它反而成功，说明 `host-platform.platform()`
//!    探测没生效、桌面分支被跑在了移动宿主上（那是最危险的失效形态）。
//!
//! ## 产物与 manifest 取哪一份
//!
//! - **产物取桌面 resources 那一份**：这才是 T3 的命题本身（移动宿主加载桌面产物）。
//!   移动 resources 那份只用于字节一致性断言（T6 门禁：两端产物同字节）。
//! - **权限取移动端 plugin.json**：宿主从**文件**读 manifest（wasm 内嵌 manifest 仅
//!   测试消费），探针必须按真实加载路径给权限，否则测的是「无权限特例」而非生产形态。
//!
//! 产物缺失时本用例**跳过并提示**（不 fail）：本机桌面 wasip3 / wasm32 产物并非总能
//!   重建（见 memory「环境限制」），把「产物没构建」判成代码回归是误报。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bedcode_wasm_core::manager::runtime::{AbiPolicy, WasmRuntime};
use bedcode_wasm_core::test_support::{build_host_ctx, runtime_engine};
use wasmtime::component::Component;

const PLUGIN_ID: &str = "com.bedcode.file-transfer";


/// 移动端产物文件名（build 脚本产物名，两端同名）
const ARTIFACT_FILE: &str = "bedcode_plugin_file_transfer.wasm";

fn mobile_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 桌面端 resources 下的共享产物（T3 的命题本体）
fn desktop_artifact() -> PathBuf {
    mobile_root()
        .join("../../bedcode-desktop/src-tauri/resources/plugins/desktop")
        .join(PLUGIN_ID)
        .join(ARTIFACT_FILE)
}

/// 移动端 resources 下的产物（T6 同字节门禁的比对侧）
fn mobile_artifact() -> PathBuf {
    mobile_root()
        .join("resources/plugins/mobile")
        .join(PLUGIN_ID)
        .join(ARTIFACT_FILE)
}

fn read_artifact(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("read artifact {}: {e}", path.display()))
}

/// 移动 manifest 声明的权限集（真实加载路径：宿主从文件读 manifest）
fn declared_permissions() -> HashSet<String> {
    let manifest_path = mobile_root()
        .join("resources/plugins/mobile")
        .join(PLUGIN_ID)
        .join("plugin.json");
    let raw = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|e| panic!("read manifest {}: {e}", manifest_path.display()));
    let manifest: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse manifest: {e}"));
    manifest["permissions"]
        .as_array()
        .expect("manifest.permissions must be an array")
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect()
}

/// 移动运行时装配（与生产 `PluginManager::init_wasm_runtime_with` 同款）：
/// 注入移动 guest 事件派发器——否则 `events` 导出走内核缺省实现，
/// 派发即 fail-visible（不注入时 `on-bus-message` 必 Err，探针会误判成 R2 失败）。
fn mobile_runtime(aot_cache_dir: Option<PathBuf>) -> WasmRuntime {
    // peer 域端口：共享产物 activate 必经 `host-peer` 原语（active-transfers 兜底），
    // 而该域的 `ports()` 未装配即 panic（域内刻意 fail-visible）。装的是**生产适配器的
    // 无头形态**（`HostPeerPorts::new(None)`：无 AppHandle → `peer_ctx` 汇报
    // HEADLESS_UNAVAILABLE、权限门 fail-safe 拒绝），不是自造替身——探针要验的是生产
    // 接线路径，无头降级正是无头环境的真实形态。crate 侧 OnceLock 幂等（首装胜出）。
    bedcode_mobile_lib::plugin::peer_ports::install(None);

    let runtime = bedcode_wasm_core::test_support::setup_headless_runtime_with_abi(
        aot_cache_dir,
        AbiPolicy { form: None },
    )
    .expect("create wasm runtime");
    runtime.set_guest_events(Arc::new(
        bedcode_mobile_lib::plugin::guest_events::MobileGuestEvents,
    ));
    runtime
}

/// 桩的完备性 = import 闭合：移动宿主实例化**桌面构建**的共享产物。
///
/// 这条红了只有两种可能：(a) `desktop_compat.rs` 的桩函数不全 / 未注册；
/// (b) 桌面 wit 演进后本模块的 `Host` impl 漏跟（那种情况应先在编译期红）。
#[test]
fn desktop_artifact_instantiates_on_mobile_host_closing_desktop_only_imports() {
    let artifact = desktop_artifact();
    if !artifact.exists() {
        eprintln!(
            "skip: 桌面共享产物未构建（{}）——先跑 `cd bedcode-desktop/wasm-apps/file-transfer && node scripts/build.js --rust-only`",
            artifact.display()
        );
        return;
    }
    let bytes = read_artifact(&artifact);

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        let runtime = mobile_runtime(None);
        // 必须用 runtime 自身 Engine 编译（跨 Engine 实例化被 wasmtime 拒绝）
        let component = Component::from_binary(runtime_engine(&runtime), &bytes)
            .expect("compile shared artifact as component");

        let host_ctx = build_host_ctx();
        let mut plugin = runtime
            .instantiate_component(
                &component,
                PLUGIN_ID,
                Arc::new(host_ctx.for_plugin(declared_permissions())),
                &[],
                None,
            )
            // 失败即 import 不闭合：错误文本会点名缺失的 interface
            .expect("instantiate desktop-built artifact on mobile host (imports must close)");

        // 版本一致性不再由产物自报整数（该机制随 wit 包版本化退役，ADR 0051 D2）：
        // 上面「实例化成功」本身就是最强断言——共享产物按桌面 world 构建，import 名
        // 带 `package bedcode:plugin@<version>`，移动宿主版本不等即实例化期被拒。
        // 版本真源与双端对齐由 `bedcode-desktop/src-tauri/tests/wit_package_version_lock.rs`
        // （10 源 + 9 生成物）与 `bedcode-file-transfer-core/src/wiring_lock.rs` 断言。

        // 产物 id 与宿主加载的 manifest 同源（防张冠李戴加载到别的插件产物）
        let manifest: serde_json::Value =
            serde_json::from_str(&plugin.get_manifest().expect("get manifest")).unwrap();
        assert_eq!(manifest["id"], PLUGIN_ID);
    });
}

/// R2 实证：移动词汇 `on-bus-message` 在桌面构建的产物上可被移动 dispatcher 找到
/// 并成功派发（并集导出视图宽容「本 world 不调用、但 world 认识」的函数）。
#[test]
fn mobile_vocabulary_event_dispatches_into_desktop_built_artifact() {
    let artifact = desktop_artifact();
    if !artifact.exists() {
        eprintln!("skip: 桌面共享产物未构建（{}）", artifact.display());
        return;
    }
    let bytes = read_artifact(&artifact);

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        let runtime = mobile_runtime(None);
        let component = Component::from_binary(runtime_engine(&runtime), &bytes)
            .expect("compile shared artifact");
        let host_ctx = build_host_ctx();
        let mut plugin = runtime
            .instantiate_component(
                &component,
                PLUGIN_ID,
                Arc::new(host_ctx.for_plugin(declared_permissions())),
                &[],
                None,
            )
            .expect("instantiate shared artifact");

        plugin.activate().expect("activate on mobile host");

        // 走内核统一入口 → 移动 dispatcher → guest `events.on-bus-message`。
        // payload 用未知 topic：guest 侧应无匹配处理而安静返回（事件投递是
        // fire-and-forget），这里的断言是「导出可被找到且调用成功」，不是业务语义。
        plugin
            .on_message("t3-probe:unknown-topic", "desktop", &serde_json::json!({}))
            .expect("dispatch on-bus-message into desktop-built artifact");

        plugin.deactivate().expect("deactivate");
    });
}

/// 平台分支探针：桌面专属命令在移动宿主必须显性 unsupported。
///
/// 反向失效信号：若该命令**成功**，说明 `host-platform.platform()` 探测没生效 /
/// 产物走了桌面分支——桩的存在反而掩盖了问题，这是最危险的形态，必须判红。
#[test]
fn desktop_only_command_is_visibly_unsupported_on_mobile_branch() {
    let artifact = desktop_artifact();
    if !artifact.exists() {
        eprintln!("skip: 桌面共享产物未构建（{}）", artifact.display());
        return;
    }
    let bytes = read_artifact(&artifact);

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        let runtime = mobile_runtime(None);
        let component = Component::from_binary(runtime_engine(&runtime), &bytes)
            .expect("compile shared artifact");
        let host_ctx = build_host_ctx();
        let mut plugin = runtime
            .instantiate_component(
                &component,
                PLUGIN_ID,
                Arc::new(host_ctx.for_plugin(declared_permissions())),
                &[],
                None,
            )
            .expect("instantiate shared artifact");

        plugin.activate().expect("activate on mobile host");

        // guest 命令面把失败包进返回 payload（`{"error": "..."}`）而非 `Result::Err`
        // ——命令面契约如此，故断言走 payload 文本。
        let payload = plugin
            .invoke_command(
                "file-transfer.reveal-in-dir",
                &serde_json::json!({ "path": "/tmp" }).to_string(),
            )
            .expect("invoke_command must return a payload");
        assert!(
            payload.contains("unsupported on mobile"),
            "桌面专属命令在移动分支必须显性 unsupported（fail-visible），实得：{payload}"
        );

        plugin.deactivate().expect("deactivate");
    });
}

/// T6 门禁：双端产物**同字节**。「一份 wasm」的价值全在这条断言上——两端各构建一次
/// 而产物不同字节，就等于两份实现悄悄分叉，且所有功能测试照样全绿。
#[test]
fn both_ends_ship_byte_identical_artifact() {
    let desktop = desktop_artifact();
    let mobile = mobile_artifact();
    if !desktop.exists() || !mobile.exists() {
        eprintln!(
            "skip: 产物缺失（desktop={} mobile={}）——两端各跑一次构建再验",
            desktop.exists(),
            mobile.exists()
        );
        return;
    }
    let d = read_artifact(&desktop);
    let m = read_artifact(&mobile);
    assert_eq!(
        d.len(),
        m.len(),
        "两端产物长度不同（{} vs {}）——构建链或工具链版本已分叉",
        d.len(),
        m.len()
    );
    assert!(
        d == m,
        "两端产物字节不同——「单产物」已退化为双份构建；查两端 build 脚本的 \
         target / 工具链 / feature 是否一致"
    );
}
