#!/usr/bin/env python3
"""把 /tmp/mobile_tests_draft.rs 拆成移动宿主三个集成测试文件（票 05 测试迁移）。"""
import re
from pathlib import Path

draft = Path("/tmp/mobile_tests_draft.rs").read_text().splitlines()
DST = Path("bedcode-mobile/src-tauri/tests")


def extract(name: str) -> str:
    """按函数名抽取 `#[test]` 段（含其前置 doc 注释）。"""
    idx = next(i for i, l in enumerate(draft) if l.startswith(f"fn {name}("))
    start = idx
    while start > 0 and (draft[start - 1].startswith("///") or draft[start - 1].startswith("#[")):
        start -= 1
    # 函数体：从签名行到列 0 的 `}` 为止
    end = idx
    while draft[end] != "}":
        end += 1
    return "\n".join(draft[start:end + 1])


def extract_block(first_line: str, last_line: str) -> str:
    a = next(i for i, l in enumerate(draft) if l.startswith(first_line))
    b = a
    while draft[b] != last_line:
        b += 1
    return "\n".join(draft[a:b + 1])


# 共享块：CapturingDispatcher（含 doc 注释）
cap_start = next(i for i, l in enumerate(draft) if l.startswith("/// 捕获型投递器"))
cap_end = next(i for i, l in enumerate(draft) if l == "}" and i > cap_start)
CAPTURING = "\n".join(draft[cap_start:cap_end + 1])

PRELUDE_COMMON = """use std::collections::HashSet;
use std::sync::Arc;

use bedcode_host_kit::WasmPluginState;
use bedcode_wasm_core::manager::runtime::{LoadedComponentPlugin, WasmRuntime};
use bedcode_wasm_core::test_support::{build_test_component, test_engine, TEST_PLUGIN_ID};
use wasmtime::component::Component;
"""

FILE_HEADERS = {
    "component_e2e.rs": """//! 移动组件闭环集成测试（票 05 绑定归位：自 wasm-core `bare/component.rs::tests` 迁入）
//!
//! **为什么迁宿主**：绑定归位后内核零 WIT 绑定（linker 里没有任何 core 域实现），
//! 夹具组件只能在**移动宿主**的进程里实例化——host 自报的 `mobile-host-core`
//! （core 12 域）+ 5 个移动独有域 adapter 就是它 import 的全部接口。
//!
//! 覆盖：实例化往返 + ABI 协商 + host-storage 直测 + 业务方法全链（生命周期 /
//! 命令 / manifest / bus 事件 / 生命周期事件映射）+ 高 ABI 拒绝 + 燃料看门狗 +
//! ResourceLimiter + SDK 宏产物加载 + AOT 缓存命中 + linker 重复注册。
//!
//! ws / 终端协议两个域名闭环各自独立成文件（`ws_client_e2e.rs` /
//! `terminal_session_e2e.rs`）。

""",
    "ws_client_e2e.rs": """//! 移动 host-websocket 客户端域全链路（票 11 §7.3；票 05 迁移动宿主集成测试）

""",
    "terminal_session_e2e.rs": """//! 移动 host-terminal-stream / host-connection / jwt-auth 全链路
//! （票 12 · ABI v15；票 05 迁移动宿主集成测试）

""",
}

# ---- component_e2e ----
parts = [
    FILE_HEADERS["component_e2e.rs"],
    PRELUDE_COMMON,
    "use bedcode_plugin_api_mobile::permission::PERMISSION_STORAGE;\n",
    "/// 测试用权限位（夹具 manifest 权限并集的对齐面）\nconst _PERMISSION_STORAGE_ALIAS: &str = PERMISSION_STORAGE;\n",
    extract("test_component_roundtrip_and_host_impl"),
    "",
    extract("test_component_business_methods_roundtrip"),
    "",
    extract("test_component_high_abi_rejected"),
    "",
    extract("test_component_fuel_trap"),
    "",
    extract("test_component_limiter_rejects_over_limit_memory"),
    "",
    extract("test_sdk_macro_component_loads_and_activates"),
    "",
    extract("test_component_aot_cache_hit"),
    "",
    """/// 重复注册同一 interface 必须报错（宿主装配面）
///
/// 防止 linker 被装配两次时静默覆盖接线（实例化时会以意外行为失败，不如注册期
/// 直接暴露）。用**宿主绑定**（`plugin::bindings`）验证 wasmtime 的
/// "defined twice" 语义——内核侧只剩 WASI 面，等价断言见 wasm-core
/// `bare/component.rs` 的 `test_component_linker_registers_wasi_by_default`。
#[test]
fn test_component_linker_rejects_duplicate_registration() {
    use bedcode_mobile_lib::plugin::bindings::bedcode;
    let engine = test_engine();
    let mut linker = wasmtime::component::Linker::<WasmPluginState>::new(&engine);
    type D = wasmtime::component::HasSelf<WasmPluginState>;
    bedcode::plugin::host_storage::add_to_linker::<WasmPluginState, D>(&mut linker, |s| s)
        .expect("first registration");
    let err = bedcode::plugin::host_storage::add_to_linker::<WasmPluginState, D>(&mut linker, |s| s)
        .expect_err("duplicate registration should fail");
    // wasmtime 对已注册的同名 interface instance 报 "defined twice"
    assert!(
        err.to_string().contains("defined twice"),
        "unexpected duplicate registration error: {}",
        err
    );
}
""",
]
(DST / "component_e2e.rs").write_text("\n".join(parts))

# ---- ws_client_e2e ----
parts = [
    FILE_HEADERS["ws_client_e2e.rs"],
    PRELUDE_COMMON,
    "use bedcode_plugin_api_mobile::permission::{PERMISSION_BUS, PERMISSION_WS_CLIENT};\n",
    CAPTURING,
    "",
    extract("ws_client_domain_full_loop_with_real_component"),
    "",
]
(DST / "ws_client_e2e.rs").write_text("\n".join(parts))

# ---- terminal_session_e2e ----
parts = [
    FILE_HEADERS["terminal_session_e2e.rs"],
    PRELUDE_COMMON,
    "use bedcode_plugin_api_mobile::permission::{PERMISSION_BUS, PERMISSION_TERMINAL_OUTPUT, PERMISSION_WS_CLIENT};\n",
    "use bedcode_wasm_core::test_support::{build_host_ctx_with, MockPorts};\n",
    CAPTURING,
    "",
    extract("terminal_session_domain_full_loop_with_real_component"),
    "",
]
(DST / "terminal_session_e2e.rs").write_text("\n".join(parts))

for f in ["component_e2e.rs", "ws_client_e2e.rs", "terminal_session_e2e.rs"]:
    print(f, len((DST / f).read_text().splitlines()), "lines")
