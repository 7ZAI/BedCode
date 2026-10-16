//! 测试夹具：经认证中心签发设备入场 token（ADR 0033）
//!
//! **为什么需要这个模块**：v33 起宿主**没有任何签发面**——测试再也不能自己造一枚
//! 「合法 token」了。唯一诚实的造法是走生产签发路径：认证中心的 `auth-grant` /
//! `jwt` / `issue` 动作（宿主经 `auth-method-invoke` 零解析转发进去）。这样每个
//! 断言都在真实链路上：中心自持密钥签出来的 token，中心自己当然认。
//!
//! **落点变迁**：原住 wasm-core `utils/auth/test_tokens.rs`（依赖其内部
//! `test_seed_plugin_secret`）；wasm-core 纯净性收口票 02 批次 04 把 host-auth 域
//! （含该种子函数）迁宿主 `src/plugin/auth.rs` 后，本夹具**随依赖迁回 lib**——
//! 旧理由（「迁回会让 wasm-core 自己的闭环测试失去造 token 通路」）随域迁出失效。
//! 消费方 = lib 集成测试（`src-tauri/tests/*`：session_e2e / system_component_test /
//! auth_center_perf），经 `bedcode_desktop_lib::utils::auth::test_tokens` 消费；
//! 注册表真源仍在 wasm-core `host_api::auth_center`（公开面取用）。
//!
//! 前置条件：中心已在册（`host_api::auth_center::is_registered()`）且其注册 methods 含
//! `jwt`——`com.bedcode.terminal-session` 的 `activate` 会自注册
//! `["pairing_code", "qr", "biometric", "jwt"]`（ADR 0031 K9）。

use bedcode_wasm_core::host_api::auth_center;
use bedcode_wasm_core::host_api::context::WasmHostContext;

/// 白盒夹具：把中心的入场密钥环**种成已知密钥**，供本模块用它签发测试 token
///
/// 为什么要种子而不是走 `auth-grant` 互调：互调依赖总线派发（`activate` 期
/// `bus_subscribe` 是 spawn 出去的异步落地），在部分 harness（自建 `PluginHost`
/// 而未 `init_message_bus`）里永远等不到回复——那是**测试基建**的限制，不是被测
/// 行为。种子走的是中心自己读密钥的那条路（`host-auth secret-get`），链路更短更直。
///
/// 种子的值就是生产形状：`{"active":1,"keys":{"1":"<hex32>"}}`（插件
/// `pairing::keys::KeyringFile` 的 serde 形状）。种完**必须在 activate 之前**——
/// 中心 `activate` 会读一次密钥环并校验格式，读不到或格式不对即阻断激活。
pub fn seed_keyring(host_ctx: &WasmHostContext, center_plugin_id: &str, key: &[u8]) {
    assert_eq!(key.len(), 32, "HS256 最小安全长度 32 字节");
    let value = serde_json::json!({
        "active": 1,
        "keys": { "1": hex::encode(key) },
    })
    .to_string();
    crate::plugin::auth::test_seed_plugin_secret(
        host_ctx,
        host_ctx,
        host_ctx,
        center_plugin_id,
        "jwt.keyring",
        &value,
    )
    .unwrap_or_else(|e| panic!("seed auth center keyring failed: {e}"));
}

/// 用上面种下的已知密钥签一枚 token（HS256，claims 形状与生产逐字一致）
///
/// 用 `jsonwebtoken`（宿主既有依赖）签：它与插件自实现的**字节级**等价由
/// 插件 `pairing/jwt.rs::legacy_wire_format_is_byte_frozen` 的冻结向量钉住——所以
/// 中心认它不是「碰巧」，是契约。
pub fn sign_with_seeded_key(
    key: &[u8],
    sub: &str,
    device_name: Option<&str>,
    fingerprint: Option<&str>,
) -> String {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after epoch")
        .as_secs();
    // serde_json 的 Map 保持插入序（未开 preserve_order 时按序迭代）——按生产
    // 声明顺序逐个插入，保证 payload 段与冻结向量同形
    let mut claims = serde_json::Map::new();
    claims.insert("sub".into(), serde_json::Value::String(sub.to_string()));
    claims.insert("iss".into(), serde_json::Value::String("BedCode".into()));
    claims.insert("iat".into(), serde_json::Value::from(now));
    claims.insert("exp".into(), serde_json::Value::from(now + 7 * 24 * 60 * 60));
    if let Some(name) = device_name {
        claims.insert("device_name".into(), serde_json::Value::String(name.into()));
    }
    if let Some(fp) = fingerprint {
        claims.insert("fingerprint".into(), serde_json::Value::String(fp.into()));
    }
    encode(
        &Header::new(Algorithm::HS256),
        &serde_json::Value::Object(claims),
        &EncodingKey::from_secret(key),
    )
    .expect("sign with seeded key")
}

/// 经认证中心签发一枚设备入场 token（走生产 `auth-grant` 路径）
///
/// 失败**直接 panic**：`issue` 是测试前置条件，前置失败时后续断言全无意义，
/// 与其让 20 个断言都红在一句「token 造不出来」上，不如立刻定位。
pub fn issue(
    host_ctx: &WasmHostContext,
    sub: &str,
    device_name: Option<&str>,
    fingerprint: Option<&str>,
) -> String {
    let params = serde_json::json!({
        "method": "issue",
        "params": {
            "sub": sub,
            "deviceName": device_name.unwrap_or(""),
            "fingerprint": fingerprint.unwrap_or(""),
        }
    });
    let reply = invoke_with_settle_retry(host_ctx, &params.to_string());
    let value: serde_json::Value =
        serde_json::from_str(&reply).unwrap_or_else(|e| panic!("auth center reply not JSON: {e}"));
    value["token"]
        .as_str()
        .unwrap_or_else(|| panic!("auth center reply missing token: {value}"))
        .to_string()
}

/// 有界重试的互调（等中心的 api 订阅落地，见模块头说明）
fn invoke_with_settle_retry(host_ctx: &WasmHostContext, payload: &str) -> String {
    const ATTEMPTS: usize = 20;
    const BACKOFF: std::time::Duration = std::time::Duration::from_millis(50);
    let mut last = String::new();
    for attempt in 1..=ATTEMPTS {
        match auth_center::invoke_auth_method(host_ctx, "jwt", payload) {
            Ok(reply) => return reply,
            Err(e) => {
                last = e;
                std::thread::sleep(BACKOFF);
                if attempt == ATTEMPTS {
                    break;
                }
            }
        }
    }
    panic!("issue entry token via auth center failed after {ATTEMPTS} attempts: {last}");
}
