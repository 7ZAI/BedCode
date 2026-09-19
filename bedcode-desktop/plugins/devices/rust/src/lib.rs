//! Device Auth Center Plugin (WASM)
//!
//! 认证中心（`com.bedcode.devices`，headless）：收敛远程终端配对 / 信任 / 同意
//! 决策语义为单一权威（auth-center-spec §3）。
//!
//! 已落地：
//! - 票 06 骨架：async 宿主加载/激活/可调用闭环 + manifest api/permissions 声明
//! - 票 07 pairing 模块：配对码 / QR token / JWT(HS256) 策略（`pairing/`），
//!   语义从宿主 `utils/auth` 平移（行为等价，对照测试）；密钥经 host-auth
//!   secret-store 托管（首启随机生成 + 持久化，明文不出宿主）
//! - 票 08 trust 模块：设备信任列表统一视图（`trust/`），pairing 记录
//!   host-storage 持久化（对齐宿主 `pairings` 表语义）+ peer trust_store
//!   经 host-peer 原语映射（对齐宿主 `list_trusted_peers` / `revoke_trusted_peer`）
//!
//! 形态：rust（纯 Rust 无前端，UI 由宿主命令面/互调 api 消费，前端 loader
//! 跳过，见 src/plugin/loader.ts）；
//! wasm32-wasip3 target（cdylib 直出 Component）；SDK 链路 —— `WasmPlugin`
//! trait + `wasm_entry!` 宏生成组件 world 导出。

mod consent;
mod pairing;
mod trust;

use bedcode_plugin_api::host::{HostBus, HostLog};
use bedcode_plugin_api::types::PluginManifest;
use bedcode_plugin_api::wasm::WasmPlugin;
use bedcode_plugin_api::wasm_host::WasmHost;
use bedcode_plugin_api::{plugin_api, BusMessage};
use pairing::code::PairingCode;
use pairing::jwt::JwtService;
use pairing::qr::QrTokenManager;
use std::sync::{Mutex, OnceLock};

/// 插件互调 api 声明（ADR 0017）：trait 方法名 ↔ manifest.api 条目
/// （`com.bedcode.devices.<method>`，`#[api(...)]` 覆盖为连字符名），宏在
/// 编译期比对防漂移。
///
/// 互调 api 面（票 09 起）：
/// - `hello`：探活（骨架期）
/// - `decide-consent`：首连确认决策（票 09 B4，消费方 file-transfer）
/// - `list-trusted-devices`：统一信任视图（票 09，消费方校验信任状态）
/// - `pairing-code-*` / `qr-code-*`（票 11 C2，宿主命令面桥接）：配对码 / QR
///   token 生命周期操作——宿主命令面（前端命令面 + server 配对端点）在认证中心
///   激活时经此互调转发，状态以认证中心为准（宿主侧降级兜底）
///
/// pairing 语义（票 07）经命令面暴露（宿主闭环测试用），互调 api 面归票 09。
#[plugin_api(manifest = "../plugin.json")]
pub trait DevicesApi {
    /// 探活：返回骨架状态（证明互调 api 声明 + 分派链路可用）
    fn hello() -> Result<String, String>;

    /// 首连确认决策（票 09 B4）：peer 信息 + 可选用户意向 → 放行 / 拒绝 / 需确认。
    /// 消费方 file-transfer 两阶段调用（阶段 1 无意向评估信任；阶段 2 回传意向）。
    #[api("decide-consent")]
    fn decide_consent(
        request: consent::model::ConsentRequest,
    ) -> Result<consent::model::ConsentDecision, String>;

    /// 统一信任视图（票 09）：配对 + 可信对端合并列表（trust 模块统一视图），
    /// 供消费方/设置面校验信任状态。
    #[api("list-trusted-devices")]
    fn list_trusted_devices() -> Result<serde_json::Value, String>;

    // ==================== 票 11 命令面桥接（C2） ====================
    // 宿主命令面转发入口：配对码/QR token 状态以认证中心为准（激活时），
    // 消费方 = 宿主 `utils/auth/auth_center.rs` 桥接层。

    /// 生成配对码（替换旧的）→ 宿主 `PairingCode` 形状
    /// `{code, created_at(RFC3339), expires_in(剩余秒)}`
    #[api("pairing-code-generate")]
    fn pairing_code_generate(ttl: u64) -> Result<serde_json::Value, String>;

    /// 当前配对码状态（过滤过期）→ 同 generate 形状 | null
    #[api("pairing-code-status")]
    fn pairing_code_status() -> Result<Option<serde_json::Value>, String>;

    /// 验证配对码（一次性：成功即消耗）→ valid 布尔
    #[api("pairing-code-verify")]
    fn pairing_code_verify(code: String) -> Result<bool, String>;

    /// 清除当前配对码
    #[api("pairing-code-clear")]
    fn pairing_code_clear() -> Result<(), String>;

    /// 生成 QR token（替换旧的）→ `{token, ttl, remaining}`
    #[api("qr-code-generate")]
    fn qr_code_generate(ttl: u64) -> Result<serde_json::Value, String>;

    /// 当前 QR token 状态（过滤过期/已用）→ `{token, ttl, remaining}` | null
    #[api("qr-code-status")]
    fn qr_code_status() -> Result<Option<serde_json::Value>, String>;

    /// 验证 QR token（一次性）→ `{valid, reason?}`；reason = 宿主错误分类同构
    /// 文本（`QR token expired` / `QR token already used` / `No active QR token`）
    #[api("qr-code-verify")]
    fn qr_code_verify(token: String) -> Result<serde_json::Value, String>;

    /// 清除当前 QR token
    #[api("qr-code-clear")]
    fn qr_code_clear() -> Result<(), String>;
}

/// 认证中心插件 — 生命周期 + 状态命令 + 探活 api
pub struct DevicesPlugin;

/// 配对码状态（跨命令调用持久；wasip3 的 thread_local 是真 TLS，实例状态
/// 必须 static Mutex —— 教训见 handoff §3）
static CURRENT_CODE: Mutex<Option<PairingCode>> = Mutex::new(None);

/// QR token 管理器状态（OnceLock：仅运行时首次访问初始化一次，返回 &'static）
static QR_MANAGER: OnceLock<QrTokenManager> = OnceLock::new();

impl DevicesApi for DevicesPlugin {
    fn hello() -> Result<String, String> {
        Ok("devices:hello".to_string())
    }

    fn decide_consent(
        request: consent::model::ConsentRequest,
    ) -> Result<consent::model::ConsentDecision, String> {
        consent::ops::decide_consent_via_host(request)
    }

    fn list_trusted_devices() -> Result<serde_json::Value, String> {
        trust::list_via_host()
    }

    // ==================== 票 11 命令面桥接（C2） ====================

    fn pairing_code_generate(ttl: u64) -> Result<serde_json::Value, String> {
        pair_code_generate_via_api(ttl)
    }

    fn pairing_code_status() -> Result<Option<serde_json::Value>, String> {
        pair_code_status_via_api()
    }

    fn pairing_code_verify(code: String) -> Result<bool, String> {
        pair_code_verify_via_api(&code)
    }

    fn pairing_code_clear() -> Result<(), String> {
        *CURRENT_CODE.lock().map_err(|e| format!("pairing code lock: {e}"))? = None;
        Ok(())
    }

    fn qr_code_generate(ttl: u64) -> Result<serde_json::Value, String> {
        qr_generate_via_api(ttl)
    }

    fn qr_code_status() -> Result<Option<serde_json::Value>, String> {
        qr_status_via_api()
    }

    fn qr_code_verify(token: String) -> Result<serde_json::Value, String> {
        qr_verify_via_api(&token)
    }

    fn qr_code_clear() -> Result<(), String> {
        qr_manager().clear();
        Ok(())
    }
}

// ==================== 票 11 互调 api 实现（命令面与 api 面共享核心） ====================
// 语义与 `invoke_command` 中 `devices.pairing.*` 命令完全一致（同一状态源），
// 仅返回形状对齐宿主 DTO（`PairingCode` serde 形状）与桥接需求。

/// 生成配对码（api 形状：宿主 `PairingCode` serde —— code / created_at / expires_in 剩余）
fn pair_code_generate_via_api(ttl: u64) -> Result<serde_json::Value, String> {
    let now = pairing::jwt::now_secs();
    let code = PairingCode::generate_with_ttl_at(ttl, now);
    *CURRENT_CODE.lock().map_err(|e| format!("pairing code lock: {e}"))? = Some(code.clone());
    serde_json::to_value(code).map_err(|e| format!("pairing code serialize: {e}"))
}

/// 当前配对码状态（过滤过期）→ null | 宿主 `PairingCode` 形状
fn pair_code_status_via_api() -> Result<Option<serde_json::Value>, String> {
    let now = pairing::jwt::now_secs();
    let guard = CURRENT_CODE.lock().map_err(|e| format!("pairing code lock: {e}"))?;
    match guard.as_ref().filter(|c| !c.is_expired_at(now)) {
        Some(code) => serde_json::to_value(code).map(Some).map_err(|e| format!("pairing code serialize: {e}")),
        None => Ok(None),
    }
}

/// 验证配对码（一次性：成功即消耗；过期顺带清除）→ valid 布尔
fn pair_code_verify_via_api(input: &str) -> Result<bool, String> {
    let mut guard = CURRENT_CODE.lock().map_err(|e| format!("pairing code lock: {e}"))?;
    let now = pairing::jwt::now_secs();
    let valid = match guard.as_ref() {
        Some(code) => {
            let ok = code.verify_at(input, now);
            if ok {
                *guard = None; // 成功即消耗（宿主 verify_and_consume 语义）
            } else if code.is_expired_at(now) {
                *guard = None; // 过期顺带清除（宿主语义）
            }
            ok
        }
        None => false,
    };
    Ok(valid)
}

/// 生成 QR token（api 形状 `{token, ttl, remaining}`；与命令面一致）
fn qr_generate_via_api(ttl: u64) -> Result<serde_json::Value, String> {
    let manager = qr_manager();
    let token = manager.generate(ttl);
    let (_, ttl, remaining) = manager.get_active().expect("fresh token active");
    Ok(serde_json::json!({ "token": token, "ttl": ttl, "remaining": remaining }))
}

/// 当前 QR token 状态（过滤过期/已用）→ null | `{token, ttl, remaining}`
fn qr_status_via_api() -> Result<Option<serde_json::Value>, String> {
    let active = qr_manager().get_active();
    let info = active.map(|(token, ttl, remaining)| {
        serde_json::json!({ "token": token, "ttl": ttl, "remaining": remaining })
    });
    Ok(info)
}

/// 验证 QR token（一次性：成功即清除）→ `{valid, reason?}`
///
/// reason 与宿主 `QrTokenManager::verify` 错误文本同构（`QR token expired` /
/// `QR token already used` / `No active QR token`），宿主端错误分类依赖
/// `contains("expired")` 等子串匹配（auth_controller qr_connect）。
fn qr_verify_via_api(input: &str) -> Result<serde_json::Value, String> {
    match qr_manager().verify(input) {
        Ok(()) => Ok(serde_json::json!({ "valid": true })),
        Err(e) => Ok(serde_json::json!({ "valid": false, "reason": e.message() })),
    }
}

impl WasmPlugin for DevicesPlugin {
    const ID: &'static str = "com.bedcode.devices";

    fn manifest() -> PluginManifest {
        // ADR-0005 单一真源：plugin.json（与 #[plugin_api] 防漂移比对同一份）
        serde_json::from_str(include_str!("../../plugin.json"))
            .expect("plugin.json must be valid PluginManifest")
    }

    fn activate() -> anyhow::Result<()> {
        let host = WasmHost;
        host.log_info("Device Auth Center plugin activated");
        // 订阅互调请求 topic（宏生成）：`bedcode.api.<api>` 逐个订阅，
        // 宿主订阅去重幂等
        DevicesApiDispatcher::register()?;
        // 密钥托管探活（票 07：密钥经 host-auth secret-store；明文不落日志，
        // 只记存在性与长度）。失败不阻断激活（密钥可延后按需生成；设备认证
        // 路径显性报错而非静默降级）
        match pairing::keys::jwt_key_from_host_auth() {
            Ok(key) => host.log_info(&format!(
                "jwt key ready via host-auth secret-store (len {})",
                key.len()
            )),
            Err(e) => host.log_warn(&format!("jwt key unavailable at activate: {}", e)),
        }
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        WasmHost.log_info("Device Auth Center plugin deactivated");
        Ok(())
    }

    /// 总线消息入口：互调请求先经宏生成的分派器（命中 api topic 则处理并回复），
    /// 其余消息保持原语义（本插件无其他订阅，直接忽略）
    fn on_message(msg: &BusMessage) -> anyhow::Result<()> {
        DevicesApiDispatcher::dispatch::<Self>(msg)?;
        Ok(())
    }

    fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        match name {
            // 骨架状态：宿主命令面桥接（票 11）读取认证中心状态的前置探活；
            // manifest 透传供宿主测试断言 api/permissions 声明生效
            "devices.status" => {
                let manifest = Self::manifest();
                Ok(serde_json::json!({
                    "plugin": Self::ID,
                    "version": env!("CARGO_PKG_VERSION"),
                    "skeleton": true,
                    "permissions": manifest.permissions,
                    "api": manifest.api,
                }))
            }

            // ==================== pairing 命令面（票 07） ====================
            // 消费：宿主配对 UI 经命令面桥接（票 11）；互调 api 面归票 09。

            // 生成配对码（替换旧的）→ {code, expires_in(剩余), ttl}
            "devices.pairing.code.generate" => {
                let ttl = args.get("ttl").and_then(|v| v.as_u64()).unwrap_or(pairing::code::PAIRING_CODE_TTL_SECS);
                let now = pairing::jwt::now_secs();
                let code = PairingCode::generate_with_ttl_at(ttl, now);
                *CURRENT_CODE.lock().expect("pairing code lock") = Some(code.clone());
                Ok(serde_json::json!({
                    "code": code.code,
                    "ttl": ttl,
                    "expires_in": code.remaining_secs_at(now),
                }))
            }

            // 验证配对码（一次性：成功即消耗）→ {valid: bool}
            "devices.pairing.code.verify" => {
                let input = args.get("code").and_then(|v| v.as_str()).unwrap_or("");
                let mut guard = CURRENT_CODE.lock().expect("pairing code lock");
                let now = pairing::jwt::now_secs();
                let valid = match guard.as_ref() {
                    Some(code) => {
                        let ok = code.verify_at(input, now);
                        if ok {
                            *guard = None; // 成功即消耗（宿主 verify_and_consume 语义）
                        } else if code.is_expired_at(now) {
                            *guard = None; // 过期顺带清除（宿主语义）
                        }
                        ok
                    }
                    None => false,
                };
                Ok(serde_json::json!({ "valid": valid }))
            }

            // 生成 QR token（替换旧的）→ {token, ttl, remaining}
            "devices.pairing.qr.generate" => {
                let ttl = args.get("ttl").and_then(|v| v.as_u64()).unwrap_or(300);
                let manager = qr_manager();
                let token = manager.generate(ttl);
                let (_, ttl, remaining) = manager.get_active().expect("fresh token active");
                Ok(serde_json::json!({ "token": token, "ttl": ttl, "remaining": remaining }))
            }

            // 验证 QR token（一次性：成功即清除）→ {valid: bool} 或错误
            "devices.pairing.qr.verify" => {
                let input = args.get("token").and_then(|v| v.as_str()).unwrap_or("");
                match qr_manager().verify(input) {
                    Ok(()) => Ok(serde_json::json!({ "valid": true })),
                    Err(e) => Err(anyhow::anyhow!("{}", e.message())),
                }
            }

            // 配对状态 → {qr: {token,ttl,remaining}|null, code: {code,expires_in}|null}
            "devices.pairing.status" => {
                let now = pairing::jwt::now_secs();
                let code = CURRENT_CODE
                    .lock()
                    .expect("pairing code lock")
                    .as_ref()
                    .filter(|c| !c.is_expired_at(now))
                    .map(|c| serde_json::json!({ "code": c.code, "expires_in": c.remaining_secs_at(now) }));
                let qr = qr_manager().get_active().map(|(token, ttl, remaining)| {
                    serde_json::json!({ "token": token, "ttl": ttl, "remaining": remaining })
                });
                Ok(serde_json::json!({ "qr": qr, "code": code }))
            }

            // JWT 签发（密钥经 host-auth secret-store）→ {token, exp}
            "devices.pairing.jwt.generate" => {
                let subject = args
                    .get("device_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("device_id required"))?;
                let device_name = args.get("device_name").and_then(|v| v.as_str()).map(String::from);
                let fingerprint = args.get("fingerprint").and_then(|v| v.as_str()).map(String::from);
                let key = pairing::keys::jwt_key_from_host_auth().map_err(anyhow::Error::msg)?;
                let service = JwtService::with_key(key);
                let token = service
                    .generate_token(subject.to_string(), device_name, fingerprint)
                    .map_err(|e| anyhow::anyhow!("jwt generate: {}", e))?;
                let claims = service
                    .verify_token(&token)
                    .map_err(|e| anyhow::anyhow!("jwt verify: {}", e))?;
                Ok(serde_json::json!({ "token": token, "exp": claims.exp }))
            }

            // JWT 验签（密钥经 host-auth secret-store）→ {valid, claims} 或错误
            "devices.pairing.jwt.verify" => {
                let token = args
                    .get("token")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("token required"))?;
                let key = pairing::keys::jwt_key_from_host_auth().map_err(anyhow::Error::msg)?;
                let service = JwtService::with_key(key);
                let claims = service
                    .verify_token_with_expiry(token)
                    .map_err(|e| anyhow::anyhow!("{}", pairing::jwt::jwt_error_message(&e)))?;
                Ok(serde_json::json!({
                    "valid": true,
                    "claims": {
                        "sub": claims.sub,
                        "iss": claims.iss,
                        "iat": claims.iat,
                        "exp": claims.exp,
                        "device_name": claims.device_name,
                        "fingerprint": claims.fingerprint,
                    }
                }))
            }

            // 密钥托管状态（明文不出插件：只报存在性与长度）
            "devices.pairing.key.status" => {
                match pairing::keys::jwt_key_from_host_auth() {
                    Ok(key) => Ok(serde_json::json!({ "present": true, "key_len": key.len() })),
                    Err(e) => Ok(serde_json::json!({ "present": false, "error": e })),
                }
            }

            // ==================== trust 命令面（票 08） ====================
            // 消费：设置面设备视图经命令面桥接（票 11）；互调 api 面归票 09。
            // 语义对照宿主：pairings 表（active 过滤 / paired_at DESC / 软删）
            // + peer trust_store（list-trusted / revoke-trusted 原语直通）。

            // 统一信任视图 → {devices: TrustedDeviceDto[], peerError: string|null}
            "devices.trust.list" => trust::list_via_host().map_err(anyhow::Error::msg),

            // 撤销统一条目（pairing 软删 / peer revoke-trusted）→ {removed, kind}
            "devices.trust.revoke" => {
                let id = args
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("id required"))?;
                trust::revoke_via_host(id).map_err(anyhow::Error::msg)
            }

            // 新增配对记录（配对完成流写入入口）→ {id}
            "devices.trust.add-pairing" => {
                let device_name = args
                    .get("deviceName")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("deviceName required"))?;
                let fingerprint = args
                    .get("deviceFingerprint")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("deviceFingerprint required"))?;
                let address = args.get("address").and_then(|v| v.as_str());
                let id = trust::add_pairing_via_host(device_name, fingerprint, address)
                    .map_err(|e| anyhow::anyhow!("trust add-pairing: {}", e))?;
                Ok(serde_json::json!({ "id": id }))
            }

            _ => Err(anyhow::anyhow!("Unknown command: {}", name)),
        }
    }
}

/// 获取/初始化 QR token 管理器（OnceLock static，跨命令调用）
fn qr_manager() -> &'static QrTokenManager {
    QR_MANAGER.get_or_init(QrTokenManager::new)
}

// ==================== Tests（票 07/11 命令面语义：native 直测） ====================
// 互调 api 与命令面共享同一状态源；native 下无宿主依赖（配对码/QR 仅用
// 静态状态 + getrandom/wasi clocks 的 wasm 适配，native 走 std）。

#[cfg(test)]
mod tests {
    use super::*;

    /// 清理共享静态状态（配对码/QR token；避免测试间串扰）
    fn reset_pairing_state() {
        *CURRENT_CODE.lock().expect("lock") = None;
        qr_manager().clear();
    }

    /// 生成 → 宿主 DTO 形状（code/created_at/expires_in 三字段，expires_in=剩余）
    #[test]
    fn pairing_generate_returns_host_dto_shape() {
        reset_pairing_state();
        let v = pair_code_generate_via_api(300).expect("generate");
        assert_eq!(v["code"].as_str().expect("code").len(), 6);
        assert!(v["code"].as_str().unwrap().chars().all(|c| c.is_ascii_digit()));
        // created_at RFC3339 UTC（宿主 chrono 可解析）
        assert!(
            pairing::code::parse_rfc3339_utc(v["created_at"].as_str().expect("created_at")).is_some(),
            "created_at 必须 RFC3339: {}",
            v["created_at"]
        );
        let expires = v["expires_in"].as_u64().expect("expires_in");
        assert!(expires <= 300, "expires_in 为剩余秒，必须 <= ttl");
    }

    /// 生成后 status 返回同码；验证成功后一次性消耗（复用失败）
    #[test]
    fn pairing_verify_is_single_use_via_api() {
        reset_pairing_state();
        let v = pair_code_generate_via_api(300).expect("generate");
        let code = v["code"].as_str().expect("code").to_string();

        let status = pair_code_status_via_api().expect("status");
        assert!(status.is_some());
        assert_eq!(status.unwrap()["code"], code.as_str());

        assert!(pair_code_verify_via_api(&code).expect("verify"));
        assert!(!pair_code_verify_via_api(&code).expect("reuse"), "一次性：二次验证失败");
        assert!(pair_code_status_via_api().expect("status after").is_none(), "消耗后无当前码");
    }

    /// 验证未知码 / 清除后验证：均 false
    #[test]
    fn pairing_verify_unknown_and_after_clear() {
        reset_pairing_state();
        pair_code_generate_via_api(300).expect("generate");
        assert!(!pair_code_verify_via_api("000000").expect("wrong"), "错误码拒绝");

        DevicesPlugin::pairing_code_clear().expect("clear");
        assert!(!pair_code_verify_via_api("anything").expect("after clear"), "清除后验证失败");
        assert!(pair_code_status_via_api().expect("status").is_none());
    }

    /// QR：生成 → 形状 → 状态 → 验证消耗（一次性）→ 清除
    #[test]
    fn qr_generate_verify_single_use_and_clear() {
        reset_pairing_state();
        let v = qr_generate_via_api(300).expect("generate");
        let token = v["token"].as_str().expect("token").to_string();
        assert_eq!(token.len(), 32, "QR token 32 hex 字符");
        assert!(v["remaining"].as_u64().expect("remaining") <= 300);

        let status = qr_status_via_api().expect("status");
        assert_eq!(status.as_ref().unwrap()["token"], token.as_str());

        let ok = qr_verify_via_api(&token).expect("verify");
        assert_eq!(ok["valid"], true);
        // 一次性：二次验证 → valid=false + reason 可分类（宿主 contains("expired") 等）
        let again = qr_verify_via_api(&token).expect("reuse");
        assert_eq!(again["valid"], false);
        assert!(again["reason"].as_str().is_some(), "拒绝必须带 reason");

        // 未生成时验证：No active QR token 分类
        DevicesPlugin::qr_code_clear().expect("clear");
        let none = qr_verify_via_api("ghost").expect("no active");
        assert_eq!(none["valid"], false);
        assert_eq!(none["reason"], "No active QR token");
        assert!(qr_status_via_api().expect("status after").is_none());
    }
}

bedcode_plugin_api::wasm_entry!(DevicesPlugin);
