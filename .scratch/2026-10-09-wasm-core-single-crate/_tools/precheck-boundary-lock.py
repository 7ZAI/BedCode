#!/usr/bin/env python3
"""票 02 批次 02 等价预检：复刻宿主 `src/server/crate_boundary_lock.rs` 断言①/②/③。

为什么用等价预检：宿主 `cargo test` 需要建成整个 host 测试二进制（tauri + wasmtime +
actix 全量链接，10G+），本机磁盘在 100% 附近不可行（与 2026-10-09 前几次会话同因）。
脚本**逐条复刻**锁的判据（段名精确匹配 / 登记表两侧 / 宿主清单全量声明），并额外做
本批次特有断言（wasm-core 已无 pty-engine 边与标识符残留）。

残余风险：脚本与 Rust 实现存在复刻偏差（本脚本已自查：判据为空时必须报错）。
"""

import argparse
import pathlib
import re
import sys

SPLIT_CRATES_SRC = "packages/bedcode-wasm-core/src/crate_boundary_lock.rs"
LOCK_SRC = "bedcode-desktop/src-tauri/src/server/crate_boundary_lock.rs"
HOST_MANIFEST = "bedcode-desktop/src-tauri/Cargo.toml"
HOST_CRATE = "bedcode-desktop"
DEP_SECTIONS = ("dependencies", "dev-dependencies", "build-dependencies")


def read(path: pathlib.Path) -> str:
    return path.read_text(encoding="utf-8")


def parse_split_crates(src: str):
    block = src.split("pub const SPLIT_CRATES", 1)[1]
    block = block.split("];", 1)[0]
    pairs = re.findall(r'\(\s*"([^"]+)"\s*,\s*"([^"]+)"\s*\)', block)
    assert pairs, "SPLIT_CRATES 解析为空（正则或源格式变了）"
    return pairs


def strip_comment_lines(text: str) -> str:
    """剥掉整行注释（Rust 数组内注释不是条目；锁本体是 Rust 数组，不受影响）"""
    return "\n".join(
        line for line in text.splitlines() if not line.strip().startswith("//")
    )


def parse_edges(src: str, const_name: str):
    """解析 `const NAME: &[(&str, &[&str])] = &[ ("x", &["y"]), ... ];`"""
    block = strip_comment_lines(src).split(f"const {const_name}", 1)[1]
    block = block.split("];", 1)[0]
    out = {}
    for name, inner in re.findall(r'\(\s*"([^"]+)"\s*,\s*&\[(.*?)\]\s*,?\s*\)', block, re.S):
        out[name] = re.findall(r'"([^"]+)"', inner)
    assert out, f"{const_name} 解析为空"
    return out


def parse_required(src: str):
    block = strip_comment_lines(src).split("const REQUIRED_DOWNWARD_EDGES", 1)[1]
    block = block.split("];", 1)[0]
    pairs = re.findall(r'\(\s*"([^"]+)"\s*,\s*"([^"]+)"\s*\)', block)
    assert pairs, "REQUIRED_DOWNWARD_EDGES 解析为空"
    return pairs


def dep_keys(manifest: str, sections):
    """逐行解析（复刻 Rust 版：段名**精确**匹配，target 段按锁的原语义不参与判定）"""
    out = []
    current = ""
    for raw in manifest.splitlines():
        trimmed = raw.strip()
        if not trimmed or trimmed.startswith("#"):
            continue
        if trimmed.startswith("[") and trimmed.endswith("]"):
            current = trimmed.strip("[]")
            continue
        if "=" not in trimmed:
            continue
        key = trimmed.split("=", 1)[0].strip()
        if key and current in sections:
            out.append(key)
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", required=True)
    args = ap.parse_args()
    repo = pathlib.Path(args.repo).resolve()
    packages = repo / "packages"

    split = parse_split_crates(read(repo / SPLIT_CRATES_SRC))
    lock = read(repo / LOCK_SRC)
    allowed = parse_edges(lock, "ALLOWED_DOWNWARD_EDGES")
    required = parse_required(lock)
    registry = [name for name, _ in split]

    failures = []

    # 断言①：无横向 / 反向 / 越级边（横向边在任何依赖段都算）
    for name, rel in split:
        manifest = read(packages / rel / "Cargo.toml")
        deps = dep_keys(manifest, DEP_SECTIONS)
        if HOST_CRATE in deps:
            failures.append(f"[①] {name} 反向依赖宿主 {HOST_CRATE}")
        allow = allowed.get(name)
        if allow is None:
            failures.append(f"[①] 登记表缺 {name} 的允许边条目（锁会 panic，这里如实报红）")
            continue
        for dep in deps:
            if dep == name or dep not in registry:
                continue
            if dep not in allow:
                failures.append(f"[①] {name} → {dep} 是未登记的横向/越级边（允许：{allow}）")

    # 断言②：必需向下边存在于生产段
    for crate_name, dep in required:
        manifest = read(packages / dict(split)[crate_name] / "Cargo.toml")
        prod = dep_keys(manifest, ("dependencies",))
        if dep not in prod:
            failures.append(f"[②] {crate_name} 生产依赖缺必需边 {dep}（当前：{prod}）")

    # 断言③：宿主清单声明全部拆分产物
    host_deps = dep_keys(read(repo / HOST_MANIFEST), DEP_SECTIONS)
    for name, _ in split:
        if name not in host_deps:
            failures.append(f"[③] 宿主清单缺 {name}")

    # 本批次特有断言：wasm-core 与 pty-engine 的边与标识符双双消失
    wasm_core_manifest = read(packages / "bedcode-wasm-core" / "Cargo.toml")
    if "bedcode-pty-engine" in dep_keys(wasm_core_manifest, DEP_SECTIONS):
        failures.append("[批次] wasm-core 清单仍有 bedcode-pty-engine 依赖")
    if "bedcode-pty-engine" in allowed.get("bedcode-wasm-core", []):
        failures.append("[批次] ALLOWED 表仍列 wasm-core → pty-engine 边")
    if ("bedcode-wasm-core", "bedcode-pty-engine") in [tuple(p) for p in required]:
        failures.append("[批次] REQUIRED 表仍列 wasm-core → pty-engine 边")
    ident_hits = []
    for path in (packages / "bedcode-wasm-core" / "src").rglob("*.rs"):
        text = path.read_text(encoding="utf-8", errors="replace")
        for i, line in enumerate(text.splitlines(), 1):
            if re.search(r"(?<![A-Za-z0-9_])bedcode_pty_engine(?![A-Za-z0-9_])", line):
                ident_hits.append(f"{path.relative_to(repo)}:{i}: {line.strip()}")
    if ident_hits:
        failures.append("[批次] wasm-core 源码仍出现 bedcode_pty_engine 标识符：\n  " + "\n  ".join(ident_hits))

    # 自查：判据不得空转（登记表 / 宿主清单必须真的非空）
    assert len(registry) >= 8 and len(allowed) >= 8 and len(required) >= 10, "预检自身空转：表规模异常"

    if failures:
        print("PRECHECK FAIL:")
        for item in failures:
            print(" -", item)
        return 1
    print(f"PRECHECK PASS：{len(registry)} 个拆分产物 · 断言①②③ 全绿 · wasm-core↔pty-engine 边与标识符均无残留")
    return 0


if __name__ == "__main__":
    sys.exit(main())
