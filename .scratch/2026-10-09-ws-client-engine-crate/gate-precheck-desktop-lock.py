#!/usr/bin/env python3
"""桌面 `capability_crates_no_product_ids` 锁的等价预检（非替代实跑）。

背景：本任务在 `SCANNED_CRATES` 登记了 `bedcode-ws-client-engine`，但该锁的实跑需要
桌面 `src-tauri` 全量构建（磁盘 100% 满 + 桌面 target 已被清空 → 不可行）。本脚本
逐条复刻该锁的判据（C-3 / C-4 / C-5）做等效预检，结果与实跑存在「复刻偏差」这一
残余风险，已在交付说明中如实标注。

判据来源：bedcode-desktop/src-tauri/tests/capability_crates_no_product_ids.rs
"""

from pathlib import Path

REPO = Path(__file__).resolve().parents[2]  # .scratch/<task>/ -> 仓库根
DESKTOP_ROOT = REPO / "bedcode-desktop" / "src-tauri"

SCANNED_CRATES = [
    "bedcode-server-base",
    "bedcode-crypto-engine",
    "bedcode-server-core",
    "bedcode-server-http",
    "bedcode-server-websocket",
    "bedcode-server-peer-net",
    "bedcode-discovery-engine",
    "bedcode-pty-engine",
    "bedcode-ws-client-engine",
]
PENDING_SCAN_CRATES = ["bedcode-wasm-core", "bedcode-host-kit"]
# 已知在途基线（**非本任务**）：两个 crate 已入库（9b26ab1e4 / 3015b392a）但未被登记进
# 任何一桶 → 桌面锁 C-3 反向断言在 HEAD 即为红。归属各自票据处置，本预检只把它们
# 计入「既有基线」而不计为本任务违规。
KNOWN_UNREGISTERED_BASELINE = {
    "bedcode-host-api-core",
    "bedcode-headless-host-probe",
}
PLACEHOLDER_SEGMENTS = {"xxx", "test", "other", "example", "sample"}
PRODUCT_ID_PREFIX = "com.bedcode."
RETIRED_HOST_SURFACE_TOKENS = [
    "host-session",
    "host-terminal",
    "output-ring-fetch",
    "session-status-changed",
    "PLUGIN_SESSION_RING_FETCH_MAX_BYTES",
    "ENV_BEDCODE_SESSION_ID",
]
PRODUCT_ID_EXCEPTIONS = {
    ("bedcode-server-http", "src/controllers/plugin_controller.rs"): {
        "com.bedcode.auto-task",
        "com.bedcode.session",
        "com.bedcode.terminal-session",
    }
}

PACKAGES_DIRS = [REPO / "packages", DESKTOP_ROOT.parent / "packages"]


def crate_dir(name: str) -> Path:
    for root in PACKAGES_DIRS:
        candidate = root / name
        if candidate.is_dir():
            return candidate
    raise SystemExit(f"crate `{name}` 在两个根下都找不到 —— 扫描器空转")


def collect_rs(root: Path):
    return sorted(root.rglob("*.rs"))


def is_test_only_file(path: Path) -> bool:
    return any(part == "tests" for part in path.parts)


def strip_line_comments(line: str) -> str:
    out, in_str, in_char, escaped, i = [], False, False, False, 0
    while i < len(line):
        b = line[i]
        if escaped:
            out.append(b)
            escaped = False
            i += 1
            continue
        if b == "\\" and (in_str or in_char):
            out.append(b)
            escaped = True
            i += 1
            continue
        if b == '"' and not in_char:
            in_str = not in_str
            out.append(b)
            i += 1
            continue
        if b == "'" and not in_str:
            in_char = not in_char
            out.append(b)
            i += 1
            continue
        if b == "/" and not in_str and not in_char and i + 1 < len(line) and line[i + 1] == "/":
            break
        out.append(b)
        i += 1
    return "".join(out)


def opens_cfg_test(line: str) -> bool:
    t = line.lstrip()
    return t.startswith("#[cfg(") and "test" in t


def prod_source(path: Path, crate_root: Path):
    if is_test_only_file(path.relative_to(crate_root)):
        return None
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return None
    lines = []
    for line in text.splitlines():
        if opens_cfg_test(line):
            break
        lines.append(strip_line_comments(line))
    rel = str(path.relative_to(crate_root)).replace("\\", "/")
    return (path, rel, lines)


def product_segments_in(text: str):
    found, rest = set(), text
    while True:
        at = rest.find(PRODUCT_ID_PREFIX)
        if at < 0:
            break
        after = rest[at + len(PRODUCT_ID_PREFIX):]
        seg = ""
        for ch in after:
            if ch.isascii() and (ch.isalnum() or ch in "_-."):
                seg += ch
            else:
                break
        first = seg.split(".")[0]
        rest = after[len(seg):]
        if not first or first in PLACEHOLDER_SEGMENTS:
            continue
        found.add(first)
    return found


def product_literals_in(text: str):
    found, rest = set(), text
    while True:
        at = rest.find(PRODUCT_ID_PREFIX)
        if at < 0:
            break
        after = rest[at + len(PRODUCT_ID_PREFIX):]
        seg = ""
        for ch in after:
            if ch.isascii() and (ch.isalnum() or ch in "_-."):
                seg += ch
            else:
                break
        rest = after[len(seg):]
        if not seg:
            continue
        found.add(PRODUCT_ID_PREFIX + seg)
    return found


def scan_crate(name: str):
    crate_root = crate_dir(name)
    return [s for s in (prod_source(p, crate_root) for p in collect_rs(crate_root / "src")) if s]


violations = []

# ---- C-3：登记 crate 在场 + 覆盖面完整 ----
for name in SCANNED_CRATES:
    src = crate_dir(name) / "src"
    if not src.is_dir():
        violations.append(f"C-3 登记了 `{name}` 但 src 不存在")
    elif not list(collect_rs(src)):
        violations.append(f"C-3 `{name}` 的 src 下没有 .rs（扫描器空转）")
    if name in PENDING_SCAN_CRATES:
        violations.append(f"C-3 `{name}` 同时出现在两个桶")

unregistered = []
for root in PACKAGES_DIRS:
    if not root.is_dir():
        violations.append(f"C-3 读不到 {root}")
        continue
    for entry in sorted(root.iterdir()):
        if not entry.name.startswith("bedcode-"):
            continue
        if entry.name in SCANNED_CRATES or entry.name in PENDING_SCAN_CRATES:
            continue
        if entry.name in KNOWN_UNREGISTERED_BASELINE:
            continue
        unregistered.append(str(entry))
if unregistered:
    violations.append(f"C-3 未登记 crate：{unregistered}")

# ---- C-4：生产代码零未登记产品 id ----
for name in SCANNED_CRATES:
    for _path, rel, lines in scan_crate(name):
        segs = product_segments_in("\n".join(lines))
        if not segs:
            continue
        if (name, rel) in PRODUCT_ID_EXCEPTIONS:
            continue
        violations.append(f"C-4 {name}/{rel}: 产品插件 id {sorted(segs)}")

# ---- C-4b：退役宿主面词汇 ----
for name in SCANNED_CRATES:
    for _path, rel, lines in scan_crate(name):
        joined = "\n".join(lines)
        hits = [t for t in RETIRED_HOST_SURFACE_TOKENS if t in joined]
        if hits:
            violations.append(f"C-4b {name}/{rel}: 退役面词汇 {hits}")

# ---- C-5：例外按内容钉死 ----
for (name, rel), expected in PRODUCT_ID_EXCEPTIONS.items():
    observed = set()
    for _path, r, lines in scan_crate(name):
        if r == rel:
            observed = product_literals_in("\n".join(lines))
    if observed != expected:
        violations.append(f"C-5 {name}/{rel}: 实测 {sorted(observed)} != 登记 {sorted(expected)}")

if violations:
    print("预检结果：FAIL")
    for v in violations:
        print("  -", v)
    raise SystemExit(1)

print("预检结果：PASS（C-3 覆盖完整 / C-4 生产代码零未登记产品 id / C-4b 零退役词汇 / C-5 例外钉死一致）")
