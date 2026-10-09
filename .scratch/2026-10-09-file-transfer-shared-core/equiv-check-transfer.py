#!/usr/bin/env python3
"""等价性校验：核内 `transfer.rs` 与移动端 `transfer_store.rs` 逐函数体比对。

判据：核以移动端（票 06/07/08 修正版）为基线，**共享函数的函数体必须逐字等价**
（归一化后）；核内多出的 `prune_absent` / `reconcile_diff` 来自桌面端旧快照通路，
另行比对桌面源文件。

为什么需要它：转写（`pub(crate)` → `pub`、注释合并、跨文件搬运）里最容易发生的
错误是「顺手改了判据」——那会让行为在测试全绿的情况下漂移。本脚本把「没改」变成
可执行断言。归一化只做：去 `pub(crate)`/`pub` 前缀、压空白、去行注释。
"""

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
CORE = REPO / "packages/bedcode-file-transfer-core/src/transfer.rs"
MOBILE = REPO / "bedcode-mobile/wasm-apps/file-transfer/rust/src/transfer_store.rs"
DESKTOP = REPO / "bedcode-desktop/wasm-apps/file-transfer/rust/src/transfer_store.rs"

# 共享函数（核 ↔ 移动端必须逐字等价）
SHARED = [
    "from_dto",
    "is_terminal",
    "is_active",
    "entry_from_dto",
    "merge_snapshot",
    "terminal_status_of",
    "reduce_event",
    "insert_active_projections",
    "mark_active_interrupted",
    "evict_overflow",
    "clear_terminal",
    "active_send_entries",
    "active_receive_entries",
    "mark_cancelled",
    "mark_paused",
    "apply_retry",
    "retry_source",
    "send_slot_open",
    "push_pull_intent",
    "take_pull_intent",
    "message",
]

# 核内多出、只在桌面端旧快照通路存在的函数（对桌面源比对）
DESKTOP_ONLY = ["prune_absent", "reconcile_diff"]

# 桌面端已知差异（本轮**刻意保留**：桌面仍是旧实现，接线在 T3 处理）
EXPECTED_DESKTOP_DIVERGENCE = {
    "reduce_event": "桌面缺 pull-started 建行锚点（移动端为票 06/07 修正版；桌面接入后统一）",
    "mark_active_interrupted": "桌面同名函数为 mark_interrupted_on_load（语义相同，仅命名旧）",
    "from_dto": "核含移动端在票 07 加的 local_path 字段（加法，桌面引擎不发该字段则为 None）",
    "insert_active_projections": "同上：核内构造条目含 local_path: None（加法字段）",
}


def strip_line_comments(line: str) -> str:
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


def prod_text(path: Path) -> str:
    """生产代码文本（剥注释 + 截断测试区）"""
    lines = []
    for line in path.read_text(encoding="utf-8").splitlines():
        t = line.lstrip()
        if t.startswith("#[cfg(") and "test" in t:
            break
        lines.append(strip_line_comments(line))
    return "\n".join(lines)


def extract_fns(text: str) -> dict[str, str]:
    """按名字抽取函数体（花括号配对，字符串感知）"""
    found: dict[str, str] = {}
    for m in re.finditer(r"\bfn\s+([a-z_][a-z0-9_]*)\s*(?:<[^>]*>)?\s*\(", text):
        name = m.group(1)
        # 找函数体的起始 '{'
        i = text.find("{", m.end())
        if i < 0:
            continue
        depth, j = 0, i
        in_str = in_char = esc = False
        while j < len(text):
            c = text[j]
            if esc:
                esc = False
            elif c == "\\" and (in_str or in_char):
                esc = True
            elif c == '"' and not in_char:
                in_str = not in_str
            elif c == "'" and not in_str:
                in_char = not in_char
            elif not in_str and not in_char:
                if c == "{":
                    depth += 1
                elif c == "}":
                    depth -= 1
                    if depth == 0:
                        break
            j += 1
        body = text[i:j + 1]
        # 同名重载/多次出现：保留第一次（re-export 场景不存在于本仓库这两个文件）
        found.setdefault(name, body)
    return found


def drop_redundant_trailing_semicolons(body: str) -> str:
    """去掉紧邻 `}` 的冗余尾分号（`{ continue; }` ≡ `{ continue }`）。

    rustfmt 在不同 crate 上下文里会给 let-else 块补/去这个分号（同一份代码在两端
    格式化结果不同），它是**语义无关的语法糖**：`continue` / `return x` 是 `!` 类型
    表达式，作块尾表达式与作语句等价。

    必须**引号感知**：字符串字面量里的 `;}` 若被一并吞掉，就会把「真的改了字符串」
    这类差异静默掩盖（锁最危险的失效形态）。
    """
    out, i, in_str, in_char, esc = [], 0, False, False, False
    n = len(body)
    while i < n:
        c = body[i]
        if esc:
            out.append(c); esc = False; i += 1; continue
        if c == "\\" and (in_str or in_char):
            out.append(c); esc = True; i += 1; continue
        if c == '"' and not in_char:
            in_str = not in_str; out.append(c); i += 1; continue
        if c == "'" and not in_str:
            in_char = not in_char; out.append(c); i += 1; continue
        if c == ";" and not in_str and not in_char and i + 1 < n and body[i + 1] == "}":
            i += 1  # 吞掉分号，保留 `}`
            continue
        out.append(c); i += 1
    return "".join(out)


def _canonicalize(s: str) -> str:
    """去花括号分组 + 去「圆/方括号层级 0」的逗号（引号感知）。

    两件事都在做同一件事：抹平 rustfmt 对 match 分支 / 块表达式的规范化差异——
    `=> { f(x) }`（块，无逗号）与 `=> f(x),`（表达式 + 分支分隔逗号）是同一份逻辑的
    两种写法。花括号一去掉，剩下的逗号就分不清是「分支分隔」还是「参数分隔」了，
    故按**括号层级**判断：参数/元素逗号在 `()` `[]` 内（层级 > 0）→ 保留；
    分支/字段分隔逗号在花括号层级 → 丢弃。
    """
    out, i, in_str, in_char, esc, depth = [], 0, False, False, False, 0
    while i < len(s):
        c = s[i]
        if esc:
            out.append(c); esc = False; i += 1; continue
        if c == "\\" and (in_str or in_char):
            out.append(c); esc = True; i += 1; continue
        if c == '"' and not in_char:
            in_str = not in_str; out.append(c); i += 1; continue
        if c == "'" and not in_str:
            in_char = not in_char; out.append(c); i += 1; continue
        if not in_str and not in_char:
            if c in "{}":
                i += 1; continue
            if c in "([":
                depth += 1; out.append(c); i += 1; continue
            if c in ")]":
                depth -= 1; out.append(c); i += 1; continue
            if c == "," and depth == 0:
                i += 1; continue
        out.append(c); i += 1
    return "".join(out)


def normalize_strict(body: str) -> str:
    """严格归一化：去可见性前缀 + 压空白 + 去 `;}` 冗余分号（其余逐字）"""
    body = re.sub(r"\bpub(\(crate\))?\s+", "", body)
    body = re.sub(r"\s+", "", body)
    return drop_redundant_trailing_semicolons(body)


def normalize(body: str) -> str:
    """判据归一化 = 严格归一化 + 花括号/分支逗号规范化（见 [`_canonicalize`]）。

    为什么必须容忍这层：**rustfmt 会按其所属 crate 的上下文重排 match 分支与 let-else
    块**（`=> { f(x) }` ⇄ `=> f(x),`、`else { continue; }` ⇄ `else { continue }`），
    同一份逻辑搬运后必然呈现两种写法。把它判红会产出噪音锁，而噪音锁会被忽略——
    那比「判据略松」严重得多。

    代价：花括号分组与分支逗号本身的变化不再被本校验器捕获；但**标识符/字面量/运算符
    的序列**仍逐 token 比对，判据改写（加 `!`、改字段名、改阈值、改分支顺序）照旧会红。
    [`checker_self_test`] 用真实变异证明这两条。
    """
    return _canonicalize(normalize_strict(body))


def checker_self_test() -> list[str]:
    """校验器自查：既不能假绿（漏真差异），也不能假红（把格式当差异）"""
    problems: list[str] = []
    base = "fn f(x: bool) -> bool { if x { return true } else { false } }"
    # ① 真判据改写必须被抓到
    for label, lhs, rhs in [
        ("判据改写", base, base.replace("return true", "return false")),
        ("条件取反", base, base.replace("if x", "if !x")),
        ("字段改名", base, base.replace("x: bool", "y: bool")),
        ("语句顺序", "fn g() { a(); b() }", "fn g() { b(); a() }"),
    ]:
        if normalize(lhs) == normalize(rhs):
            problems.append(f"{label}未被识别（校验器假绿）：{label}")
    # ② 纯格式差异（rustfmt 的两种规范化产物）不得被当成差异
    for label, variant in [
        ("分号", "fn f(x: bool) -> bool { if x { return true; } else { false; } }"),
        ("match 块分支", "match x { A => { f(1) } B => { f(2) } }"),
    ]:
        peer = (
            "fn f(x: bool) -> bool { if x { return true } else { false } }"
            if label == "分号"
            else "match x { A => f(1), B => f(2) }"
        )
        if normalize(peer) != normalize(variant):
            problems.append(f"纯格式差异被误判（校验器假红，{label}）：{variant}")
    # ③ 引号感知：字符串内的 `;}` / 花括号不得被吞（否则「改了字符串」会被静默掩盖）
    if drop_redundant_trailing_semicolons('let s = "a;}";') != 'let s = "a;}";':
        problems.append("引号感知失效：字符串内的 `;}` 被吞掉")
    if _canonicalize('let s="{";') != 'let s="{";':
        problems.append("引号感知失效：字符串内的花括号被剥掉")
    # ④ 括号层级判据必须真的保留参数逗号（否则 `f(a,b)` 与 `f(ab)` 会混同）
    if normalize("fn f() { g(a, b) }") == normalize("fn f() { g(a b) }"):
        problems.append("括号内逗号被误删（校验器假绿）")
    return problems


def compare(core_body: str, src_body: str) -> tuple[bool, bool]:
    """返回 (等价, 严格逐字等价)"""
    return normalize(core_body) == normalize(src_body), normalize_strict(core_body) == normalize_strict(src_body)


def main() -> int:
    fail: list[str] = []

    print("=== 校验器自查（防假绿 / 防假红） ===")
    for p in checker_self_test():
        fail.append(f"校验器自查失败：{p}")
    print("  自查项：判据改写被抓 / 条件取反被抓 / 花括号分组不误判 / 引号感知")
    if fail:
        for f in fail:
            print("  FAIL:", f)
        return 1

    core_fns = extract_fns(prod_text(CORE))
    mobile_fns = extract_fns(prod_text(MOBILE))
    desktop_fns = extract_fns(prod_text(DESKTOP))

    strict_hits = 0

    print("\n=== 核 ↔ 移动端（基线，必须等价） ===")
    for name in SHARED:
        if name not in core_fns:
            fail.append(f"核内缺函数：{name}")
            continue
        if name not in mobile_fns:
            fail.append(f"移动端源缺函数（脚本判据过期？）：{name}")
            continue
        ok, strict = compare(core_fns[name], mobile_fns[name])
        strict_hits += int(strict)
        if not ok:
            fail.append(f"函数体不等价：{name}")
            print(f"  MISMATCH {name}")
        else:
            print(f"  OK       {name}{'' if strict else '   （等价；仅花括号分组差异）'}")

    print("\n=== 核内多出（桌面旧快照通路）↔ 桌面端 ===")
    for name in DESKTOP_ONLY:
        if name not in core_fns:
            fail.append(f"核内缺桌面专有函数：{name}")
            continue
        if name not in desktop_fns:
            fail.append(f"桌面源缺函数（脚本判据过期？）：{name}")
            continue
        ok, strict = compare(core_fns[name], desktop_fns[name])
        strict_hits += int(strict)
        if not ok:
            fail.append(f"桌面专有函数体不等价：{name}")
        else:
            print(f"  OK       {name}{'' if strict else '   （等价；仅花括号分组差异）'}")

    print("\n=== 桌面端差异清点（必须全部在允许清单内） ===")
    for name in SHARED:
        if name not in desktop_fns:
            continue
        ok, _ = compare(core_fns.get(name, ""), desktop_fns[name])
        if not ok:
            if name in EXPECTED_DESKTOP_DIVERGENCE:
                print(f"  允许差异 {name}: {EXPECTED_DESKTOP_DIVERGENCE[name]}")
            else:
                fail.append(f"桌面端出现未登记的差异（须评估是否行为变更）：{name}")

    total = len(SHARED) + len(DESKTOP_ONLY)
    print("\n=== 结论 ===")
    if fail:
        for f in fail:
            print("  FAIL:", f)
        return 1
    print(
        f"  PASS（{total} 个函数与各自基线等价，其中 {strict_hits} 个连空白与花括号分组"
        f"都逐字相同；桌面差异清点与登记一致）"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
