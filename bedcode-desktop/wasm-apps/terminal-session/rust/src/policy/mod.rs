//! policy 模块（票 05 自认证中心搬入会话中心 · ADR 0033 起**兼验签点**）—— 认证策略
//!
//! 消费方 = 宿主 server 认证面（HTTP `/api` 中间件 + WS 插件端点首消息）。自
//! v33 / ADR 0033 起**验签与策略裁决收为同一次调用**：宿主不再持有任何设备 JWT
//! 密码学（`utils/auth/jwt.rs` 已退役），宿主只经注册表找到认证中心，调本导出；
//! 本模块内部先验签（`pairing::jwt`，密钥来自本插件的密钥环）再逐条做策略。
//!
//! **为什么在一处**：对称密码学下验签方必须持密钥 ⇒ 验签与裁决拆到两个地方就必然
//! 把密钥推到宿主。本模块本来就是策略汇聚点，验签只是「补上一步」而非新建链路。
//!
//! 策略（顺序即短路顺序，先验签后语义，避免在无效 token 上浪费解析）：
//! 1. **密码学策略**：`HS256` 签名有效（密钥环逐代尝试——轮换宽限期内旧代仍合法）。
//!    签名无效 → 拒绝（不泄露是哪一步失败）
//! 2. **结构策略**：token 恰好三段 base64url、payload 可解码为 `JwtClaims`
//!    → 否则拒绝（**不信任调用方传入的中间结果**，claims 一律自己解）
//! 3. **claims 策略**：`iss == JWT_ISSUER`、`sub` 非空 → 否则拒绝
//! 4. **时效策略**：`exp` 未过 → 过期拒绝
//! 5. **信任策略**：`fingerprint` 有值 → 查内核 `pairings` 原始记录（host-auth
//!    `trusted-devices-list`，票 05 起为真源，插件不再自持镜像）：记录存在且
//!    `isActive = false`（已撤销）→ 拒绝；记录活跃 / 不存在 → 放行（不存在 =
//!    无信任锚点，仅凭验签——**刻意保持搬迁前语义**，收紧为「未配对即拒绝」是
//!    新协议决策，不在本批次）；记录面读取失败 → 放行（log_warn：撤销是唯一
//!    显式拒绝信号，读取失败不能误杀全部连接）。fingerprint 缺失 → 放行。
//!
//! 返回：放行 → claims JSON（宿主据此建立连接身份 / 转发 `caller` 上下文）；
//! 拒绝 → 错误（拒绝原因，宿主透传连接方/日志）。

#[cfg(any(test, target_arch = "wasm32"))]
use crate::pairing::jwt::{self, JwtClaims, JwtError};
#[cfg(any(test, target_arch = "wasm32"))]
use crate::trust::model::PairingRecord;
#[cfg(target_arch = "wasm32")]
use crate::trust::source::TrustRecords;

/// 策略裁决核心（纯函数，native 单测直接覆盖；信任快照与密钥由调用方注入）
///
/// 入参：
/// - `token`：待裁决的设备入场凭证
/// - `keys`：本插件密钥环的**有序**候选密钥（当前代优先）——轮换宽限期内上一代
///   仍然合法（ADR 0033 D4）
/// - `trust_records`：当前 `pairings` 原始记录快照
///
/// 信任语义：撤销是唯一显式拒绝信号（`active=false`）；镜像未命中（迁移期
/// 未同步）从宽放行。调用方（wasm 包装）负责注入密钥与信任快照。
///
/// cfg：wasm 运行时（`verify_device_token` 真实裁决）与 native 单测（注入
/// 快照直接覆盖）两个消费面；纯 native 非测试构建无消费方，不外露 API。
#[cfg(any(test, target_arch = "wasm32"))]
pub(crate) fn evaluate(
    token: &str,
    keys: &[&[u8]],
    trust_records: &[PairingRecord],
) -> Result<String, String> {
    // 0. 密码学策略（ADR 0033：宿主不再验签，这里是**唯一**验签点）。
    //    `verify_with_keys` 内部已含结构 / alg / claims / 时效复检，故下一步的
    //    `decode_claims` 不会与它矛盾（同一段解码、同一条判据）。
    let verified = jwt::verify_with_keys(keys, token, jwt::now_secs())
        .map_err(|e| match e {
            JwtError::TokenExpired => "token expired".to_string(),
            _ => "token signature invalid".to_string(),
        })?;
    // 1. 结构策略：claims 必须能独立回解（不信任任何外部传入的中间结果）
    let claims = decode_claims(token)?;
    debug_assert_eq!(
        claims, verified,
        "验签产出的 claims 与独立回解必须一致（同一判据、同一输入）"
    );
    // 2. claims 策略
    if claims.iss != jwt::JWT_ISSUER {
        return Err(format!("token issuer policy violation: {}", claims.iss));
    }
    if claims.sub.is_empty() {
        return Err("token subject missing".to_string());
    }
    // 3. 时效策略（严格 `exp < now`，与验签路径的 leeway 语义不同：
    //    验签要兼容 60s 抖动，策略层要「此刻确实还有效」）
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
    // 放行：返回 claims JSON（宿主据此建立连接身份 / 转发 caller 上下文）
    serde_json::to_string(&claims).map_err(|e| format!("claims serialize: {}", e))
}

/// 结构解码：JWT 三段切分 → payload base64url 解码 → `JwtClaims` 解析
///
/// **不验签**（密码学由 [`evaluate`] 第 0 步的 `verify_with_keys` 完成）——本函数
/// 只做结构复检：即使签名有效，结构/claims 解不出来也是策略拒绝（两端判据分离，
/// 避免「验签通过就当 claims 可用」）。
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

/// wasm 运行时：真实裁决——取密钥环候选密钥 + 读 `pairings` 原始记录后
/// 按 [`evaluate`] 执行（**先验签再策略**）
///
/// 两步都可能失败，各有各的处置：
/// - 密钥环不可用（存储故障 / 密钥环损坏）→ **显性拒绝**（`auth keyring unavailable`）。
///   绝不降级为进程随机密钥：设备会以为已配对，实际每次重启全灭
/// - 记录面读取失败 → log_warn + 按空记录放行（撤销是唯一显式拒绝信号；读取失败
///   不能误杀全部连接——与搬迁前镜像读取失败的从宽语义一致）
#[cfg(target_arch = "wasm32")]
pub(crate) fn verify_device_token(token: &str) -> Result<String, String> {
    use bedcode_plugin_api::host::HostLog;
    let host = bedcode_plugin_api::wasm_host::WasmHost;
    let ring = crate::pairing::keys::keyring_from_host_auth()
        .map_err(|e| format!("auth keyring unavailable: {e}"))?;
    let decision = match host.records() {
        Ok(records) => evaluate(token, &ring.verification_keys(), &records),
        Err(e) => {
            host.log_warn(&format!(
                "policy: trust records read failed (allow by policy): {}",
                e
            ));
            evaluate(token, &ring.verification_keys(), &[])
        }
    };
    // `kid` 诊断（不是闸门——签名才是闸门，见 `auth_http::jwt::verify_token_with`）：
    // 放行但 `kid` 不在环内 = token 自称一个已被裁掉的代次（宽限期已过的旧 token
    // 不可能验过，所以这要么是有人手改了 kid，要么密钥环被外部动过），留痕以便排障。
    if let Ok(claims_json) = &decision {
        let kid = serde_json::from_str::<serde_json::Value>(claims_json)
            .ok()
            .and_then(|v| v.get("kid").and_then(|k| k.as_str()).map(str::to_string));
        if let Some(kid) = kid.filter(|k| !k.is_empty()) {
            if !ring.knows_kid(Some(&kid)) {
                host.log_warn(&format!(
                    "policy: token accepted with unknown key generation {kid} (diagnostic only)"
                ));
            }
        }
    }
    decision
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

    /// 测试密钥环（两代：当前 `0x42..` + 上一代 `0x24..`，`kid` 与代次对应）
    fn ring_keys() -> Vec<Vec<u8>> {
        vec![vec![0x42u8; 32], vec![0x24u8; 32]]
    }

    /// 候选集视图（当前代优先——与 `keys::Keyring::verification_keys` 同序）
    fn ring_refs(keys: &[Vec<u8>]) -> Vec<&[u8]> {
        keys.iter().map(|k| k.as_slice()).collect()
    }

    /// 构造测试 token（用**真实签发面**签名——本模块就是验签方，不能拿假签名测）
    fn token(sub: &str, iss: &str, fingerprint: Option<&str>, expires_in_secs: u64) -> String {
        token_with(sub, iss, fingerprint, expires_in_secs, Some("g1"), 0)
    }

    /// 指定密钥下标 + `kid` 的 token 构造（跨代 / 伪造 kid 用）
    fn token_with(
        sub: &str,
        iss: &str,
        fingerprint: Option<&str>,
        expires_in_secs: u64,
        kid: Option<&str>,
        key_index: usize,
    ) -> String {
        let keys = ring_keys();
        let mut claims = JwtClaims::new_at(
            sub.to_string(),
            Some("Pixel 9".to_string()),
            fingerprint.map(String::from),
            expires_in_secs,
            jwt::now_secs(),
        );
        claims.iss = iss.to_string();
        claims.kid = kid.map(String::from);
        let key = keys[key_index].clone();
        let svc = match kid {
            Some(kid) => JwtService::with_kid(key, kid.to_string()),
            None => JwtService::with_key(key),
        };
        svc.encode(&claims).expect("encode token")
    }

    /// 便捷裁决：候选集 = 测试密钥环全代
    fn eval(token: &str, records: &[PairingRecord]) -> Result<String, String> {
        let keys = ring_keys();
        evaluate(token, &ring_refs(&keys), records)
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

    // ==================== 放行（正例 / 边界） ====================

    /// 正例：合法 token（签名有效 + iss/sub/指纹齐全 + 未过期）+ 空镜像 → 放行，
    /// claims JSON 可回读
    #[test]
    fn valid_token_allows_with_claims_json() {
        let t = token("device-1", "BedCode", Some("fp-abc"), 3600);
        let out = eval(&t, &[]).expect("allow");
        let claims: JwtClaims = serde_json::from_str(&out).expect("claims json");
        assert_eq!(claims.sub, "device-1");
        assert_eq!(claims.fingerprint.as_deref(), Some("fp-abc"));
    }

    /// 正例：指纹已信任（active=true）→ 放行
    #[test]
    fn trusted_fingerprint_allows() {
        let t = token("device-1", "BedCode", Some("fp-abc"), 3600);
        assert!(eval(&t, &[active_record("fp-abc")]).is_ok());
    }

    /// 边界：指纹存在但内核无记录（无信任锚点）→ 从宽放行
    /// （搬迁前镜像语义的保持；收紧为「未配对即拒绝」是新协议决策，不在本批次）
    #[test]
    fn unknown_fingerprint_allows() {
        let t = token("device-1", "BedCode", Some("fp-ghost"), 3600);
        assert!(eval(&t, &[active_record("fp-abc")]).is_ok());
    }

    /// 边界：指纹缺失 → 放行（无信任锚点，仅凭验签）
    #[test]
    fn missing_fingerprint_allows() {
        let t = token("device-1", "BedCode", None, 3600);
        assert!(eval(&t, &[]).is_ok());
    }

    // ==================== 验签（ADR 0033 新增的第 0 道关） ====================

    /// 反例：**签名被篡改** → 拒绝（claims 完全合法时也拒——这是 ADR 0033 把验签
    /// 收进本模块后最核心的断言：宿主不再验签，这里是唯一防线）
    #[test]
    fn tampered_signature_denies_even_with_valid_claims() {
        let t = token("device-1", "BedCode", Some("fp-abc"), 3600);
        let tampered = format!("{}x", &t[..t.len() - 4]);
        let err = eval(&tampered, &[]).expect_err("篡改签名必须拒绝");
        assert!(
            err.contains("signature"),
            "拒绝原因必须指向签名: {err}"
        );
    }

    /// 反例：**用别的密钥签的 token** → 拒绝（伪造者拿不到密钥环）
    #[test]
    fn token_signed_by_foreign_key_denies() {
        let mut claims = JwtClaims::new_at(
            "device-1".to_string(),
            Some("Pixel 9".to_string()),
            Some("fp-abc".to_string()),
            3600,
            jwt::now_secs(),
        );
        claims.kid = Some("g1".to_string());
        let foreign = JwtService::with_kid(vec![0x77u8; 32], "g1".to_string())
            .encode(&claims)
            .expect("encode");
        let err = eval(&foreign, &[]).expect_err("外来密钥签的 token 必须拒绝");
        assert!(err.contains("signature"), "got: {err}");
    }

    /// 反例：**空候选集**（密钥环不可用）→ 拒绝，绝不放行
    #[test]
    fn empty_key_set_denies_rather_than_allows() {
        let t = token("device-1", "BedCode", Some("fp-abc"), 3600);
        let err = evaluate(&t, &[], &[]).expect_err("无密钥必须拒绝");
        assert!(err.contains("signature"), "got: {err}");
    }

    /// 边界：轮换宽限期内，上一代密钥签的 token 仍放行（ADR 0033 D4）
    #[test]
    fn previous_generation_still_verifies_within_grace_window() {
        // 上一代密钥（ring_keys 的 [1]）签的 token，kid 仍写 g1（代次标识不随密钥变）
        let t = token_with("device-1", "BedCode", Some("fp-abc"), 3600, Some("g1"), 1);
        assert!(eval(&t, &[]).is_ok(), "宽限期内旧代 token 必须放行");
    }

    /// 边界：无 `kid` 的迁移前形态 token → 仍可验签（不因认不出 kid 而误拒）
    #[test]
    fn legacy_token_without_kid_still_allows() {
        let t = token_with("device-1", "BedCode", Some("fp-abc"), 3600, None, 0);
        let out = eval(&t, &[]).expect("无 kid 的旧 token 必须放行");
        let claims: JwtClaims = serde_json::from_str(&out).expect("claims json");
        assert_eq!(claims.kid, None);
    }

    // ==================== 拒绝（策略四类，语义不变） ====================

    /// 反例：iss 策略违反（非 JWT_ISSUER）→ 拒绝
    #[test]
    fn wrong_issuer_denies() {
        let t = token("device-1", "Evil", Some("fp-abc"), 3600);
        let err = eval(&t, &[]).expect_err("deny");
        assert!(err.contains("issuer"), "拒绝原因必须可读: {}", err);
    }

    /// 反例：sub 为空 → 拒绝
    #[test]
    fn empty_subject_denies() {
        let t = token("", "BedCode", Some("fp-abc"), 3600);
        assert!(eval(&t, &[]).is_err());
    }

    /// 反例：过期 token → 拒绝
    #[test]
    fn expired_token_denies() {
        // exp = now - 1（严格 `<` 判过期，exp == now 不算过期）
        let claims = JwtClaims::new_at(
            "device-1".to_string(),
            Some("Pixel 9".to_string()),
            Some("fp-abc".to_string()),
            0,
            jwt::now_secs() - 1,
        );
        let t = JwtService::with_key(vec![0x42u8; 32])
            .encode(&claims)
            .expect("encode token");
        assert!(eval(&t, &[]).is_err());
    }

    /// 反例：结构非法（非三段 / payload 非 base64url / claims 不可解析）→ 拒绝
    #[test]
    fn malformed_tokens_deny() {
        assert!(eval("one.segment", &[]).is_err());
        assert!(eval("a.b.c", &[]).is_err(), "b 非 base64url");
        // 合法 base64url 但非 claims JSON
        let bad_payload = format!("x.{}.y", jwt::b64url_encode(b"not-json"));
        assert!(eval(&bad_payload, &[]).is_err());
    }

    /// 边界：撤销优先于其他一切——已撤销设备的**签名有效** token 也拒绝，
    /// 且拒绝原因指向撤销（不是签名）
    #[test]
    fn revocation_overrides_valid_signature() {
        let t = token("device-1", "BedCode", Some("fp-abc"), 3600);
        let err = eval(&t, &[revoked_record("fp-abc")]).expect_err("deny");
        assert!(err.contains("revoked"));
    }

    /// 反例：指纹已撤销（active=false）→ 拒绝，带撤销语义
    #[test]
    fn revoked_fingerprint_denies() {
        let t = token("device-1", "BedCode", Some("fp-abc"), 3600);
        let err = eval(&t, &[revoked_record("fp-abc")]).expect_err("deny");
        assert!(err.contains("revoked"), "拒绝原因必须可读: {}", err);
    }

    /// native 路径显性失败（无宿主环境）
    #[test]
    fn native_path_fails_without_host() {
        assert!(verify_device_token("x.y.z").is_err());
    }
}
