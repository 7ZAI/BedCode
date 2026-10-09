#!/usr/bin/env python3
"""票 02 批次 02：把 wasm-core 内两个 pty 集成测试二进制迁到宿主 src-tauri/tests/。

判据（每一步都有 count 断言，防静默漏替换）：
- 源文件从 git HEAD 取（工作区已删除；HEAD 版与迁出前的工作区逐字一致——
  该文件不在其他会话的在改清单里）
- 只做三类变换：文件头/导入块、`crate::host_api::pty::HostPtyPorts` 路径、
  pty_e2e 里那条 stale 的 activation.rs 接线锁（改钉 DomainHooks 链，见 pty_wiring.rs）
- 其余行**逐字保留**（含所有断言、注释、格式）

用法：python3 migrate_pty_tests.py --repo <repo-root>
"""

import argparse
import subprocess
import pathlib
import sys

PTY_E2E_SRC = "packages/bedcode-wasm-core/src/manager/runtime/tests/pty_e2e.rs"
PERF_SRC = "packages/bedcode-wasm-core/src/manager/runtime/tests/terminal_output_perf.rs"
PTY_E2E_DST = "bedcode-desktop/src-tauri/tests/pty_e2e.rs"
PERF_DST = "bedcode-desktop/src-tauri/tests/terminal_output_perf.rs"

PTY_E2E_HEADER_OLD = """//! host-pty 创建→拉取 / IO / 事件 / 背压 / 隔离矩阵（ABI v16）
//!
//! 自 `wasm_runtime.rs` 的 `mod tests` 拆出（共享脚手架在 `mod tests`，
//! 经 `use super::*` 可见）；fixture 互斥与产物构建语义不变。

use super::*;
use bedcode_plugin_api::host::{pty_event_topic, PTY_EXIT};
"""

PTY_E2E_HEADER_NEW = """//! host-pty 创建→拉取 / IO / 事件 / 背压 / 隔离矩阵（ABI v16）
//!
//! 自 wasm-core `manager/runtime/tests/pty_e2e.rs` 迁入（wasm-core 纯净性收口票 02
//! 批次 02）：这些用例跨 crate（wasm-core 运行时 + pty-engine 域 + 宿主端口 adapter），
//! 跨 crate 集成测试一律住宿主 `src-tauri/tests/`——wasm-core 已不依赖
//! `bedcode-pty-engine`，也不再持有 pty 端口 adapter（真源见 `src/plugin/pty.rs`）。
//!
//! 共享脚手架从 wasm-core 的 `mod tests` 改为 `mod support`（pty 一族断言助手 /
//! fixture 只读读取 / e2e 串行锁 / 超时兜底）；用例本体逐字保留。

mod support;

use std::collections::HashMap;
use std::sync::Arc;

use bedcode_plugin_api::host::{pty_event_topic, PTY_EXIT};
use tokio::sync::{Mutex, RwLock};

use support::*;
"""

PTY_E2E_STALE_LOCK_OLD = """        // 接线锁：停用路径必须调用插件 PTY 回收（本夹具没有 PluginHost，行为侧由
        // 下面的 purge 断言兜住，调用点存在性在此锁死——AGENTS §7 停用回收契约）。
        // 注意：deactivate_plugin_inner 经 P2 拆至 host/activation.rs，此处锁其源码。
        // 断言放宽到「pty 能力域回收调用点存在」（M-13 + pty-capability-domain D1：
        // 域机制随 host-pty 能力域迁到 `bedcode_pty_engine::plugin_binding::registry`）：
        // 旧式整行精确匹配会被参数签名/rustfmt 变动弄断且不在运行路径执行——参数
        // 形态交给下方行为断言与编译期类型检查
        let host_src = include_str!("../../host/activation.rs");
        assert!(
            host_src.contains("plugin_binding::purge_for_plugin"),
            "deactivate_plugin_inner 未接线 host-pty 停用回收"
        );
"""

PTY_E2E_STALE_LOCK_NEW = """        // 接线锁（停用路径必须触发本域回收）改钉 DomainHooks 装配链，见
        // `tests/pty_wiring.rs::deactivate_path_triggers_domain_purge_hook`——行为侧
        // 由下方 purge 断言兜住（本夹具没有 PluginHost，不跑 deactivate 全链）。
"""

PERF_IMPORTS_OLD = """use super::*;
use bedcode_pty_engine::PtyRing;
use std::time::Instant;
"""

PERF_IMPORTS_NEW = """// 迁宿主（wasm-core 纯净性收口票 02 批次 02：跨 crate 集成测试一律住宿主
// `src-tauri/tests/`）。原版经 `use super::*` 继承 runtime.rs 的测试脚手架，这里改用
// `mod support`（pty 一族 + fixture 只读读取 + e2e 串行锁 + 超时兜底）。
mod support;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{Mutex, RwLock};

use support::*;

use bedcode_pty_engine::PtyRing;
"""

HOST_PTY_PORTS_OLD = "crate::host_api::pty::HostPtyPorts"
HOST_PTY_PORTS_NEW = "HostPtyPorts"


def read_head(repo: pathlib.Path, rel: str) -> str:
    out = subprocess.run(
        ["git", "--no-pager", "show", f"HEAD:{rel}"],
        cwd=repo,
        capture_output=True,
    )
    if out.returncode != 0:
        sys.exit(f"git show HEAD:{rel} 失败：{out.stderr.decode(errors='replace')}")
    return out.stdout.decode()


def replace_once(text: str, old: str, new: str, label: str, expect: int = 1) -> str:
    count = text.count(old)
    assert count == expect, f"{label}: 期望 {expect} 处，实际 {count} 处"
    return text.replace(old, new)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", required=True)
    args = ap.parse_args()
    repo = pathlib.Path(args.repo).resolve()

    # ---- pty_e2e.rs ----
    e2e = read_head(repo, PTY_E2E_SRC)
    e2e = replace_once(e2e, PTY_E2E_HEADER_OLD, PTY_E2E_HEADER_NEW, "pty_e2e 头部/导入")
    e2e = replace_once(e2e, PTY_E2E_STALE_LOCK_OLD, PTY_E2E_STALE_LOCK_NEW, "pty_e2e stale 接线锁")
    e2e = replace_once(e2e, HOST_PTY_PORTS_OLD, HOST_PTY_PORTS_NEW, "pty_e2e adapter 路径", expect=3)
    assert "use super::*" not in e2e, "pty_e2e 不得残留 `use super::*`"
    assert "crate::host_api" not in e2e, "pty_e2e 不得残留 crate::host_api 路径"
    (repo / PTY_E2E_DST).write_text(e2e)
    print(f"wrote {PTY_E2E_DST}: {len(e2e.splitlines())} lines")

    # ---- terminal_output_perf.rs ----
    perf = read_head(repo, PERF_SRC)
    perf = replace_once(perf, PERF_IMPORTS_OLD, PERF_IMPORTS_NEW, "perf 导入块")
    perf = replace_once(perf, HOST_PTY_PORTS_OLD, HOST_PTY_PORTS_NEW, "perf adapter 路径", expect=1)
    # 只判「代码行」形态：本次新增的说明注释里也会出现 `use super::*` 字面量
    assert "\nuse super::*;" not in perf, "perf 不得残留 `use super::*;` 代码行"
    assert "crate::host_api" not in perf, "perf 不得残留 crate::host_api 路径"
    (repo / PERF_DST).write_text(perf)
    print(f"wrote {PERF_DST}: {len(perf.splitlines())} lines")


if __name__ == "__main__":
    main()
