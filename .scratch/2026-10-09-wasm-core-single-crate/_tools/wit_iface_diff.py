#!/usr/bin/env python3
"""统计双端 WIT 中共有 interface 的函数级差异（核心 world 边界的事实底座）。
用法：python3 wit_iface_diff.py <移动端 wit> <桌面端 wit>
"""
import re
import sys


def parse(path):
    src = open(path, encoding="utf-8").read()
    # 去掉注释，避免注释里的示例干扰
    src = re.sub(r"//.*", "", src)
    ifaces = {}
    # 匹配顶层 interface NAME { ... }（以行首 interface 起始，到行首 } 结束）
    for m in re.finditer(r"^interface\s+([\w-]+)\s*\{(.*?)^\}", src, re.S | re.M):
        name, body = m.group(1), m.group(2)
        funcs = set()
        for fm in re.finditer(r"^\s*([\w][\w-]*)\s*:\s*(?:async\s+)?(?:static\s+)?func\b", body, re.M):
            funcs.add(fm.group(1))
        for fm in re.finditer(r"^\s*(?:async\s+)?func\s*\(\s*([\w][\w-]*)", body, re.M):
            funcs.add(fm.group(1))
        ifaces[name] = funcs
    return ifaces


def main():
    mob, dsk = parse(sys.argv[1]), parse(sys.argv[2])
    common = sorted(set(mob) & set(dsk))
    print(f"移动端 interfaces: {len(mob)}  桌面端 interfaces: {len(dsk)}  共有: {len(common)}")
    print()
    print(f"{'interface':<26}{'移动':>6}{'桌面':>6}  差异")
    print("-" * 100)
    for name in common:
        m, d = mob[name], dsk[name]
        only_m = sorted(m - d)
        only_d = sorted(d - m)
        diff = []
        if only_m:
            diff.append("仅移动: " + ",".join(only_m))
        if only_d:
            diff.append("仅桌面: " + ",".join(only_d))
        print(f"{name:<26}{len(m):>6}{len(d):>6}  {'; '.join(diff) if diff else '一致'}")
    print()
    print("仅移动:", sorted(set(mob) - set(dsk)))
    print("仅桌面:", sorted(set(dsk) - set(mob)))


if __name__ == "__main__":
    main()
