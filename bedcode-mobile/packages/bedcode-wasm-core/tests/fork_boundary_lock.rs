//! 移动端 wasm-core fork 边界锁（票 17 · ADR 0040 D2）
//!
//! fork crate 的三不原则：
//! 1. **不绑桌面契约**：`bedcode_plugin_api::`（桌面 SDK 包名）不得出现——
//!    bindgen 与类型引用一律走 `bedcode_plugin_api_mobile`（ADR 0018 契约独立；
//!    needle 带双冒号后缀，`bedcode_plugin_api_mobile::` 不含该子串不误伤）
//! 2. **不依赖桌面能力域/基础层**：桌面 7 个 crate（server-base/core/http/
//!    websocket/peer-net/pty-engine/crypto-engine）不得出现
//!    （双端共享锚点白名单 = `bedcode-host-kit` / `bedcode-peer-net` /
//!    `bedcode-link-crypto` / `bedcode-discovery-engine` / `bedcode-ws-client-engine`
//!    ——末两者是双端共享的**纯引擎能力 crate**（ADR 0042 mDNS 引擎、ADR 0043
//!    WS 出站连接引擎，均零 WIT 默认形态），随抽根自禁入名单移入白名单）
//! 3. **不复活桌面独有域文件**：host_api 桌面 21 域 / system 桌面引擎面 /
//!    utils 宿主胶水 / manager 装配层 / host-task / L1 能力路由 / intercall
//!    （移动 WIT 无 host-api-call）——文件级缺席断言（lib.rs 内嵌锁的独立复核）
//!
//! 反向断言：机制核文件必须在场（fork 面 = 桌面机制的移动真源；缺失 = 有人
//! 以「裁剪」为名把机制一并删掉）。
//!
//! 只扫非注释行：本锁自身的说明与源文件「为什么删」的记账注释不算回接。

use std::path::{Path, PathBuf};

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

#[test]
fn fork_does_not_bind_desktop_sdk_or_capability_crates() {
    // needle 带边界：`bedcode_plugin_api::`（:: 结尾）不会命中
    // `bedcode_plugin_api_mobile::`；能力域 crate 名裸词在 use/调用中必然出现
    let retired_needles = [
        "bedcode_plugin_api::",
        "bedcode_server_base",
        "bedcode_server_core",
        "bedcode_server_http",
        "bedcode_server_websocket",
        "bedcode_server_peer_net",
        "bedcode_pty_engine",
        "bedcode_crypto_engine",
    ];
    let mut violations: Vec<String> = Vec::new();
    visit_rs(&src_dir(), &mut |rel, content| {
        for (idx, raw) in content.lines().enumerate() {
            let line = raw.trim_start();
            if line.starts_with("//") || line.starts_with("/*") || line.starts_with('*') {
                continue;
            }
            for needle in retired_needles {
                if line.contains(needle) {
                    violations.push(format!("{rel}:{}: {line}", idx + 1));
                }
            }
        }
    });
    assert!(
        violations.is_empty(),
        "fork crate 出现桌面 SDK / 桌面能力域引用（ADR 0018 契约独立 + ADR 0040 D2）：\n{}",
        violations.join("\n")
    );
}

#[test]
fn desktop_only_domain_files_stay_absent() {
    let retired_files = [
        "src/system/config.rs",
        "src/system/opener.rs",
        "src/system/process.rs",
        "src/system/wsl.rs",
        "src/utils.rs",
        "src/utils/auth.rs",
        "src/manager/host.rs",
        "src/manager/task.rs",
        "src/manager/capability.rs",
        // "src/manager/runtime/component.rs" 与 "src/test_support.rs" 已随批次 2
        // 移出本清单（票 17 §6）：移动 WIT 绑定层（component.rs，绑定移动
        // bedcode.wit v17——与桌面同名文件绑定桌面 WIT 22 import 是两份独立
        // 形状）与移动测试支持面（test_support.rs，含移动夹具构建器）是
        // **合法新增**，非桌面回接；锁只盯「桌面形状的回接」，此处登记同名
        // 会误伤。防桌面回接的真正判据 = SDK 包名锁（用例 1）+ WIT 路径
        // （bindgen 指向 plugin-sdk-mobile）+ 宿主装配在宿主侧（host/ 不在）。
        "src/crypto.rs",
        "src/intercall.rs",
        "src/host_harness.rs",
        "src/enums.rs",
        "src/host_api/pty.rs",
        "src/host_api/task.rs",
        "src/host_api/timer.rs",
        "src/host_api/process.rs",
        "src/host_api/app.rs",
        "src/host_api/crypto.rs",
        "src/host_api/auth_center.rs",
        "src/host_api/api.rs",
        "src/host_api/unit_executor.rs",
        "src/host_api/wsl_fs.rs",
        "src/host_api/sqlite_scaffold.rs",
        "src/host_api/status.rs",
    ];
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut present: Vec<String> = Vec::new();
    for rel in retired_files {
        if root.join(rel).exists() {
            present.push(rel.to_string());
        }
    }
    assert!(
        present.is_empty(),
        "桌面独有域文件回到移动 wasm-core（票 17 §3.2 删面）：{present:?}"
    );
}

#[test]
fn mechanism_core_files_stay_present() {
    // 反向断言：机制核是本 fork 的存在意义（票 18 将从这里抽共享核），
    // 缺失 = 「裁剪桌面域」被扩大化为「删机制」
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let required = [
        "src/bus.rs",
        "src/config.rs",
        "src/monitor.rs",
        "src/permission.rs",
        "src/runtime_util.rs",
        "src/storage.rs",
        "src/host_context_registry.rs",
        "src/db.rs",
        "src/db/schema.sql",
        "src/manager.rs",
        "src/manager/loader.rs",
        "src/manager/registry.rs",
        "src/manager/validation.rs",
        "src/manager/downloader.rs",
        "src/manager/types.rs",
        "src/security.rs",
        "src/security/fs_auth.rs",
        "src/security/approval.rs",
        "src/host_api.rs",
        "src/host_api/context.rs",
        "src/error.rs",
        "src/system.rs",
    ];
    for rel in required {
        assert!(
            root.join(rel).exists(),
            "机制核文件缺失（fork 面被过度裁剪）：{rel}"
        );
    }
}

/// 双端对称结构锁（票 19 Part B）：机制核模块路径在**桌面整核与本 fork** 双侧
/// 同时在场——一侧以「裁剪」为名删机制（或另一侧结构性重组丢失机制）即红。
///
/// 名单 = 机制核在场面去掉 `src/error.rs`（移动 AppError 自持形状，非双端对称面；
/// 桌面 AppError 真源在 `bedcode-server-base`）。
#[test]
fn mechanism_core_modules_symmetric_with_desktop_core() {
    let desktop_src =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../packages/bedcode-wasm-core/src");
    let symmetric = [
        "bus.rs",
        "config.rs",
        "monitor.rs",
        "permission.rs",
        "runtime_util.rs",
        "storage.rs",
        "host_context_registry.rs",
        "db.rs",
        "db/schema.sql",
        "manager.rs",
        "manager/loader.rs",
        "manager/registry.rs",
        "manager/validation.rs",
        "manager/downloader.rs",
        "manager/types.rs",
        "security.rs",
        "security/fs_auth.rs",
        "security/approval.rs",
        "host_api.rs",
        "host_api/context.rs",
        "system.rs",
    ];
    let mut missing: Vec<String> = Vec::new();
    for rel in symmetric {
        if !desktop_src.join(rel).exists() {
            missing.push(rel.to_string());
        }
        assert!(
            src_dir().join(rel).exists(),
            "fork crate 机制核文件缺失（对称锁锚点失效）：{rel}"
        );
    }
    assert!(
        missing.is_empty(),
        "桌面整核机制模块缺失（双端对称结构被破坏——机制修复必须双端生效，票 18/19）：{missing:?}"
    );
}

/// 递归访问 src 下全部 .rs 文件（相对 src/ 的路径 + 内容）
fn visit_rs(dir: &Path, f: &mut impl FnMut(&str, &str)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            visit_rs(&path, f);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            let rel = path
                .strip_prefix(&src_dir())
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if let Ok(content) = std::fs::read_to_string(&path) {
                f(&format!("src/{rel}"), &content);
            }
        }
    }
}
