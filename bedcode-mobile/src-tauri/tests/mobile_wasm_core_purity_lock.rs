//! wasm-core 纯净性防回接锁（wasm-core-mobile-purity 票 06-①）
//!
//! 判据来源：spec §0 用户指令①「wasm-core 保持纯粹的 wasm-core 机制原则，不该引入额外
//! 能力」+ 指令⑥「移动端特殊的能力留在移动端宿主侧」+ ADR 0047 D4/D5。
//!
//! 四道判据（任一被破坏即红）：
//! 1. **移动独有域不得回流**：`bare/host_impl/` 不得再出现 notify / fs_mobile /
//!    terminal_stream / connection / auth 域文件；生产装配段（`#[cfg(test)]` 之前）
//!    不得出现这些 interface 的 `Host` impl；反向钉住移动宿主 adapter 在场。
//! 2. **形态壳不得含域逻辑**：http / fs 壳不得再出现执行本体（reqwest / fs 系统调用、
//!    载荷构造）；peer 时代的句柄表 / 重拨符号全仓形态面零命中。
//! 3. **共享实现核不得消失**（正向）：三条域的机制面必须在共享 crate 里可寻址——
//!    否则「壳变薄」会退化成「能力没了」（fail-visible）。
//! 4. **壳行数上限**：单文件与目录合计双重上限，防止有人把域逻辑改写成「壳里的
//!    实现」再慢慢长回来。
//!
//! 为什么独立成锁：票 03–05 把 3.3k 行域逻辑迁出 wasm-core，靠的是**人工对表**；
//! 回接（把逻辑写回壳、或在壳里新起一个域实现）不会让任何既有测试转红——本锁是那条
//! 缺口的门禁。
//!
//! 变异自检记录见 `.scratch/2026-10-10-wasm-core-mobile-purity/ticket-06-closure.md`。

use std::path::{Path, PathBuf};

/// 仓库根（本锁在 `bedcode-mobile/src-tauri/tests/`，上跳三级 = `mobile/src-tauri/../..`）
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root")
        .to_path_buf()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("读取 {} 失败：{e}", path.display()))
}

/// 形态壳目录（bare 形态的逐域 WIT 壳）
fn bare_shell_dir() -> PathBuf {
    repo_root().join("packages/bedcode-wasm-core/src/manager/runtime/bare/host_impl")
}

/// 移动宿主 adapter 目录
fn mobile_adapter_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/plugin")
}

// ==================== 判据 1：移动独有域不得回流 ====================

/// 移动独有域（ADR 0047 D5）：wasm-core 里不得再有这些域文件
const MOBILE_OWNED_SHELL_FILES: [&str; 5] = [
    "notify.rs",
    "fs_mobile.rs",
    "terminal_stream.rs",
    "connection.rs",
    "auth.rs",
];

/// 迁移后宿主 adapter 的落点（正向钉住：能力在宿主侧仍可达）
const MOBILE_ADAPTER_FILES: [&str; 5] = [
    "notify.rs",
    "fs_mobile.rs",
    "terminal_stream.rs",
    "connection.rs",
    "auth.rs",
];

#[test]
fn mobile_owned_domains_stay_in_mobile_host() {
    for file in MOBILE_OWNED_SHELL_FILES {
        let in_core = bare_shell_dir().join(file);
        assert!(
            !in_core.exists(),
            "移动独有域回流 wasm-core：{} 存在（应在移动宿主 src/plugin/{file}）",
            in_core.display()
        );
    }
    for file in MOBILE_ADAPTER_FILES {
        let in_host = mobile_adapter_dir().join(file);
        assert!(
            in_host.exists(),
            "移动宿主 adapter 缺失：{}（能力去 wasm-core 化不等于能力消失）",
            in_host.display()
        );
    }
}

/// 生产装配段（`#[cfg(test)]` 之前）不得出现移动独有 interface 的 Host impl
///
/// 为什么按 cfg(test) 切段：crate 自身的单测需要一份替身实现（`standin_msg` 形态），
/// 它**必须**留在 crate 内；要拦的是「生产路径重新长出实现」。
#[test]
fn production_linker_does_not_own_mobile_only_interfaces() {
    let path = repo_root().join("packages/bedcode-wasm-core/src/manager/runtime/bare/component.rs");
    let content = read(&path);
    let cut = content.find("#[cfg(test)]").unwrap_or(content.len());
    let production = &content[..cut];

    for needle in [
        "impl bedcode::plugin::host_notify::Host for",
        "impl bedcode::plugin::host_fs_mobile::Host for",
        "impl bedcode::plugin::host_terminal_stream::Host for",
        "impl bedcode::plugin::host_connection_target::Host for",
        "impl bedcode::plugin::host_auth_client::Host for",
    ] {
        assert!(
            !production.contains(needle),
            "移动独有域的生产 Host impl 回流 wasm-core（应住移动宿主 plugin/*.rs）：{needle}"
        );
    }
}

// ==================== 判据 2：形态壳不得含域逻辑 ====================

/// `(相对仓库根的路径, 禁现针脚, 说明)`
const SHELL_FORBIDDEN: [(&str, &str, &str); 6] = [
    (
        "packages/bedcode-wasm-core/src/manager/runtime/bare/host_impl/http.rs",
        "reqwest",
        "http 执行本体（客户端 / 请求发送）在能力域 crate，壳里不得重建",
    ),
    (
        "packages/bedcode-wasm-core/src/manager/runtime/bare/host_impl/http.rs",
        "execute_http_request",
        "非流式执行属能力域（server-http::plugin_binding::egress）",
    ),
    (
        "packages/bedcode-wasm-core/src/manager/runtime/bare/host_impl/http.rs",
        "execute_streaming_http",
        "SSE 流式执行属能力域",
    ),
    (
        "packages/bedcode-wasm-core/src/manager/runtime/bare/host_impl/fs.rs",
        "std::fs::",
        "文件系统调用属共享核 bedcode-host-api-core::fs",
    ),
    (
        "packages/bedcode-wasm-core/src/manager/runtime/bare/host_impl/fs.rs",
        "create_dir_all",
        "父目录守卫属共享核（壳只做端口接线）",
    ),
    (
        "packages/bedcode-wasm-core/src/manager/runtime/bare/host_impl/fs.rs",
        "remove_file",
        "删除（含幂等与平台分支）属共享核",
    ),
];

/// peer 时代的域逻辑符号（句柄表 / 自动重拨）：整个形态面零命中
const DOMAIN_LOGIC_MARKERS: [&str; 2] = ["HandleTable", "fn redial"];

#[test]
fn form_shells_do_not_regrow_domain_logic() {
    let root = repo_root();
    for (rel, needle, why) in SHELL_FORBIDDEN {
        let content = read(&root.join(rel));
        // 只扫非注释行：头注里说明「为什么不在这里做」属正常（本锁扫的是代码面）
        for (idx, line) in content.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            assert!(
                !line.contains(needle),
                "{rel}:{} 出现域逻辑针脚 `{needle}`（{why}）",
                idx + 1
            );
        }
    }

    for entry in std::fs::read_dir(bare_shell_dir()).expect("bare host_impl 目录") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let content = read(&path);
        for marker in DOMAIN_LOGIC_MARKERS {
            assert!(
                !content.contains(marker),
                "{} 含域逻辑标记 `{marker}`（票 03 已把 peer 域逻辑下沉能力域 crate）",
                path.display()
            );
        }
    }
}

// ==================== 判据 3：共享实现核不得消失 ====================

/// `(相对仓库根路径, 必须在场针脚)`
const SHARED_CORES: [(&str, &str); 4] = [
    (
        "packages/bedcode-server-http/src/plugin_binding/egress.rs",
        "pub fn http_fetch(",
    ),
    ("packages/bedcode-host-api-core/src/fs.rs", "pub fn fs_read("),
    ("packages/bedcode-server-peer-net/src/plugin_binding.rs", "pub fn peer_"),
    // 壳必须真的指向共享核（薄转发的最低证据：引用了那个 crate）
    (
        "packages/bedcode-wasm-core/src/manager/runtime/bare/host_impl/http.rs",
        "bedcode_server_http",
    ),
];

#[test]
fn shared_implementation_cores_stay_addressable() {
    let root = repo_root();
    for (rel, needle) in SHARED_CORES {
        let path = root.join(rel);
        assert!(path.exists(), "共享实现落点缺失：{rel}");
        assert!(
            read(&path).contains(needle),
            "{rel} 中找不到 `{needle}`（能力被搬到「壳之外」却无处可寻 = 静默失效）"
        );
    }
    let fs_shell = read(&root.join("packages/bedcode-wasm-core/src/manager/runtime/bare/host_impl/fs.rs"));
    assert!(
        fs_shell.contains("bedcode_host_api_core::fs"),
        "fs 壳必须转发到共享核 bedcode-host-api-core::fs"
    );
}

// ==================== 判据 4：壳行数上限 ====================

/// 单文件上限（当前最大 = ws.rs 333 行；留 ~20% 给注释，够改够说明，不够塞逻辑）
const SHELL_FILE_LINE_LIMIT: usize = 400;

/// 目录合计上限（当前 14 文件 / 2045 行；票 03 前约 3.3k）
const SHELL_DIR_LINE_LIMIT: usize = 2400;

#[test]
fn form_shells_stay_thin() {
    let dir = bare_shell_dir();
    let mut total = 0usize;
    let mut files = 0usize;
    let mut violations = Vec::new();

    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("bare host_impl 目录")
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("rs"))
        .collect();
    paths.push(dir.join("../host_impl.rs"));

    for path in paths {
        let lines = read(&path).lines().count();
        total += lines;
        files += 1;
        if lines > SHELL_FILE_LINE_LIMIT {
            violations.push(format!("{}: {lines} 行 > 上限 {SHELL_FILE_LINE_LIMIT}", path.display()));
        }
    }

    assert!(files >= 12, "扫描面异常：只读到 {files} 个壳文件（目录结构变了？）");
    assert!(violations.is_empty(), "壳文件超行数上限：\n{}", violations.join("\n"));
    assert!(
        total <= SHELL_DIR_LINE_LIMIT,
        "壳目录合计 {total} 行 > 上限 {SHELL_DIR_LINE_LIMIT}（域逻辑回流的典型征兆）"
    );
}

// ==================== 判据 5：退役的 http 引擎不得回接 ====================

#[test]
fn retired_mobile_http_engine_stays_retired() {
    let root = repo_root();
    let engine = root.join("packages/bedcode-wasm-core/src/host_api/http_engine.rs");
    assert!(
        !engine.exists(),
        "已退役的移动 http 引擎又出现了：{}（执行本体住能力域 crate）",
        engine.display()
    );
    let host_api = read(&root.join("packages/bedcode-wasm-core/src/host_api.rs"));
    assert!(
        !host_api.contains("pub mod http_engine"),
        "wasm-core 重新挂载了已退役的 http 引擎模块"
    );
}
