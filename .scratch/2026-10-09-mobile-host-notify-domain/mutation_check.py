#!/usr/bin/env python3
"""host-notify 域变异自检脚本（权限门旁路 / WIT 函数名漂移，mutate→测试→restore）。

用法：
  python3 mutation_check.py gate mutate|restore     # notify.rs 权限门旁路
  python3 mutation_check.py wit  mutate|restore     # WIT check-permission 改名漂移
"""
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]

GATE_FILE = REPO / "bedcode-mobile/packages/bedcode-wasm-core/src/manager/runtime/host_impl/notify.rs"
GATE_ORIG = """    require_notify(state)?;
    let (vibrate, sound) = parse_notify_options(options_json)?;"""
GATE_MUTATED = """    let (vibrate, sound) = parse_notify_options(options_json)?;"""

WIT_FILE = REPO / "bedcode-mobile/packages/plugin-sdk-mobile/rust/wit/bedcode.wit"
WIT_ORIG = "    check-permission: func() -> result<bool, string>;"
WIT_MUTATED = "    check-permissions: func() -> result<bool, string>;"


def apply(path: pathlib.Path, orig: str, mutated: str, mode: str) -> None:
    s = path.read_text()
    if mode == "mutate":
        assert orig in s, f"anchor missing (already mutated?): {path}"
        path.write_text(s.replace(orig, mutated, 1))
        print(f"mutated: {path.name}")
    elif mode == "restore":
        assert mutated in s, f"mutated anchor missing: {path}"
        path.write_text(s.replace(mutated, orig, 1))
        print(f"restored: {path.name}")
    else:
        raise SystemExit(f"unknown mode: {mode}")


target = sys.argv[1]
mode = sys.argv[2]
if target == "gate":
    apply(GATE_FILE, GATE_ORIG, GATE_MUTATED, mode)
elif target == "wit":
    apply(WIT_FILE, WIT_ORIG, WIT_MUTATED, mode)
else:
    raise SystemExit(f"unknown target: {target}")
