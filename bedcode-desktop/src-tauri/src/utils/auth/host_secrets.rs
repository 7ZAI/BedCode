//! 宿主侧密钥托管（secret-store 宿主域）
//!
//! 与插件 secret-store（`plugin/manager/wasm_runtime/host_impl/auth.rs`，属主
//! 隔离 + 权限门）同表（`plugin_secrets`）不同域：宿主自身的基础设施密钥
//! （JWT 密钥等）以保留属主 `host` 存储，读写走本模块——宿主是最终仲裁者，
//! 不经插件权限门；插件侧 `WHERE plugin_id = ?1` 过滤天然读不到 `host` 行，
//! 属主隔离在 SQL 层成立（第二道闸）。
//!
//! 生命周期：`lib.rs` setup 中主库就绪后调用 [`init`] 注入句柄并预生成密钥；
//! 首启随机生成 + 持久化，重启后密钥稳定。未 init 的环境（单测）下
//! [`get_or_generate`] 返回未初始化错误，调用方（如 `jwt.rs`）回退进程内
//! 随机密钥。
//!
//! 明文不落日志红线（AGENTS.md §8）：本模块日志只记 `key` 与 `value_len`，
//! 禁止打印值本身；错误消息不含值内容。

use crate::db::Database;
use rand::rngs::OsRng;
use rand::RngCore;
use rusqlite::OptionalExtension;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

/// 宿主密钥保留属主（插件命名空间隔离；插件经 host-auth 只能触达自身 plugin_id 行）
pub(crate) const HOST_SECRET_OWNER: &str = "host";

/// 存储实现（可独立构造供单测注入内存/文件库）
pub(crate) struct HostSecretStore {
    db: Arc<tokio::sync::Mutex<Database>>,
    /// read-through 缓存：key → 明文（宿主内存边界；仅本进程可见）
    cache: RwLock<HashMap<String, Vec<u8>>>,
}

impl HostSecretStore {
    pub(crate) fn new(db: Arc<tokio::sync::Mutex<Database>>) -> Self {
        Self {
            db,
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// 读密钥；不存在则随机生成 `len` 字节并持久化（首启）。幂等：已存在
    /// 时返回既有值，不重置。
    pub(crate) fn get_or_generate(&self, key: &str, len: usize) -> Result<Vec<u8>, String> {
        // 缓存读（热路径：JwtService 每请求构造，命中后零 DB 访问）
        if let Some(v) = self
            .cache
            .read()
            .map_err(|e| format!("secret cache poisoned: {}", e))?
            .get(key)
        {
            return Ok(v.clone());
        }

        // 主库读（read-through）
        let existing = self.read_value(key)?;
        let value: Vec<u8> = match existing {
            Some(hexed) => hex::decode(&hexed).map_err(|e| format!("stored secret decode error: {}", e))?,
            None => {
                let mut buf = vec![0u8; len];
                OsRng.fill_bytes(&mut buf);
                let hexed = hex::encode(&buf);
                tracing::info!(
                    key = %key,
                    value_len = buf.len(),
                    "host_secret_store: new secret generated and persisted (length only)"
                );
                self.upsert(key, &hexed)?;
                buf
            }
        };

        self.cache
            .write()
            .map_err(|e| format!("secret cache poisoned: {}", e))?
            .insert(key.to_string(), value.clone());
        Ok(value)
    }

    /// 主库读取（属主固定为 `host`）
    fn read_value(&self, key: &str) -> Result<Option<String>, String> {
        let db = self.db.clone();
        let owner = HOST_SECRET_OWNER.to_string();
        let k = key.to_string();
        crate::plugin::manager::wasm_runtime::block_on_async(async move {
            let db = db.lock().await;
            db.conn()
                .query_row(
                    "SELECT value FROM plugin_secrets WHERE plugin_id = ?1 AND key = ?2",
                    rusqlite::params![owner, k],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|e| format!("host secret read failed: {}", e))
        })
    }

    /// 主库写入（UPSERT；幂等覆盖）
    fn upsert(&self, key: &str, hexed: &str) -> Result<(), String> {
        let db = self.db.clone();
        let owner = HOST_SECRET_OWNER.to_string();
        let k = key.to_string();
        let v = hexed.to_string();
        let now = chrono::Utc::now().to_rfc3339();
        crate::plugin::manager::wasm_runtime::block_on_async(async move {
            let db = db.lock().await;
            db.conn()
                .execute(
                    "INSERT INTO plugin_secrets (plugin_id, key, value, updated_at) \
                     VALUES (?1, ?2, ?3, ?4) \
                     ON CONFLICT(plugin_id, key) DO UPDATE SET value = ?3, updated_at = ?4",
                    rusqlite::params![owner, k, v, now],
                )
                .map_err(|e| format!("host secret write failed: {}", e))?;
            Ok::<(), String>(())
        })
    }
}

/// 进程级单例（lib.rs setup 注入）
static STORE: OnceLock<HostSecretStore> = OnceLock::new();

/// 注入主库句柄（lib.rs setup 调用一次；重复调用幂等忽略）
pub(crate) fn init(db: Arc<tokio::sync::Mutex<Database>>) {
    let _ = STORE.set(HostSecretStore::new(db));
}

/// 读或首启生成密钥（见 [`HostSecretStore::get_or_generate`]）
pub(crate) fn get_or_generate(key: &str, len: usize) -> Result<Vec<u8>, String> {
    STORE
        .get()
        .ok_or_else(|| "host secret store not initialized".to_string())?
        .get_or_generate(key, len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn mem_store() -> HostSecretStore {
        let db = crate::db::Database::new(std::path::Path::new(":memory:")).expect("open in-memory db");
        db.init_schema().expect("init schema");
        HostSecretStore::new(Arc::new(tokio::sync::Mutex::new(db)))
    }

    /// 文件后备库（重启持久化测试用；每个用例独立临时目录）
    fn file_store(dir: &std::path::Path) -> HostSecretStore {
        let db = crate::db::Database::new(&dir.join("bedcode.db")).expect("open file db");
        db.init_schema().expect("init schema");
        HostSecretStore::new(Arc::new(tokio::sync::Mutex::new(db)))
    }

    /// 直读主库行（绕过缓存验证真源落库）
    fn raw_row(db_path: &std::path::Path, key: &str) -> Option<String> {
        let db = crate::db::Database::new(db_path).expect("open file db");
        db.conn()
            .query_row(
                "SELECT value FROM plugin_secrets WHERE plugin_id = ?1 AND key = ?2",
                rusqlite::params![HOST_SECRET_OWNER, key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .expect("query row")
    }

    #[test]
    fn first_launch_generates_and_persists_under_host_owner() {
        let store = mem_store();
        let v = store.get_or_generate("jwt.key", 32).unwrap();
        assert_eq!(v.len(), 32);
        // 非零（随机生成器工作）
        assert!(v.iter().any(|&b| b != 0));
        // 二次调用幂等（同一实例缓存 + 主库均命中既有值，不重置）
        assert_eq!(store.get_or_generate("jwt.key", 32).unwrap(), v);
        // 请求更长长度也不重置既有密钥（密钥稳定性优先于长度调整）
        assert_eq!(store.get_or_generate("jwt.key", 64).unwrap(), v);
    }

    #[test]
    fn secret_stable_across_restart() {
        let dir = std::env::temp_dir().join(format!("host-secrets-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("bedcode.db");

        // 第一代：生成并持久化
        let first = file_store(&dir).get_or_generate("jwt.key", 32).unwrap();

        // 真源落库（属主 = host）
        let stored = raw_row(&db_path, "jwt.key").expect("row persisted");
        assert_eq!(hex::decode(&stored).unwrap(), first);

        // 第二代（全新实例 = 重启）：读回同一密钥
        let second = file_store(&dir).get_or_generate("jwt.key", 32).unwrap();
        assert_eq!(second, first);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn not_initialized_returns_error() {
        // 静态 STORE 仅由 lib.rs setup 的 init() 注入；cargo test 不跑 setup，
        // 故进程内未初始化，自由函数应返回明确错误（调用方据此回退）。
        let err = super::get_or_generate("jwt.key", 32).unwrap_err();
        assert!(err.contains("not initialized"));
    }
}
