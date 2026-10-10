#!/usr/bin/env python3
"""mdns 反向锁判据修复的等价复刻 + 合成变异（票 02 批次 03 收尾）。

复刻 `src/plugin/mdns.rs::mdns_target_method_family_matches_the_self_reported_exports`
修复后的判据：剥 `fn ` 前缀后按 `mdns_` 前缀计数，与域自报 `EXPORTS.len()` 相等。
"""
import re
import sys

CTX = "packages/bedcode-wasm-core/src/host_api/context.rs"
ROUTING = "packages/bedcode-discovery-engine/src/routing.rs"

ctx = open(CTX, encoding="utf-8").read()
start = ctx.find("pub trait CapabilityTarget")
assert start >= 0, "trait 必须存在"
block = ctx[start:]
end = block.find("\n}\n")
assert end > 0, "trait 必须以列零 } 收尾"

decls = [l.strip()[3:] for l in block[:end].splitlines() if l.strip().startswith("fn ")]
mdns = [d for d in decls if d.startswith("mdns_")]

routing = open(ROUTING, encoding="utf-8").read()
m = re.search(r"EXPORTS\s*:\s*&\[&str\]\s*=\s*&\[(.*?)\]", routing, re.S)
exports = re.findall(r'"([^"]+)"', m.group(1))

print(f"trait 内 fn 总数 = {len(decls)}")
print(f"mdns_* 方法 = {mdns}")
print(f"域自报 EXPORTS = {len(exports)} 条: {exports}")
ok = len(mdns) == len(exports)
print(f"PASS(criterion) = {ok}")

# 合成变异：往解析后的 trait 块注入第 6 条 mdns 方法 ⇒ 判据必须翻红
mutated = block[:end] + "\n    fn mdns_fake_probe(&self);\n"
mut_decls = [
    l.strip()[3:]
    for l in mutated.splitlines()
    if l.strip().startswith("fn ")
]
mut_mdns = [d for d in mut_decls if d.startswith("mdns_")]
mut_ok = len(mut_mdns) == len(exports)
print(f"变异后计数 = {len(mut_mdns)}，变异后 PASS = {mut_ok}（应为 False）")

sys.exit(0 if (ok and not mut_ok) else 1)
