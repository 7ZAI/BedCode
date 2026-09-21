//! Auth Types
//!
//! 认证相关类型定义

use serde::{Deserialize, Serialize};

/// 认证阶段
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthStage {
    /// 交换证书（生物凭证绑定：移动端上报公钥，空公钥表示解绑）
    ExchangeCertificate,
    /// 生物认证请求（移动端 → 桌面端，请求挑战值）
    BiometricRequest,
    /// 生物认证挑战值下发（桌面端 → 移动端，携带一次性随机数）
    BiometricChallenge,
    /// 生物认证应答（移动端 → 桌面端，携带挑战值签名）
    BiometricVerify,
    /// 认证成功
    Authenticated,
    /// JWT 重新认证（移动端发送，携带 session_token）
    Reauthenticate,
    /// 认证失败
    Failed,
}

/// 认证载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthPayload {
    /// 认证阶段
    pub stage: AuthStage,
    /// 设备 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    /// 设备名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    /// 设备指纹
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_fingerprint: Option<String>,
    /// 配对码
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pairing_code: Option<String>,
    /// 会话令牌
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,
    /// 错误消息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// QR 令牌
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qr_token: Option<String>,
    /// 生物凭证公钥（SPKI base64，绑定/解绑时携带）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    /// 生物认证挑战值（一次性随机数）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub challenge_nonce: Option<String>,
    /// 生物认证挑战值签名（base64，r||s 原始格式）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// 实际使用的认证方式（pairing_code / qr / biometric / jwt）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_method: Option<String>,
    /// WS 链路加密协商（issue 04）：Reauthenticate 请求携临时公钥，
    /// 认证成功响应携服务端临时公钥回执（auth_ok/auth 明文回执，此后帧加密）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crypto: Option<CryptoProposal>,
}

/// WS 链路加密协商载荷（issue 04）：请求侧携客户端临时 X25519 公钥（ek），
/// 响应侧携服务端临时公钥回执。v 当前固定 1
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CryptoProposal {
    pub v: u8,
    /// 临时 X25519 公钥（base64）
    pub ek: String,
}

impl Default for AuthPayload {
    fn default() -> Self {
        Self {
            // 占位值：调用方以 `..Default::default()` 填充其余字段并显式覆盖 stage，
            // 旧 RequestPairing/VerifyCode 配对 stage 已随 WS 配对 HTTP 化下线删除
            stage: AuthStage::Failed,
            device_id: None,
            device_name: None,
            device_fingerprint: None,
            pairing_code: None,
            session_token: None,
            error: None,
            qr_token: None,
            public_key: None,
            challenge_nonce: None,
            signature: None,
            auth_method: None,
            crypto: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每个 AuthStage 变体序列化 → 反序列化 → 等值（跨端协议表面，票据 23）
    #[test]
    fn auth_stage_serde_roundtrip_all_variants() {
        let stages = [
            AuthStage::ExchangeCertificate,
            AuthStage::BiometricRequest,
            AuthStage::BiometricChallenge,
            AuthStage::BiometricVerify,
            AuthStage::Authenticated,
            AuthStage::Reauthenticate,
            AuthStage::Failed,
        ];
        for stage in stages {
            let json = serde_json::to_string(&stage).unwrap();
            let back: AuthStage = serde_json::from_str(&json).unwrap();
            assert_eq!(back, stage, "变体往返不一致: {json}");
        }
    }

    /// snake_case 标签锁（跨端契约：移动端 TS 依赖这些字面量）
    #[test]
    fn auth_stage_wire_labels_locked() {
        assert_eq!(
            serde_json::to_string(&AuthStage::Authenticated).unwrap(),
            "\"authenticated\""
        );
        assert_eq!(
            serde_json::to_string(&AuthStage::ExchangeCertificate).unwrap(),
            "\"exchange_certificate\""
        );
        assert_eq!(
            serde_json::to_string(&AuthStage::Reauthenticate).unwrap(),
            "\"reauthenticate\""
        );
    }

    /// 未知 variant 反序列化拒绝（协议错位不得静默吞掉）
    #[test]
    fn auth_stage_unknown_variant_rejected() {
        assert!(serde_json::from_str::<AuthStage>("\"bogus_stage\"").is_err());
    }

    #[test]
    fn auth_payload_roundtrip_full_fields() {
        let payload = AuthPayload {
            stage: AuthStage::BiometricVerify,
            device_id: Some("d1".to_string()),
            device_name: Some("Pixel 8".to_string()),
            device_fingerprint: Some("abc123".to_string()),
            public_key: Some("SPKI-BASE64".to_string()),
            challenge_nonce: Some("nonce-1".to_string()),
            signature: Some("sig-1".to_string()),
            auth_method: Some("biometric".to_string()),
            crypto: Some(CryptoProposal {
                v: 1,
                ek: "EK==".to_string(),
            }),
            ..Default::default()
        };
        let json = serde_json::to_string(&payload).unwrap();
        let back: AuthPayload = serde_json::from_str(&json).unwrap();
        // 无 PartialEq 派生，逐字段断言关键字段
        assert_eq!(back.stage, AuthStage::BiometricVerify);
        assert_eq!(back.device_id.as_deref(), Some("d1"));
        assert_eq!(back.public_key.as_deref(), Some("SPKI-BASE64"));
        assert_eq!(back.crypto.as_ref().map(|c| c.ek.as_str()), Some("EK=="));
        // 空字段被 skip 掉（默认序列化紧凑形态）
        assert!(!json.contains("pairing_code"), "空字段应被跳过: {json}");
    }

    #[test]
    fn crypto_proposal_camel_case_wire_labels() {
        // camelCase 标签锁（crypto 协商字段，移动端 TS 同构）
        let json = serde_json::to_string(&CryptoProposal {
            v: 1,
            ek: "EK==".to_string(),
        })
        .unwrap();
        assert!(json.contains("\"ek\""), "camelCase ek 标签: {json}");
        assert!(!json.contains("ek_"), "不得出现 snake_case 标签: {json}");
    }
}
