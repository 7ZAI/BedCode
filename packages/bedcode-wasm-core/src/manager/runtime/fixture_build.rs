//! 宿主测试夹具的构建器（`#[cfg(test)]` 专用）
//!
//! 提为独立 `pub(crate)` 模块的原因：夹具合并不止 `runtime::tests` 在用——
//! `runtime/component.rs` 的 `mod tests` 与 `host/tests/wasm_flow_test.rs` 也各有一份
//! 「mtime 检查 + `cargo build`」。三处各写一遍会漂移（合集 crate 改一处、另两处漏改）。
//! `runtime::tests` 自身是私有模块，兄弟模块（`host::tests::*`）无法寻址，故放这里。

use std::path::{Path, PathBuf};

use super::fixture_target;
use super::WASIP3_NIGHTLY;
use crate::config::plugin_debug_mode;

/// 取一组目录下所有源文件的最新 mtime（目录不存在时返回 `UNIX_EPOCH`）
///
/// **按目录树递归扫，不枚举具体文件**。原各夹具 builder 都是手写文件清单
/// （`plugin-sdk-desktop/rust/src/wasm.rs`、`wasm_host.rs`、…），漏一个文件就会
/// “改了不重建”，且症状是**测试全绿但跑的是旧产物**（最难查的一类）。
fn newest_source_mtime(dirs: &[PathBuf]) -> std::time::SystemTime {
    let mut newest = std::time::SystemTime::UNIX_EPOCH;
    let mut stack: Vec<PathBuf> = dirs.to_vec();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            // Cargo.lock / *.wasm 等非源文件不参与（会因构建自我更新而误触发重建）
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if name == "Cargo.lock" || path.extension().is_some_and(|e| e == "wasm") {
                continue;
            }
            if let Ok(modified) = entry.metadata().and_then(|m| m.modified()) {
                if modified > newest {
                    newest = modified;
                }
            }
        }
    }
    newest
}

/// SDK 夹具产物是否需要重建（产物比任一依赖源文件旧则重建）
///
/// **必须包含 SDK 自身的源文件**：缓存命中时本函数短路返回，**根本不会调用 cargo**，
/// 所以「cargo 的依赖指纹会发现 SDK 变了」不成立——只盯夹具自身文件会让 SDK 改动
/// 被静默忽略（测试全绿但跑的是旧产物）。
pub(crate) fn fixture_needs_rebuild(module_path: &Path) -> bool {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // 2026-10-08 迁根：本 crate 在根 `packages/`，plugin-sdk-* 留在
    // `bedcode-desktop/packages/`（契约与夹具 crate），须回两级再进桌面 packages
    let packages_dir = manifest_dir.join("../../bedcode-desktop/packages");
    let watched = [
        // 合集 crate 自身
        packages_dir.join("plugin-sdk-fixtures/src"),
        packages_dir.join("plugin-sdk-fixtures/Cargo.toml"),
        // SDK 全部源文件（rust/src 递归 + wit + 过程宏 crate）
        packages_dir.join("plugin-sdk-desktop/rust/src"),
        packages_dir.join("plugin-sdk-desktop/rust/wit"),
        packages_dir.join("plugin-sdk-desktop/rust-macros/src"),
    ];
    let newest = newest_source_mtime(&watched);
    let module_modified = std::fs::metadata(module_path)
        .and_then(|m| m.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    newest > module_modified
}

/// SDK 绑定夹具合集（`packages/plugin-sdk-fixtures`）按 feature + profile 的产物路径
///
/// 合集 crate 各 feature 编译出的 wasm 同名（都是 `bedcode_plugin_sdk_fixtures.wasm`），
/// 后构建的会覆盖先构建的——故按 feature（+profile）加后缀另存一份，原地产物只当
/// 中间步骤。依赖图仍在共享 target 目录里复用（实测切 feature 仅重编夹具本体 ~3s，
/// 不重编 SDK）。
pub(crate) fn sdk_fixture_artifact(feature: &str, profile: &str) -> PathBuf {
    fixture_target::dir()
        .join("wasm32-wasip3")
        .join(profile)
        .join(format!("bedcode_plugin_sdk_fixtures.{feature}.{profile}.wasm"))
}

/// SDK 夹具构建互斥锁
///
/// 夹具产物必须互斥：cargo 对各 feature 编出的 wasm **同名**，而测试用例并行跑。
/// 若不串行，A 线程 build 完 feature=pty、还没来得及归档，B 线程的 build=ws 就把
/// 同名产物覆写了——A 归档到的是 B 的内容（实测表现为
/// `failed to parse WebAssembly module`，读到的是另一个线程半写完的文件）。
///
/// 只锁"build + 归档"这一段：命中缓存的快路径不取锁。
static SDK_FIXTURE_BUILD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// SDK 夹具构建互斥锁的守卫（中毒后取回内部值继续）
fn lock_sdk_fixture_build() -> std::sync::MutexGuard<'static, ()> {
    SDK_FIXTURE_BUILD_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// 构建 SDK 绑定夹具合集的单个夹具（feature 互斥，一次一个）
///
/// `feature` 取 `http` / `task` / `pty` / `sdk` / `ws` / `wasip3`。
///
/// `BEDCODE_PLUGIN_DEBUG=1`（dev 构建下）走 **debug** profile——保留 DWARF 行号，
/// 供 `test_debug_mode_trap_includes_line_info` 断言 trap 错误串含 `file:line`
/// （release 产物无行号）。产物名含 profile，两者可共存不互相覆盖。
pub(crate) fn build_sdk_fixture(feature: &str) -> Vec<u8> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // 2026-10-08 迁根：plugin-sdk-fixtures 留在 bedcode-desktop/packages/，回两级再进
    let packages_dir = manifest_dir.join("../../bedcode-desktop/packages");
    let plugin_dir = packages_dir.join("plugin-sdk-fixtures");

    let profile = if plugin_debug_mode() { "debug" } else { "release" };
    let module_path = sdk_fixture_artifact(feature, profile);

    // 命中缓存的快路径不取锁
    if module_path.exists() && !fixture_needs_rebuild(&module_path) {
        return std::fs::read(&module_path).expect("read cached sdk fixture component");
    }

    // build + 归档必须与其它 feature 串行：各 feature 产物同名，见锁的注释
    let _guard = lock_sdk_fixture_build();
    // 取锁后重新确认一次缓存：等锁期间可能已有别的线程编好了同一个 feature
    if module_path.exists() && !fixture_needs_rebuild(&module_path) {
        return std::fs::read(&module_path).expect("read cached sdk fixture component");
    }

    let mut args = vec!["build", "--target", "wasm32-wasip3"];
    if profile == "release" {
        args.push("--release");
    }
    let manifest = plugin_dir.join("Cargo.toml");
    args.extend([
        "--no-default-features",
        "--features",
        feature,
        "--manifest-path",
        manifest.to_str().unwrap(),
    ]);
    let status = std::process::Command::new("cargo")
        .env("RUSTUP_TOOLCHAIN", WASIP3_NIGHTLY)
        .env("CARGO_TARGET_DIR", fixture_target::dir())
        .args(args)
        .status()
        .expect("run cargo build for sdk fixture component");
    assert!(
        status.success(),
        "sdk fixture ({feature}/{profile}) component WASM build failed"
    );

    // cargo 产出的固定名拷贝一份到 feature+profile 专属名：下一轮构建别的 feature
    // 会覆写固定名，但不会动已归档的产物
    let built = fixture_target::artifact("wasm32-wasip3", profile, "bedcode_plugin_sdk_fixtures");
    std::fs::copy(&built, &module_path)
        .unwrap_or_else(|e| panic!("archive sdk fixture {feature}/{profile} artifact: {e}"));
    std::fs::read(&module_path).expect("read sdk fixture component after build")
}
