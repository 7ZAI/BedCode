#!/usr/bin/env python3
"""等价预检：复刻 bedcode-desktop/src-tauri/tests/capability_crates_no_product_ids.rs 的
C-3 / C-4 / C-5 / C-6 判据。

为什么需要它：桌面 host target 已被清空，全量重建需 10G+ 与数十分钟，故该集成测试无法
实跑。本脚本**从锁源文件解析**登记表（不硬编码副本），复刻判据给出 PASS/FAIL。

残余风险（必须如实写进交付说明）：脚本是对 Rust 实现的复刻，两侧可能存在偏差；真正的
门禁以宿主 `cargo test --test capability_crates_no_product_ids` 为准。
"""

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
LOCK = REPO / "bedcode-desktop/src-tauri/tests/capability_crates_no_product_ids.rs"
PACKAGES_DIRS = [REPO / "packages", REPO / "bedcode-desktop/packages"]

PRODUCT_ID_PREFIX = "com.bedcode."
PLACEHOLDERS = {"xxx", "test", "other", "example", "sample"}


def parse_rust_str_list(src: str, const_name: str) -> list[str]:
    m = re.search(rf"const {const_name}: &\[&str\] = &\[(.*?)\];", src, re.S)
    if not m:
        raise SystemExit(f"解析失败：找不到 {const_name}")
    return re.findall(r'"([^"]+)"', m.group(1))


def parse_rust_tuple_list(src: str, const_name: str) -> list[tuple[str, str]]:
    m = re.search(rf"const {const_name}: &\[\(&str, &str\)\] = &\[(.*?)\n\];", src, re.S)
    if not m:
        raise SystemExit(f"解析失败：找不到 {const_name}")
    return re.findall(r'\(\s*"([^"]+)",\s*"(.*?)",?\s*\)', m.group(1), re.S)


def strip_line_comments(line: str) -> str:
    """引号感知剥行注释（与锁内实现同口径）"""
    out, i, in_str, in_char, esc = [], 0, False, False, False
    while i < len(line):
        c = line[i]
        if esc:
            out.append(c); esc = False; i += 1; continue
        if c == "\\" and (in_str or in_char):
            out.append(c); esc = True
        elif c == '"' and not in_char:
            in_str = not in_str; out.append(c)
        elif c == "'" and not in_str:
            in_char = not in_char; out.append(c)
        elif c == "/" and not in_str and not in_char and i + 1 < len(line) and line[i + 1] == "/":
            break
        else:
            out.append(c)
        i += 1
    return "".join(out)


def prod_lines(path: Path) -> list[str] | None:
    """路径含 tests 段 → None；剥注释 + 截断首个 #[cfg(...test...)]"""
    if "tests" in path.parts:
        return None
    text = path.read_text(encoding="utf-8", errors="replace")
    lines = []
    for line in text.splitlines():
        t = line.lstrip()
        if t.startswith("#[cfg(") and "test" in t:
            break
        lines.append(strip_line_comments(line))
    return lines


def product_segments(text: str) -> set[str]:
    found, rest = set(), text
    while True:
        at = rest.find(PRODUCT_ID_PREFIX)
        if at < 0:
            return found
        after = rest[at + len(PRODUCT_ID_PREFIX):]
        seg = re.match(r"[A-Za-z0-9_.\-]*", after).group(0)
        rest = after[len(seg):]
        first = seg.split(".")[0]
        if first and first not in PLACEHOLDERS:
            found.add(first)


def parse_exceptions(src: str) -> list[tuple[str, str]]:
    """PRODUCT_ID_EXCEPTIONS: [(crate_name, file_rel)]（按文件放行的登记面）"""
    return re.findall(
        r'ProductIdException\s*\{\s*crate_name:\s*"([^"]+)",\s*file_rel:\s*"([^"]+)"', src, re.S
    )


def main() -> int:
    src = LOCK.read_text(encoding="utf-8")
    scanned = parse_rust_str_list(src, "SCANNED_CRATES")
    pending = [n for n, _ in parse_rust_tuple_list(src, "PENDING_SCAN_CRATES")]
    retired = parse_rust_str_list(src, "RETIRED_HOST_SURFACE_TOKENS")
    exceptions = parse_exceptions(src)
    known = set(scanned) | set(pending)

    fail: list[str] = []
    print(f"SCANNED({len(scanned)}): {scanned}")
    print(f"PENDING({len(pending)}): {pending}\n")

    # ── C-3 前半：登记 crate 必须真实存在且有 .rs
    def crate_dir(name: str) -> Path | None:
        for root in PACKAGES_DIRS:
            p = root / name
            if p.is_dir():
                return p
        return None

    for name in scanned:
        d = crate_dir(name)
        if d is None:
            fail.append(f"C-3 登记但不存在的 crate：{name}")
            continue
        if not list((d / "src").rglob("*.rs")):
            fail.append(f"C-3 {name} 的 src 下无 .rs（扫描器空转）")
        if name in pending:
            fail.append(f"C-3 {name} 同时在两个桶里")

    # ── C-3 后半：反向覆盖完整
    for root in PACKAGES_DIRS:
        if not root.is_dir():
            continue
        for entry in sorted(root.iterdir()):
            if entry.is_dir() and entry.name.startswith("bedcode-") and entry.name not in known:
                fail.append(f"C-3 未登记的 packages/bedcode-* 目录：{entry}")

    # ── C-4 / C-6
    for name in scanned:
        d = crate_dir(name)
        if d is None:
            continue
        for path in sorted((d / "src").rglob("*.rs")):
            lines = prod_lines(path)
            if lines is None:
                continue
            text = "\n".join(lines)
            segs = product_segments(text)
            rel = path.relative_to(d).as_posix()
            if segs and (name, rel) not in exceptions:
                fail.append(f"C-4 {name}/{rel}: 产品插件 id {sorted(segs)}")
            elif segs:
                print(f"  [例外放行·C-5 内容钉死由宿主锁负责] {name}/{rel}")
            for token in retired:
                if token in text:
                    fail.append(f"C-6 {name}/{path.relative_to(d)}: 退役面 `{token}`")

    print("=== 预检结果 ===")
    if fail:
        for f in fail:
            print("  FAIL:", f)
        return 1
    print("  PASS（C-3 覆盖完整 + C-4 零未登记产品 id + C-6 零退役面回接）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
