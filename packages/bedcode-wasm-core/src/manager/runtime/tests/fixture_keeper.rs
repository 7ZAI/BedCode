//! SDK 夹具产物 keeper（wasm-core 纯净性收口票 02 批次 04 / 05；票 07 补 pty / ws）
//!
//! `crypto`（批次 04）与 `task`（批次 05）夹具的**消费者都是宿主 e2e**
//! （`src-tauri/tests/host_crypto_e2e.rs` / `src-tauri/tests/task_e2e.rs`）——
//! host-crypto 与 host-task 的 WIT 实现迁宿主后，内核测试二进制不再注册该
//! interface，本 crate 已无用例实例化这些夹具。产物按既有约定由 wasm-core 测试
//! 构建（宿主测试只读不建，见 `test_support::sdk_fixture_artifact_bytes` 的缺失
//! 报错文案），故各留一个 keeper 用例确保产物在场且随源码重建——没有它，宿主
//! e2e 会在「产物缺失」上红，且该红指向一条没人负责的重建路径。
//!
//! **票 07 补齐 `pty` / `ws`**（票 05 记账的缺位）：两者的宿主消费者
//! （`src-tauri/tests/terminal_output_perf.rs` / `ws_e2e.rs`）同样只读不建；此前
//! pty 靠手动复刻构建命令补产物、ws 靠内核测试顺带构建——「宿主单独跑」时即红。
//! 补 keeper 后与 crypto/task 同口径：产物随源码重建，宿主只读有保障。
//!
//! **票 04 补充 `sdk` / `http`**：迁移前的构建方就在本 crate 的迁移用例里
//! （`component_e2e` / `sdk_e2e` / `http_e2e` 实例化夹具时顺带构建）；ticket-04
//! 把用例迁桌面宿主（`src-tauri/tests/{component_e2e,sdk_e2e,http_e2e,engine_e2e,
//! owner_e2e,wasm_flow_e2e}.rs`，只读消费）时，构建方随之消失——没有 keeper
//! 时「wasm-core 测试全绿 + 桌面 e2e 红（产物缺失）」会各红一半。补上后口径统一。

use super::build_sdk_fixture;

/// keeper 共用断言体（各夹具一个入口用例，skip 口径一致）
fn keep_fixture_alive(feature: &str) {
    // 工具链未装（CI stable / 本地未 install）时跳过——与 preopen_e2e 的 skip 口径一致：
    // 夹具构建走 pinned nightly + wasm32-wasip3，缺工具链不是本用例要抓的回归。
    if !super::wasip3_toolchain_ready() {
        eprintln!(
            "[skip] wasip3 工具链未安装，{feature} 夹具产物未构建——宿主侧 e2e 会在产物缺失处显性报错（先补工具链跑本 keeper）"
        );
        return;
    }
    let bytes = build_sdk_fixture(feature);
    assert!(
        bytes.len() > 1024,
        "{feature} 夹具产物异常（{} 字节）——构建成功但产物为空/截断",
        bytes.len()
    );
}

#[test]
fn crypto_fixture_artifact_is_built_for_host_e2e() {
    keep_fixture_alive("crypto");
}

#[test]
fn task_fixture_artifact_is_built_for_host_e2e() {
    keep_fixture_alive("task");
}

#[test]
fn pty_fixture_artifact_is_built_for_host_e2e() {
    keep_fixture_alive("pty");
}

#[test]
fn ws_fixture_artifact_is_built_for_host_e2e() {
    keep_fixture_alive("ws");
}

/// SDK 基础夹具（consumer = 桌面迁移 e2e：component_e2e / sdk_e2e / engine_e2e /
/// owner_e2e / wasm_flow_e2e——见上「票 04 补充」）
#[test]
fn sdk_fixture_artifact_is_built_for_host_e2e() {
    keep_fixture_alive("sdk");
}

/// HTTP 域夹具（consumer = 桌面迁移 e2e：http_e2e）
#[test]
fn http_fixture_artifact_is_built_for_host_e2e() {
    keep_fixture_alive("http");
}

/// 系统组件夹具（consumer = 桌面 `tests/system_component_test.rs` 的 L1 基础服务 /
/// 能力路由 / trap 隔离场景）。它不走 `plugin-sdk-fixtures` 的 feature 合集，而是
/// 独立 crate `bedcode-desktop/packages/plugin-system-test`，故构建器也独立；
/// 缺本keeper 时该夹具**无人构建**，桌面集成测试会在 `test_support::
/// system_test_artifact_bytes` 的「产物缺失」上红——6 个用例同因。
#[test]
fn system_test_fixture_artifact_is_built_for_host_e2e() {
    if !super::wasip3_toolchain_ready() {
        eprintln!(
            "[skip] wasip3 工具链未安装，system-test 夹具产物未构建——桌面 system_component_test 会在产物缺失处显性报错（先补工具链跑本 keeper）"
        );
        return;
    }
    let bytes = super::build_system_test_fixture();
    assert!(
        bytes.len() > 1024,
        "system-test 夹具产物异常（{} 字节）——构建成功但产物为空/截断",
        bytes.len()
    );
}
