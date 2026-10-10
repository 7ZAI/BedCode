//! host-crypto 端到端（wasm-core 纯净性收口票 02 批次 04）
//!
//! WIT 实现迁宿主（`src/plugin/crypto.rs`，路径 B：宿主自带 bindgen + `Host` impl +
//! 域函数同处）后的 **wasm→宿主原语通路**验证——插件从 wasm 侧按名调用宿主加密引擎
//! 原语（AEAD 往返 + 未知名 fail-visible + X25519 双端共享 + KDF 派生），验证「插件
//! 真正用起来了」而非仅 SDK 绑定可编译。
//!
//! 自内核 `wasi_e2e` 的 host-crypto 探针段迁入：内核测试二进制不再注册该 interface
//! ⇒ 携带其 import 的夹具无法在内核测试里实例化，探针随域走宿主（夹具同步拆为独立
//! `crypto` feature，产物由 wasm-core 测试的 `fixture_keeper` 用例构建，本文件只读）。

use bedcode_desktop_lib::wasm_core::host_api::context::PermissionScope;
use bedcode_wasm_core::test_support::{sdk_fixture_artifact_bytes, setup_wasm_runtime};

#[test]
fn host_crypto_roundtrip_via_real_component() {
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    // 探针夹具声明三权限域（无头测试上下文不走 manifest 授权路径，显式补授——
    // 与管理器测试的 `grant_permissions` 同款）
    host_ctx.permission().grant_permissions(
        "com.bedcode.crypto-test",
        &[
            "crypto:aead".to_string(),
            "crypto:kdf".to_string(),
            "crypto:asym".to_string(),
        ],
    );
    let component = wasm_runtime
        .compile_component(&sdk_fixture_artifact_bytes("crypto"))
        .expect("compile crypto fixture component");
    let mut plugin = wasm_runtime
        .instantiate_component(&component, "com.bedcode.crypto-test", host_ctx, &[], None)
        .expect("instantiate crypto fixture");

    let r = plugin
        .invoke_command("host-crypto.roundtrip", "{}")
        .expect("host-crypto roundtrip command");
    let v = serde_json::from_str::<serde_json::Value>(&r).unwrap();
    assert_eq!(
        v.get("ok").and_then(|v| v.as_bool()),
        Some(true),
        "host-crypto 探针未通过: {r}"
    );
    assert_eq!(
        v.get("x25519").and_then(|v| v.as_bool()),
        Some(true),
        "x25519 双端共享未通过"
    );
}
