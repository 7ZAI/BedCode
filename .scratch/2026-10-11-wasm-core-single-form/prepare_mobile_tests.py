#!/usr/bin/env python3
"""票 05 测试迁移（第二版）：HEAD `bare/component.rs::tests` → 移动宿主集成测试草稿。

机械改写：路径前缀 / 导出直调 → 公开壳方法 / 私有面 → test_support 转发面。
产物：/tmp/mobile_tests_draft.rs（单文件草稿 + 待人工点清单）。运行后可删。"""
import re
from pathlib import Path

src = Path("/tmp/bare_component_tests.rs").read_text()
lines = src.splitlines()
start = next(i for i, l in enumerate(lines) if l.strip().startswith("pub(crate) mod tests"))
body = []
for l in lines[start + 1:]:
    if l == "}":
        break
    body.append(l[4:] if l.startswith("    ") else l)
text = "\n".join(body)

# ---- 路径改写 ----
for a, b in [
    ("crate::test_support::", "bedcode_wasm_core::test_support::"),
    ("crate::bus::MessageDispatcher", "bedcode_wasm_core::bus::MessageDispatcher"),
    ("crate::bus::BusMessageHandler", "bedcode_wasm_core::bus::BusMessageHandler"),
    ("crate::bus::MessageBus", "bedcode_wasm_core::bus::MessageBus"),
    ("crate::manager::types::PluginLifecycleEvent",
     "bedcode_wasm_core::manager::types::PluginLifecycleEvent"),
    ("crate::security::fs_auth::FsAuthChecker", "bedcode_wasm_core::security::fs_auth::FsAuthChecker"),
    ("crate::storage::PluginStorage", "bedcode_wasm_core::storage::PluginStorage"),
    ("crate::db::", "bedcode_wasm_core::db::"),
    ("use super::super::{WasmHostContext, WasmRuntime};",
     "use bedcode_wasm_core::manager::runtime::{LoadedComponentPlugin, WasmHostContext, WasmRuntime};"),
    ("use super::*;",
     "use bedcode_wasm_core::manager::runtime::WasmPluginState;\nuse wasmtime::component::Component;"),
]:
    text = text.replace(a, b)

# ---- 导出直调 → 公开壳方法 / 转发面 ----
text = re.sub(r"[ \t]*let exports = plugin\.exports\(\)\.expect\(\"(?:world )?exports\"\);\n", "", text)
text = re.sub(
    r"exports\s*\n\s*\.bedcode_plugin_abi\(\)\s*\n\s*\.call_version\(&mut plugin\.store\)\s*\n\s*\.unwrap\(\),",
    "bedcode_wasm_core::test_support::plugin_abi_version(&mut plugin).unwrap(),",
    text,
)
text = re.sub(
    r"exports\s*\n\s*\.bedcode_plugin_lifecycle\(\)\s*\n\s*\.call_activate\(&mut plugin\.store\)\s*\n\s*\.expect\(\"call activate\"\)\s*\n\s*\.expect\(\"activate guest err\"\);",
    "plugin.activate().expect(\"activate\");",
    text,
)
text = re.sub(
    r"exports\s*\n\s*\.bedcode_plugin_command\(\)\s*\n\s*\.call_invoke\(&mut plugin\.store, ([^)\n]*)\)\s*\n\s*\.expect\(\"call invoke\"\)",
    r"plugin.invoke_command(\1).expect(\"call invoke\")",
    text,
)
text = re.sub(
    r"exports\s*\n\s*\.bedcode_plugin_manifest\(\)\s*\n\s*\.call_get\(&mut plugin\.store\)\s*\n\s*\.expect\(\"manifest\"\),",
    "plugin.get_manifest().expect(\"manifest\"),",
    text,
)
text = re.sub(
    r"let result =\n?\s*exports\s*\n\s*\.bedcode_plugin_command\(\)\s*\n\s*\.call_invoke\(&mut plugin\.store, ([^)\n]*)\);",
    r"let result = plugin.invoke_command(\1);",
    text,
)
text = re.sub(
    r"let budget = plugin\.store\.data\(\)\.limits\.fuel_budget\(\);\n\s*let remaining = plugin\.store\.get_fuel\(\)\.expect\(\"get fuel\"\);",
    "let (store, _) = bedcode_wasm_core::test_support::plugin_raw_store(&mut plugin);\n                let budget = store.data().limits.fuel_budget();\n                let remaining = store.get_fuel().expect(\"get fuel\");",
    text,
)
text = re.sub(
    r"let cmd_out = exports\.bedcode_plugin_command\(\)\n\s*\.call_invoke\(&mut plugin\.store, ([^)\n]*)\)",
    r"plugin.invoke_command(\1)",
    text,
)

Path("/tmp/mobile_tests_draft.rs").write_text(text)
print("draft lines:", len(text.splitlines()))
print("剩余可疑点：")
for i, l in enumerate(text.splitlines(), 1):
    if any(k in l for k in ("exports", "plugin.store", "build_component_linker", "call_invoke", "call_version", "call_get", "call_activate")):
        print(f"{i:5}: {l}")
