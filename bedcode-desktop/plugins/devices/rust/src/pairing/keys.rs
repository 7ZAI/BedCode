//! 密钥托管（认证中心 pairing 模块 · 票 07）
//!
//! 密码学密钥经 host-auth secret-store（WIT v15）获取：首启随机生成
//! （32 字节 HS256 最小安全长度）+ 持久化到插件属主命名空间，明文不出宿主
//! （宿主日志只记长度，票 04 已实现）。
//!
//! 抽象 `SecretStore` 隔离宿主依赖：wasm 运行时（命令面/激活路径）走
//! `WasmHost`（host-auth import）；native 单测注入内存 mock，核心逻辑
//! （get-or-create / hex 解析）在无宿主环境可测。`WasmHost` 的 impl 用
//! `cfg(target_arch = "wasm32")` 限定，native 链接不引用 host-auth import 符号。

use crate::pairing::jwt::{JWT_SECRET_KEY_ID, JWT_SECRET_KEY_LEN};

#[cfg(test)]
use std::sync::Mutex;

/// 密钥存储抽象（插件属主隔离由宿主保证；mock 供 native 单测）
pub trait SecretStore {
    /// 读取属主密钥；键不存在返回 `Ok(None)`
    fn get(&self, key: &str) -> Result<Option<String>, String>;
    /// 写入/覆盖属主密钥
    fn set(&self, key: &str, value: &str) -> Result<(), String>;
}

/// 认证中心密钥获取策略：get-or-create（首启随机生成 + 持久化）
///
/// - 密钥以 hex 字符串落库（host-auth 值为 String）；读取后解码为字节
/// - 日志只记长度，明文不落宿主日志（spec §3 红线；宿主侧日志同样只记长度）
/// - 失败（存储不可用）返回错误，调用方决定降级策略（命令面报错；插件不静默
///   生成进程内密钥 —— 重启即失效的 token 会破坏已配对设备，宁显性失败）
pub fn get_or_create_jwt_key(store: &impl SecretStore) -> Result<Vec<u8>, String> {
    if let Some(existing) = store.get(JWT_SECRET_KEY_ID)? {
        let key = hex::decode(existing)
            .map_err(|e| format!("stored jwt key not hex: {}", e))?;
        if key.len() != JWT_SECRET_KEY_LEN {
            return Err(format!(
                "stored jwt key length mismatch: expected {}, got {}",
                JWT_SECRET_KEY_LEN,
                key.len()
            ));
        }
        Ok(key)
    } else {
        // 首启：OsRng 等价熵（getrandom，wasip3 → wasi:random）+ hex 落库
        let mut key = vec![0u8; JWT_SECRET_KEY_LEN];
        getrandom::fill(&mut key).map_err(|e| format!("entropy unavailable: {}", e))?;
        store.set(JWT_SECRET_KEY_ID, &hex::encode(&key))?;
        Ok(key)
    }
}

/// wasm 运行时路径：host-auth secret-store（`WasmHost`）
#[cfg(target_arch = "wasm32")]
pub fn jwt_key_from_host_auth() -> Result<Vec<u8>, String> {
    get_or_create_jwt_key(&WasmKeyStore)
}

/// wasm 运行时：`SecretStore` for `WasmHost`（WIT host-auth 四函数）
#[cfg(target_arch = "wasm32")]
struct WasmKeyStore;

#[cfg(target_arch = "wasm32")]
impl SecretStore for WasmKeyStore {
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        // auth_secret_get：权限门（manifest 已声明 auth）+ 插件属主隔离由宿主仲裁
        use bedcode_plugin_api::host::HostAuth;
        bedcode_plugin_api::wasm_host::WasmHost
            .auth_secret_get(key)
            .map_err(|e| e.message)
    }

    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        use bedcode_plugin_api::host::HostAuth;
        bedcode_plugin_api::wasm_host::WasmHost
            .auth_secret_set(key, value)
            .map_err(|e| e.message)
    }
}

/// native（cargo test）路径：密钥依赖宿主，无 host-auth —— 显性失败；
/// 单测经 `MockSecretStore` 注入
#[cfg(not(target_arch = "wasm32"))]
pub fn jwt_key_from_host_auth() -> Result<Vec<u8>, String> {
    Err("host-auth secret-store unavailable outside wasm runtime".to_string())
}

/// 内存 mock 密钥存储（native 单测用；宿主侧对照见宿主测试）
#[cfg(test)]
pub struct MockSecretStore {
    inner: Mutex<Vec<(String, String)>>,
}

#[cfg(test)]
impl MockSecretStore {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Vec::new()),
        }
    }
}

#[cfg(test)]
impl SecretStore for MockSecretStore {
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        let inner = self.inner.lock().unwrap();
        Ok(inner.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()))
    }

    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap();
        if let Some(entry) = inner.iter_mut().find(|(k, _)| k == key) {
            entry.1 = value.to_string();
        } else {
            inner.push((key.to_string(), value.to_string()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 首启：随机生成 + 落库（hex 长度 = 32B × 2 = 64 字符）
    #[test]
    fn first_call_generates_and_persists() {
        let store = MockSecretStore::new();
        let key = get_or_create_jwt_key(&store).expect("generate");
        assert_eq!(key.len(), JWT_SECRET_KEY_LEN);
        let stored = store.get(JWT_SECRET_KEY_ID).unwrap().expect("persisted");
        assert_eq!(stored.len(), 64, "32 字节 → 64 hex 字符落库");
        // 熵非全零（新密钥必须随机）
        assert!(key.iter().any(|b| *b != 0));
    }

    /// 二次调用：读回已持久化密钥（重启稳定，不重新生成）
    #[test]
    fn second_call_reads_back_persisted_key() {
        let store = MockSecretStore::new();
        let first = get_or_create_jwt_key(&store).expect("first");
        let second = get_or_create_jwt_key(&store).expect("second");
        assert_eq!(first, second, "重启后密钥必须稳定（读回持久化值）");
        // 覆盖写替换旧值（宿主 UPSERT 语义）
        store.set(JWT_SECRET_KEY_ID, &hex::encode(vec![0xabu8; JWT_SECRET_KEY_LEN])).unwrap();
        let third = get_or_create_jwt_key(&store).expect("third");
        assert_eq!(third, vec![0xabu8; JWT_SECRET_KEY_LEN], "覆盖写生效");
    }

    /// 损坏存储（非 hex / 长度不符）：显性失败，不静默降级
    #[test]
    fn corrupt_stored_key_fails_loudly() {
        let store = MockSecretStore::new();
        store.set(JWT_SECRET_KEY_ID, "not-hex!").unwrap();
        assert!(get_or_create_jwt_key(&store).is_err(), "非 hex 必须报错");

        let store2 = MockSecretStore::new();
        store2.set(JWT_SECRET_KEY_ID, &hex::encode([0u8; 16])).unwrap();
        let err = get_or_create_jwt_key(&store2).unwrap_err();
        assert!(err.contains("length mismatch"), "长度不符必须报错: {}", err);
    }

    /// native 路径显性失败（无宿主环境不得静默生成进程内密钥）
    #[test]
    fn native_path_fails_without_host() {
        assert!(jwt_key_from_host_auth().is_err());
    }
}
