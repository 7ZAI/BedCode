//! host_api 共享实现核边界锁（票 18 §6 门禁）
//!
//! lib.rs 模块文档「边界判据」的机械执行面：
//!
//! 1. **不依赖双端 SDK**：`plugin_api`（needle 同时命中桌面 `bedcode_plugin_api` 与
//!    移动 `bedcode_plugin_api_mobile`）不得出现在实现层；
//! 2. **不回引双端 wasm-core**：`wasm_core`（双端 crate 名 `bedcode_wasm_core` /
//!    `bedcode_wasm_core_mobile` 同含该子串）不得出现；
//! 3. **不依赖宿主平台与运行时**：`tauri` / `tokio` 不得出现——阻塞与异步驱动是
//!    各端 adapter 的职责，实现层保持机制级最小依赖；
//! 4. **域文件在场**：实现层模块不得以「重构」为名被清空（缺失 = 共享核名存实亡）。
//!
//! ## 位置与自排除（2026-10-16）
//!
//! 本锁原先住在 crate 根 `tests/boundary_lock.rs`——那是**独立测试二进制**，只经 `pub`
//! API 访问对外行为，被宿主侧 `capability_crates_unit_tests_only.rs` 判为治理 crate
//! 不得有的 crate 根测试面（它一直在把桌面 `cargo test` 判红）。迁入 `src/` 后由
//! `lib.rs` 以 `#[cfg(test)] mod boundary_lock;` 引入（与同族 `file-transfer-core` /
//! `ws-client-engine` 的 `src/boundary_lock.rs` 同形态）。
//!
//! 迁入后**必须自排除本文件**（`SELF_LOCK_FILE`）：它自身含被禁 needle 的字面量常量表
//! （`retired_needles`）与报错文案，不自排除会扫到自己而恒红。
//!
//! 扫描只计非注释行：本锁自身的说明与源文件里的记账注释不算回接
//! （注释纪律见 AGENTS §6——注释解释为什么，锁不惩罚解释）。

use std::path::{Path, PathBuf};

/// 自排除的锁文件（含被禁 needle 的字面量，见头注「位置与自排除」）
const SELF_LOCK_FILE: &str = "boundary_lock.rs";

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

#[test]
fn impl_layer_has_no_sdk_wasm_core_host_platform_or_runtime_deps() {
    let retired_needles = ["plugin_api", "wasm_core", "tauri", "tokio"];
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
        "host_api 共享实现核出现越界依赖（票 18 §2 判据：实现层不得引用 SDK / wasm-core / 宿主平台 / 运行时）：\n{}",
        violations.join("\n")
    );
}

#[test]
fn domain_impl_files_stay_present() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let required = [
        "src/lib.rs",
        "src/gate.rs",
        "src/storage.rs",
        "src/bus.rs",
        "src/database.rs",
        "src/log.rs",
        "src/events.rs",
        "src/fs.rs",
    ];
    for rel in required {
        assert!(
            root.join(rel).exists(),
            "共享实现核域文件缺失（「重构」不得清空实现层）：{rel}"
        );
    }
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
            // 自排除：本锁文件自己就含被禁 needle 的字面量常量表（见头注）
            if path.file_name().and_then(|n| n.to_str()) == Some(SELF_LOCK_FILE) {
                continue;
            }
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
