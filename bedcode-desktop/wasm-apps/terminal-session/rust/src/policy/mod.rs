//! policy 模块（票 05 自认证中心搬入会话中心）— 认证策略（连接放行/拒绝）
//!
//! 消费方 = 宿主 server 认证中间件：连接建立验签**执行留宿主**（密码学引擎
//! 不移动，spec D3「不动」表；插件密钥域与宿主不同，无法也不应验签——红线
//! 「验签执行点留宿主」），验签通过后经 `auth-policy` 能力导出
//! （`verify-device-token`，desktop 独有）取本模块策略裁决。
//!
//! 策略：
//! 1. **结构策略**：token 恰好三段 base64url、payload 可解码为 `JwtClaims`
//!    → 否则拒绝（宿主验签已保证签名/结构合法，此处为独立复检——结构/claims
//!    解析失败即策略拒绝，不信任宿主传入的中间结果）
//! 2. **claims 策略**：`iss == JWT_ISSUER`、`sub` 非空 → 否则拒绝
//! 3. **时效策略**：`exp` 未过（宿主已验签，此处防御性复检）→ 过期拒绝
//! 4. **信任策略**：`fingerprint` 有值 → 查内核 `pairings` 原始记录（host-auth
//!    `trusted-devices-list`，票 05 起为真源，插件不再自持镜像）：记录存在且
//!    `isActive = false`（已撤销）→ 拒绝；记录活跃 / 不存在 → 放行（不存在 =
//!    无信任锚点，仅凭验签——**刻意保持搬迁前语义**，收紧为「未配对即拒绝」是
//!    新协议决策，不在本批次）；记录面读取失败 → 放行（log_warn：撤销是唯一
//!    显式拒绝信号，读取失败不能误杀全部连接）。fingerprint 缺失 → 放行。
//!
//! 返回：放行 → claims JSON；拒绝 → 错误（拒绝原因，宿主透传连接方/日志）。

#[cfg(any(test, target_arch = "wasm32"))]
use crate::pairing::jwt::{self, JwtClaims};
#[cfg(any(test, target_arch = "wasm32"))]
use crate::trust::model::PairingRecord;
#[cfg(target_arch = "wasm32")]
use crate::trust::source::TrustRecords;

/// 策略裁决核心（纯函数，native 单测直接覆盖；信任快照由调用方注入）
///
/// 信任语义：撤销是唯一显式拒绝信号（`active=false`）；镜像未命中（迁移期
/// 未同步）从宽放行。调用方（wasm 包装）负责注入当前 trust 镜像快照。
///
/// cfg：wasm 运行时（`verify_device_token` 真实裁决）与 native 单测（注入
/// 快照直接覆盖）两个消费面；纯 native 非测试构建无消费方，不外露 API。
#[cfg(any(test, target_arch = "wasm32"))]
pub(crate) fn evaluate(token: &str, trust_records: &[PairingRecord]) -> Result<String, String> {
    // 1. 结构策略：三段 base64url + claims 可解析（独立复检，不信任宿主中间结果）
    let claims = decode_claims(token)?;
    // 2. claims 策略
    if claims.iss != jwt::JWT_ISSUER {
        return Err(format!("token issuer policy violation: {}", claims.iss));
    }
    if claims.sub.is_empty() {
        return Err("token subject missing".to_string());
    }
    // 3. 时效策略（防御性复检——宿主已验签，此处确保策略决策独立成立）
    if claims.is_expired_at(jwt::now_secs()) {
        return Err("token expired".to_string());
    }
    // 4. 信任策略：指纹撤销检查
    if let Some(fp) = claims.fingerprint.as_deref() {
        if !fp.is_empty()
            && trust_records
                .iter()
                .find(|r| r.device_fingerprint == fp)
                .is_some_and(|r| !r.is_active)
        {
            return Err("device revoked from trust list".to_string());
        }
    }
    // 放行：返回 claims JSON（宿主侧以自身验签结果为准，此处为策略裁决结果）
    serde_json::to_string(&claims).map_err(|e| format!("claims serialize: {}", e))
}

/// 结构解码：JWT 三段切分 → payload base64url 解码 → `JwtClaims` 解析
///
/// 不验签（验签执行点留宿主中间件，spec §3 红线；插件密钥域与宿主不同，
/// 无法也不应验签）——结构/claims 解码失败即策略拒绝。
#[cfg(any(test, target_arch = "wasm32"))]
fn decode_claims(token: &str) -> Result<JwtClaims, String> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err("malformed token (expected 3 segments)".to_string());
    }
    let payload_bytes = jwt::b64url_decode(parts[1])
        .ok_or_else(|| "malformed token (payload not base64url)".to_string())?;
    serde_json::from_slice::<JwtClaims>(&payload_bytes)
        .map_err(|e| format!("malformed token (claims parse): {}", e))
}

/// wasm 运行时：真实策略（读取内核 `pairings` 原始记录后按 [`evaluate`] 裁决）
///
/// 记录面读取失败 → log_warn + 按空记录放行（撤销是唯一显式拒绝信号；读取失败
/// 不能误杀全部连接——与搬迁前镜像读取失败的从宽语义一致）。
#[cfg(target_arch = "wasm32")]
pub(crate) fn verify_device_token(token: &str) -> Result<String, String> {
    use bedcode_plugin_api::host::HostLog;
    let host = bedcode_plugin_api::wasm_host::WasmHost;
    match host.records() {
        Ok(records) => evaluate(token, &records),
        Err(e) => {
            host.log_warn(&format!(
                "policy: trust records read failed (allow by policy): {}",
                e
            ));
            evaluate(token, &[])
        }
    }
}

/// native（cargo test）路径：密钥/存储依赖宿主，显性失败；单测经 [`evaluate`]
/// 注入信任快照直接覆盖策略核心
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn verify_device_token(_token: &str) -> Result<String, String> {
    Err("auth-policy unavailable outside wasm runtime".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pairing::jwt::JwtService;

    /// 构造测试 token（插件 HS256 自实现，与宿主同一格式；测试仅需结构/策略，
    /// 验签由宿主负责——此处签名只为构造合法三段 token）
    fn token(sub: &str, iss: &str, fingerprint: Option<&str>, expires_in_secs: u64) -> String {
        let mut claims = JwtClaims::new_at(
            sub.to_string(),
            Some("Pixel 9".to_string()),
            fingerprint.map(String::from),
            expires_in_secs,
            jwt::now_secs(),
        );
        claims.iss = iss.to_string();
        JwtService::with_key([0x42u8; 32].to_vec())
            .encode(&claims)
            .expect("encode token")
    }

    fn revoked_record(fp: &str) -> PairingRecord {
        PairingRecord {
            id: "p-1".to_string(),
            device_name: "Pixel 9".to_string(),
            device_fingerprint: fp.to_string(),
            address: None,
            paired_at: "2026-09-19T00:00:00Z".to_string(),
            last_seen: None,
            connect_count: 0,
            is_active: false,
        }
    }

    fn active_record(fp: &str) -> PairingRecord {
        let mut r = revoked_record(fp);
        r.is_active = true;
        r
    }

    /// 正例：合法 token（iss/sub/指纹齐全、未过期）+ 空镜像 → 放行，claims JSON 可回读
    #[test]
    fn valid_token_allows_with_claims_json() {
        let t = token("device-1", "BedCode", Some("fp-abc"), 3600);
        let out = evaluate(&t, &[]).expect("allow");
        let claims: JwtClaims = serde_json::from_str(&out).expect("claims json");
        assert_eq!(claims.sub, "device-1");
        assert_eq!(claims.fingerprint.as_deref(), Some("fp-abc"));
    }

    /// 正例：指纹已信任（active=true）→ 放行
    #[test]
    fn trusted_fingerprint_allows() {
        let t = token("device-1", "BedCode", Some("fp-abc"), 3600);
        assert!(evaluate(&t, &[active_record("fp-abc")]).is_ok());
    }

    /// 反例：指纹已撤销（active=false）→ 拒绝，带撤销语义
    #[test]
    fn revoked_fingerprint_denies() {
        let t = token("device-1", "BedCode", Some("fp-abc"), 3600);
        let err = evaluate(&t, &[revoked_record("fp-abc")]).expect_err("deny");
        assert!(err.contains("revoked"), "拒绝原因必须可读: {}", err);
    }

    /// 边界：指纹存在但内核无记录（无信任锚点）→ 从宽放行
    /// （搬迁前镜像语义的保持；收紧为「未配对即拒绝」是新协议决策，不在本批次）
    #[test]
    fn unknown_fingerprint_allows() {
        let t = token("device-1", "BedCode", Some("fp-ghost"), 3600);
        assert!(evaluate(&t, &[active_record("fp-abc")]).is_ok());
    }

    /// 边界：指纹缺失 → 放行（无信任锚点，仅凭验签）
    #[test]
    fn missing_fingerprint_allows() {
        let t = token("device-1", "BedCode", None, 3600);
        assert!(evaluate(&t, &[]).is_ok());
    }

    /// 反例：iss 策略违反（非 JWT_ISSUER）→ 拒绝
    #[test]
    fn wrong_issuer_denies() {
        let t = token("device-1", "Evil", Some("fp-abc"), 3600);
        let err = evaluate(&t, &[]).expect_err("deny");
        assert!(err.contains("issuer"), "拒绝原因必须可读: {}", err);
    }

    /// 反例：sub 为空 → 拒绝
    #[test]
    fn empty_subject_denies() {
        let t = token("", "BedCode", Some("fp-abc"), 3600);
        assert!(evaluate(&t, &[]).is_err());
    }

    /// 反例：过期 token → 拒绝
    #[test]
    fn expired_token_denies() {
        // exp = now - 1（严格 `<` 判过期，exp == now 不算过期——与宿主 is_expired 语义一致）
        let claims = JwtClaims::new_at(
            "device-1".to_string(),
            Some("Pixel 9".to_string()),
            Some("fp-abc".to_string()),
            0,
            jwt::now_secs() - 1,
        );
        let t = JwtService::with_key([0x42u8; 32].to_vec())
            .encode(&claims)
            .expect("encode token");
        assert!(evaluate(&t, &[]).is_err());
    }

    /// 反例：结构非法（非三段 / payload 非 base64url / claims 不可解析）→ 拒绝
    #[test]
    fn malformed_tokens_deny() {
        assert!(evaluate("one.segment", &[]).is_err());
        assert!(evaluate("a.b.c", &[]).is_err(), "b 非 base64url");
        // 合法 base64url 但非 claims JSON
        let bad_payload = format!("x.{}.y", jwt::b64url_encode(b"not-json"));
        assert!(evaluate(&bad_payload, &[]).is_err());
    }

    /// 边界：撤销优先于结构合法——已撤销设备的合法 token 也拒绝
    #[test]
    fn revocation_overrides_valid_signature() {
        let t = token("device-1", "BedCode", Some("fp-abc"), 3600);
        let err = evaluate(&t, &[revoked_record("fp-abc")]).expect_err("deny");
        assert!(err.contains("revoked"));
    }

    /// native 路径显性失败（无宿主环境）
    #[test]
    fn native_path_fails_without_host() {
        assert!(verify_device_token("x.y.z").is_err());
    }
}
