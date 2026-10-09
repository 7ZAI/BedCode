#!/usr/bin/env python3
"""票 02 批次 02 迁移补丁：宿主测试访问器修正。

`WasmHostContext.permission` 字段是 `pub(crate)`（wasm-core 内），宿主测试看不到；
对外通道是 pub trait `PermissionScope::permission()`。故迁入的用例里
`.permission.grant_permissions(…)` 一律改为 `.permission().grant_permissions(…)`
（含 rustfmt 折行的 `.permission\n  .grant_permissions` 形态），并补 trait 导入。

判据：替换后不得残留 `.permission.grant_permissions` / 折行形态；替换次数 > 0。
"""

import argparse
import pathlib
import re
import sys

FILES = [
    "bedcode-desktop/src-tauri/tests/pty_e2e.rs",
    "bedcode-desktop/src-tauri/tests/terminal_output_perf.rs",
]

CALL_RE = re.compile(r"\.permission\s*\n?\s*\.grant_permissions")

TRAIT_IMPORT = "use bedcode_desktop_lib::wasm_core::host_api::context::PermissionScope;\n"


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", required=True)
    args = ap.parse_args()
    repo = pathlib.Path(args.repo).resolve()

    for rel in FILES:
        path = repo / rel
        text = path.read_text()
        new, count = CALL_RE.subn(".permission().grant_permissions", text)
        assert count > 0, f"{rel}: 未找到 `.permission.grant_permissions` 调用"
        assert CALL_RE.search(new) is None, f"{rel}: 仍有未替换的调用"
        # 补 trait 导入（放在 use support::* 之前，与其他 use 同组）
        anchor = "use support::*;"
        assert anchor in new, f"{rel}: 缺 `use support::*;` 锚点"
        new = new.replace(anchor, TRAIT_IMPORT + anchor, 1)
        assert new.count(TRAIT_IMPORT) == 1, f"{rel}: trait 导入重复"
        path.write_text(new)
        print(f"patched {rel}: {count} 处 permission 访问器 + 1 处 trait 导入")


if __name__ == "__main__":
    main()
    sys.exit(0)
