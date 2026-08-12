//! 生物认证密钥插件（BiometricKeyPlugin，P-256 + Keystore）
//!
//! 从 android_plugins.rs 拆分。

use std::sync::OnceLock;
use tauri::plugin::{Builder, PluginHandle};

/// 已注册的 BiometricKeyPlugin 句柄（仅 Android 平台使用）
static BIOMETRIC_KEY_HANDLE: OnceLock<PluginHandle<tauri::Wry>> = OnceLock::new();


/// 注册 BiometricKeyPlugin（生物认证密钥：Android Keystore 生成/签名/删除）
pub fn biometric_key_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    Builder::new("biometric-key")
        .setup(|_app, api| {
            #[cfg(target_os = "android")]
            {
                let handle = api.register_android_plugin("com.bedcode.mobile", "BiometricKeyPlugin")?;
                let _ = BIOMETRIC_KEY_HANDLE.set(handle);
            }
            #[cfg(not(target_os = "android"))] // 非 Android 平台消除 unused 警告
            let _ = api;
            Ok(())
        })
        .build()
}


/// 生成生物认证密钥对（P-256，私钥存 Keystore 且需生物认证解锁）
///
/// 返回公钥（SPKI X.509 DER，base64）
#[cfg(target_os = "android")]
pub async fn biometric_generate_keypair(fingerprint: &str) -> crate::Result<String> {
    let handle = BIOMETRIC_KEY_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("BiometricKeyPlugin not registered".to_string())
    })?;
    let payload = serde_json::json!({ "alias": biometric_alias(fingerprint) });
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("generateKeyPair", payload)
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to generate biometric key: {}", e)))?;
    // Kotlin 端失败时透传具体原因（如 Keystore 异常）
    if response.get("success").and_then(|v| v.as_bool()).unwrap_or(true) == false {
        let reason = response
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown key generation error");
        return Err(crate::AppError::Plugin(format!("Biometric key generation failed: {}", reason)));
    }
    response.get("publicKey").and_then(|v| v.as_str()).map(String::from)
        .ok_or_else(|| crate::AppError::Plugin("Missing publicKey in biometric response".to_string()))
}


/// 生物认证签名：弹系统生物识别，认证通过后对消息签名
///
/// 返回原始 r||s 格式签名（base64）
#[cfg(target_os = "android")]
pub async fn biometric_sign(fingerprint: &str, message_hex: &str) -> crate::Result<String> {
    let handle = BIOMETRIC_KEY_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("BiometricKeyPlugin not registered".to_string())
    })?;
    let payload = serde_json::json!({ "alias": biometric_alias(fingerprint), "message": message_hex });
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("sign", payload)
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to sign with biometric key: {}", e)))?;
    // Kotlin 端失败（用户取消 / 认证失败 / 异常）时透传具体原因（系统文案，已是用户语言），
    // 不再加英文包装前缀，避免 toast 出现 "Plugin error: Biometric sign failed: ..." 中英混杂
    if response.get("success").and_then(|v| v.as_bool()).unwrap_or(true) == false {
        let reason = response
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown biometric error");
        return Err(crate::AppError::Plugin(reason.to_string()));
    }
    response.get("signature").and_then(|v| v.as_str()).map(String::from)
        .ok_or_else(|| crate::AppError::Plugin("Missing signature in biometric response".to_string()))
}


/// 删除生物认证密钥（解绑时调用）
#[cfg(target_os = "android")]
pub async fn biometric_delete_key(fingerprint: &str) -> crate::Result<()> {
    let handle = BIOMETRIC_KEY_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("BiometricKeyPlugin not registered".to_string())
    })?;
    let payload = serde_json::json!({ "alias": biometric_alias(fingerprint) });
    handle
        .run_mobile_plugin_async::<()>("deleteKey", payload)
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to delete biometric key: {}", e)))?;
    Ok(())
}


/// 检查生物认证密钥是否已存在
#[cfg(target_os = "android")]
pub async fn biometric_has_key(fingerprint: &str) -> crate::Result<bool> {
    let handle = BIOMETRIC_KEY_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("BiometricKeyPlugin not registered".to_string())
    })?;
    let payload = serde_json::json!({ "alias": biometric_alias(fingerprint) });
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("hasKey", payload)
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to check biometric key: {}", e)))?;
    Ok(response.get("hasKey").and_then(|v| v.as_bool()).unwrap_or(false))
}


/// 检查设备是否支持生物认证密钥（硬件 + 已录入生物特征）
///
/// 返回 (是否支持, BiometricManager 结果码)：原因码供 UI 展示具体不支持原因。
#[cfg(target_os = "android")]
pub async fn biometric_device_supported() -> crate::Result<(bool, i32)> {
    let handle = BIOMETRIC_KEY_HANDLE.get().ok_or_else(|| {
        crate::AppError::Plugin("BiometricKeyPlugin not registered".to_string())
    })?;
    let response: serde_json::Value = handle
        .run_mobile_plugin_async("isDeviceSupported", serde_json::json!({}))
        .await
        .map_err(|e| crate::AppError::Plugin(format!("Failed to check biometric support: {}", e)))?;
    let supported = response
        .get("supported")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let reason = response
        .get("reason")
        .and_then(|v| v.as_i64())
        .unwrap_or(-1) as i32;
    Ok((supported, reason))
}


/// 生成 Keystore 别名（指纹哈希，避免非法字符并保证长度稳定）
fn biometric_alias(fingerprint: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(fingerprint.as_bytes());
    format!("bedcode_biometric_{}", hex::encode(hash))
}


/// 非 Android 平台生物认证不可用（桌面 dev 环境）
#[cfg(not(target_os = "android"))]
pub async fn biometric_generate_keypair(_fingerprint: &str) -> crate::Result<String> {
    Err(crate::AppError::Plugin("Biometric key unavailable on this platform".to_string()))
}


#[cfg(not(target_os = "android"))]
pub async fn biometric_sign(_fingerprint: &str, _message_hex: &str) -> crate::Result<String> {
    Err(crate::AppError::Plugin("Biometric key unavailable on this platform".to_string()))
}


#[cfg(not(target_os = "android"))]
pub async fn biometric_delete_key(_fingerprint: &str) -> crate::Result<()> {
    Err(crate::AppError::Plugin("Biometric key unavailable on this platform".to_string()))
}


#[cfg(not(target_os = "android"))]
pub async fn biometric_has_key(_fingerprint: &str) -> crate::Result<bool> {
    Ok(false)
}


#[cfg(not(target_os = "android"))]
pub async fn biometric_device_supported() -> crate::Result<(bool, i32)> {
    Ok((false, -1))
}
