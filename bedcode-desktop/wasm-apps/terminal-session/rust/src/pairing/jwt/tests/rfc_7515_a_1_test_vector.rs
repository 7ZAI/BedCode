//! RFC 7515 §A.1 官方 test vector — crate 内单元测试（自 bedcode-desktop/wasm-apps/terminal-session/rust/src/pairing/jwt.rs 迁出）

use super::*;

/// HS256 官方 test vector（RFC 7515 §A.1.1）：给定 key/header/payload，
/// 签名必须等于官方输出。注意 RFC 原文 header/payload 含 `\r\n` 换行与
/// 缩进（为展示方便）；signing input 必须逐字节复刻 RFC 值。
/// 这是「宿主 jsonwebtoken 与插件自实现等价」的密码学锚点
/// （jsonwebtoken 的 HMAC 实现与 RFC 向量一致）。
#[test]
fn rfc7515_a1_hs256_official_vector() {
    // RFC 7515 §A.1 原文 base64url（header: {"typ":"JWT",\r\n "alg":"HS256"}；
    // payload: {"iss":"joe",\r\n "exp":1300819380,\r\n "http://example.com/is_root":true}）
    let signing_input = concat!(
        "eyJ0eXAiOiJKV1QiLA0KICJhbGciOiJIUzI1NiJ9",
        ".",
        "eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFtcGxlLmNvbS9pc19yb290Ijp0cnVlfQ",
    );

    // 官方签名值（RFC 7515 §A.1.1，经独立 Python hmac 实现交叉验证）
    assert_eq!(
        sign_hs256(&rfc7515_key(), signing_input),
        "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk",
        "HS256 签名必须等于 RFC 7515 A.1 官方输出"
    );
}
