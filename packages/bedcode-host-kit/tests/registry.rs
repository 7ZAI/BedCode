//! 自动注册表的行为契约（正例 / 反例 / 边界）
//!
//! 依据 `unit-test-discipline`：每个用例断言**可观察行为**，不测 mock 调用次数；
//! 白名单锁必须**双向**都钉住（漏进 / 漏出），否则任一方向单独存在都能被绕过。

use bedcode_host_kit::{HostKitError, HostModule, HostModuleDesc, ModuleRegistry, WasmPluginState};
use wasmtime::component::Linker;

/// 测试用能力模块（替身：只登记一个空壳 interface 命名空间，不引入 WIT 依赖）
struct StubModule {
    name: &'static str,
    interfaces: &'static [&'static str],
}

impl HostModule for StubModule {
    fn desc(&self) -> HostModuleDesc {
        HostModuleDesc {
            name: self.name,
            interfaces: self.interfaces,
            permissions: &[],
            abi_min: 1,
        }
    }

    fn register(&self, linker: &mut Linker<WasmPluginState>) -> wasmtime::Result<()> {
        // 必须真往 instance 里**定义一个函数**——`Linker::instance(name)` 本身
        // 允许对同一名字重复开启（wasmtime 有意为之，见 `into_instance` 注释：
        // “explicitly allow re-opening an instance multiple times”），不报错。
        // 真正的重复注册护栏是「同一 instance 内同名函数」——也正是 wit-bindgen 的
        // `add_to_linker` 会碰的那条。
        let mut instance = linker.instance(self.interfaces[0])?;
        instance.func_new("ping", |_store, _ty, _args, _rets| Ok(()))?;
        Ok(())
    }
}

// 三个静态提交（名字刻意乱序，用于钉住「按名排序而非注册顺序」）
static ZETA: StubModule = StubModule {
    name: "zeta",
    interfaces: &["test:pkg/zeta"],
};
static ALPHA: StubModule = StubModule {
    name: "alpha",
    interfaces: &["test:pkg/alpha"],
};
static MID: StubModule = StubModule {
    name: "mid",
    interfaces: &["test:pkg/mid"],
};

fn test_engine() -> wasmtime::Engine {
    wasmtime::Engine::default()
}

/// 正例：三个自报模块全部被收集到，且按**模块名字典序**排列
///
/// 排序契约的价值：装配顺序固定 ⇒ 重复注册的报错信息可复现（否则错误串取决于
/// 链接器把 rlib 排在哪，属「同一份代码两次构建两种错误」的不可诊断态）。
#[test]
fn collected_returns_all_submitted_modules_sorted_by_name() {
    let registry = ModuleRegistry::collected();
    let names: Vec<&str> = registry.descs().iter().map(|d| d.name).collect();

    for expected in ["alpha", "mid", "zeta"] {
        assert!(
            names.contains(&expected),
            "module '{expected}' not collected: {names:?}"
        );
    }
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(
        names, sorted,
        "modules must be sorted by name, got {names:?}"
    );
    assert_eq!(registry.len(), 3, "exactly three stub modules registered");
    assert!(!registry.is_empty());
}

/// 描述符内容随模块一起被收集（接口路径 / 权限位 / ABI 下界三元组）
#[test]
fn collected_preserves_descriptor_payload() {
    let registry = ModuleRegistry::collected();
    let alpha = registry
        .descs()
        .into_iter()
        .find(|d| d.name == "alpha")
        .expect("alpha module collected");
    assert_eq!(alpha.interfaces, &["test:pkg/alpha"][..]);
    assert_eq!(alpha.permissions, &[] as &[&str]);
    assert_eq!(alpha.abi_min, 1);
}

/// 白名单恰好等于收集集 ⇒ 通过（正例）
#[test]
fn whitelist_matching_collected_set_passes() {
    let registry = ModuleRegistry::collected();
    assert!(registry.verify_whitelist(&["alpha", "mid", "zeta"]).is_ok());
}

/// 白名单**顺序无关**（宿主声明顺序不应影响判定）
#[test]
fn whitelist_order_does_not_matter() {
    let registry = ModuleRegistry::collected();
    assert!(registry.verify_whitelist(&["zeta", "alpha", "mid"]).is_ok());
}

/// 反例 A：有模块自报但不在白名单 ⇒ 红，并点名是哪些（防「新能力悄悄进产品」）
#[test]
fn whitelist_rejects_unlisted_module_and_names_it() {
    let registry = ModuleRegistry::collected();
    let err = registry
        .verify_whitelist(&["alpha", "mid"])
        .expect_err("unlisted module must be rejected");

    match err {
        HostKitError::WhitelistMismatch { unlisted, missing } => {
            assert_eq!(
                unlisted,
                vec!["zeta".to_string()],
                "must name the unlisted module"
            );
            assert!(missing.is_empty(), "no module is missing here");
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

/// 反例 B：白名单有模块但未被收集 ⇒ 红，并点名缺哪些
///
/// 这条是「依赖被删 / crate 没链上」的唯一早期信号——没有它，能力域会静默从插件
/// import 集消失，而 guest 编译期照常 import、运行期才炸。
#[test]
fn whitelist_rejects_missing_module_and_names_it() {
    let registry = ModuleRegistry::collected();
    let err = registry
        .verify_whitelist(&["alpha", "mid", "zeta", "sqlite"])
        .expect_err("missing module must be rejected");

    match err {
        HostKitError::WhitelistMismatch { unlisted, missing } => {
            assert_eq!(
                missing,
                vec!["sqlite".to_string()],
                "must name the missing module"
            );
            assert!(unlisted.is_empty(), "no extra module here");
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

/// 反例 C：双向差异同时出现时，两侧都要被点名（不得只报一侧）
#[test]
fn whitelist_reports_both_directions_at_once() {
    let registry = ModuleRegistry::collected();
    let err = registry
        .verify_whitelist(&["alpha", "ghost"])
        .expect_err("both directions differ");

    match err {
        HostKitError::WhitelistMismatch { unlisted, missing } => {
            assert_eq!(unlisted, vec!["mid".to_string(), "zeta".to_string()]);
            assert_eq!(missing, vec!["ghost".to_string()]);
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

/// 装配：全部模块装进 linker 即成功
#[test]
fn install_all_registers_every_module() {
    let engine = test_engine();
    let mut linker = Linker::<WasmPluginState>::new(&engine);
    ModuleRegistry::collected()
        .install_all(&mut linker)
        .expect("all stub modules install");
}

/// 反例 D：同一 linker 上重复装配必须失败并**点名模块**（不得静默覆盖接线）
///
/// 同一 interface 被装两次 = 两个能力域抢同一个 guest import 名字段，
/// 实例化期的行为会难以归因，故在装配期就炸。
///
/// 关键：第二次必须装进**同一个 linker**（宿主实例化期共用一个 linker）——
/// 装进新 linker 不构成重复，无从测起。
#[test]
fn install_all_rejects_duplicate_on_same_linker_and_names_module() {
    let engine = test_engine();
    let registry = ModuleRegistry::collected();
    let mut linker = Linker::<WasmPluginState>::new(&engine);

    registry.install_all(&mut linker).expect("first install");
    let err = registry
        .install_all(&mut linker)
        .expect_err("duplicate registration on the same linker must fail");

    match err {
        HostKitError::Register { module, source } => {
            assert!(
                matches!(module, "alpha" | "mid" | "zeta"),
                "must name the offending module, got {module:?}"
            );
            assert!(
                source.to_string().contains("defined twice"),
                "unexpected wasmtime error: {source}"
            );
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

/// 边界：每个新 linker 独立装配都成功（装进新 linker 不算重复）
///
/// 与 [`install_all_rejects_duplicate_on_same_linker_and_names_module`] 成对，
/// 钉住「重复」的准确含义：同一 linker 内的二次装配，而非「进程内装过」。
#[test]
fn install_all_into_fresh_linkers_succeeds_each_time() {
    let engine = test_engine();
    let registry = ModuleRegistry::collected();
    for round in 0..2 {
        let mut linker = Linker::<WasmPluginState>::new(&engine);
        registry
            .install_all(&mut linker)
            .unwrap_or_else(|e| panic!("round {round} must install into a fresh linker: {e}"));
    }
}

/// 错误串含期望/实际类型名（向下转型失败时必须能定位，fail-visible 的前提）
#[test]
fn host_port_unavailable_error_names_both_types() {
    let err = HostKitError::HostPortUnavailable {
        expected: "wasm_core::host_api::context::WasmHostContext",
        actual: "bedcode_host_kit::tests::OtherCtx",
    };
    let text = err.to_string();
    assert!(
        text.contains("WasmHostContext"),
        "missing expected type: {text}"
    );
    assert!(text.contains("OtherCtx"), "missing actual type: {text}");
}

inventory::submit! {
    bedcode_host_kit::ModuleEntry { module: &ALPHA }
}
inventory::submit! {
    bedcode_host_kit::ModuleEntry { module: &MID }
}
inventory::submit! {
    bedcode_host_kit::ModuleEntry { module: &ZETA }
}
