#!/usr/bin/env python3
"""一次性迁移脚本（票 05 会话 B）：把 wasm-core `bare/component.rs` 的 12 个
core 域 `Host` impl 块按其所属域拆分，插入移动宿主 `plugin/host_domains/<域>.rs`。

规则：
- 只搬 `impl bedcode::plugin::host_*::Host for WasmPluginState { ... }` 块（按大括号配平）；
- `super::host_impl::` → ``（同文件内的域函数直调）；`super::host_ctx` 保持（super = host_domains）；
- 插入点 = 目标文件最后一个顶层 `use ...;` 语句之后（import 段末尾）。
运行后脚本自身删除。"""
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
COMPONENT = REPO / "packages/bedcode-wasm-core/src/manager/runtime/bare/component.rs"
DST_DIR = REPO / "bedcode-mobile/src-tauri/src/plugin/host_domains"

IFACE_TO_FILE = {
    "host_log": "log.rs",
    "host_storage": "storage.rs",
    "host_plugin_database": "db.rs",
    "host_events": "event.rs",
    "host_http": "http.rs",
    "host_fs": "fs.rs",
    "host_config": "config.rs",
    "host_bus": "bus.rs",
    "host_peer": "peer.rs",
    "host_mdns": "mdns.rs",
    "host_platform": "platform.rs",
    "host_websocket": "ws.rs",
}

src = COMPONENT.read_text()
lines = src.splitlines(keepends=True)
blocks = {}
i = 0
while i < len(lines):
    m = re.match(r"impl bedcode::plugin::(host_[a-z_]+)::Host for WasmPluginState \{", lines[i])
    if m:
        iface = m.group(1)
        depth = 0
        start = i
        collected = []
        j = i
        while j < len(lines):
            collected.append(lines[j])
            depth += lines[j].count("{") - lines[j].count("}")
            if depth == 0:
                break
            j += 1
        if depth != 0:
            sys.exit(f"unbalanced braces at line {start+1}")
        blocks[iface] = "".join(collected)
        i = j + 1
    else:
        i += 1

missing = set(IFACE_TO_FILE) - set(blocks)
if missing:
    sys.exit(f"missing impl blocks: {sorted(missing)}")

for iface, fname in IFACE_TO_FILE.items():
    block = blocks[iface].replace("super::host_impl::", "")
    path = DST_DIR / fname
    if path.exists():
        text = path.read_text()
        # 最后一个顶层 use 语句的结束位置（`[^;]*` 可跨行，use 语句内无分号）
        uses = list(re.finditer(r"(?m)^use [^;]*;", text))
        if not uses:
            sys.exit(f"{fname}: no top-level use statement to anchor insertion")
        pos = uses[-1].end()
        new_text = text[:pos] + "\n\n" + block.rstrip("\n") + "\n" + text[pos:]
        path.write_text(new_text)
        print(f"{fname}: impl {iface} inserted ({block.count(chr(10))} lines)")
    elif fname == "log.rs":
        header = (
            "//! host_log — 插件日志（移动宿主 adapter）\n"
            "//!\n"
            "//! 4 级日志经共享核 `bedcode-host-api-core::log`（callsite 缓存 + per-plugin\n"
            "//! 级别阈值 + `[plugin:{}]` 前缀）；`mark_plugin_error` 经宿主注入的\n"
            "//! `status_reporter` 回调（置 Error + 前端通知）——与 wasm-core 时代逐字同语义。\n"
            "\n"
            "use bedcode_host_kit::WasmPluginState;\n"
        )
        path.write_text(header + "\n" + block.rstrip("\n") + "\n")
        print(f"log.rs: created with impl {iface}")
    else:
        sys.exit(f"{fname} missing and not the log special case")
