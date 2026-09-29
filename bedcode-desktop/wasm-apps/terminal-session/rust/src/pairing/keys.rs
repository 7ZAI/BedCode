//! 入场签发密钥环（终端会话中心 pairing 域 · ADR 0033）
//!
//! **入场密钥的真源在本插件**（v33 / ADR 0033）：生成、签发、验签全部本地化，
//! 宿主不再持有任何设备 JWT 密码学。密钥材料仍经 `host-auth secret-store` 落库
//! （表 `plugin_secrets`，属主 = 本插件；宿主日志只记长度，明文不落日志）——
//! **托管位置没变，变的是「谁有资格用它」**：只有中心能读自己那行。
//!
//! ## 为什么是「keyring」而不是一把 key
//!
//! D1 之后入场密钥**明文躺在 guest 线性内存里**。于是泄露后果从「理论风险」变成
//! 「不可恢复风险」：既有 token 仍在 7 天窗口内有效，而 `utils/auth/` 下**今天没有任何
//! 轮换机制**。故本模块直接给出最小可用的轮换（ADR 0033 D4）：
//!
//! - 密钥环最多保留**两代**：`active`（当前签发用）+ 其**上一代**（只验签）。
//!   宽限期 = 最长 token TTL = 7 天（`DEFAULT_TOKEN_EXPIRY_SECS`）；超出宽限期的
//!   旧代在轮换时即被裁掉，不无限累积。
//! - `kid` = 代次标识，**可选**写进 JWT claims。旧 token（无 `kid`）走「先试当前、
//!   再试上一代」，不破坏存量 wire 格式。
//!
//! ## 为什么是「一个 secret 值」而不是「多个 key 行」
//!
//! 指针式存法（`jwt.kid` + `jwt.key.<gen>` 多个行）要 3+ 次宿主调用才能读出密钥环，
//! 且轮换存在**中间态窗口**（`kid` 已指向新代、密钥行还没写 → 读到的 keyring 不可用）。
//! 整环编码成**一个** secret 值后：读取 = 1 次调用、轮换 = 1 次 UPSERT（原子），
//! 不存在半写状态。
//!
//! 抽象 `SecretStore` 隔离宿主依赖：wasm 运行时（命令面/激活路径）走 `WasmHost`
//! （host-auth import）；native 单测注入内存 mock，核心逻辑（get-or-create /
//! 轮换 / hex 解析 / 代次裁剪）在无宿主环境可测。`WasmHost` 的 impl 用
//! `cfg(target_arch = "wasm32")` 限定，native 链接不引用 host-auth import 符号。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::pairing::jwt::{JWT_SECRET_KEY_LEN, JWT_SECRET_KEY_ID};

#[cfg(test)]
use std::sync::Mutex;

/// 密钥环在 host-auth secret-store 中的键名（属主 = 本插件）
pub const KEYRING_SECRET_ID: &str = "jwt.keyring";

/// 密钥环最多保留代次（`active` + 上一代；更老的在轮换时裁掉）
pub const MAX_GENERATIONS: usize = 2;

/// 密钥存储抽象（插件属主隔离由宿主保证；mock 供 native 单测）
pub trait SecretStore {
    /// 读取属主密钥；键不存在返回 `Ok(None)`
    fn get(&self, key: &str) -> Result<Option<String>, String>;
    /// 写入/覆盖属主密钥
    fn set(&self, key: &str, value: &str) -> Result<(), String>;
    /// 删除属主密钥（键不存在也视为成功）
    fn delete(&self, key: &str) -> Result<(), String>;
}

/// 落库形状：`{"active": <代次>, "keys": {"<代次>": "<hex>"}}`
///
/// 整环**一个** secret 值（见模块头「为什么是一个 secret 值」）。`keys` 只保留
/// `MAX_GENERATIONS` 项；代次单调递增，作为 JWT `kid` 的取值来源。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct KeyringFile {
    active: u64,
    keys: BTreeMap<u64, String>,
}

/// 内存态密钥环：代次 → 密钥字节 + 当前签发代次
#[derive(Debug, Clone)]
pub struct Keyring {
    active: u64,
    keys: BTreeMap<u64, Vec<u8>>,
}

impl Keyring {
    /// 当前签发代次标识（写进 claims 的 `kid`；`g<代次>`）
    pub fn active_kid(&self) -> String {
        format!("g{}", self.active)
    }

    /// 当前签发用密钥（**唯一**签发面）
    pub fn signing_key(&self) -> Result<&[u8], String> {
        self.keys
            .get(&self.active)
            .map(|k| k.as_slice())
            .ok_or_else(|| format!("keyring corrupt: active generation {} missing", self.active))
    }

    /// 验签候选密钥集：当前代优先、其后按代次倒序（每代至多一次尝试）
    ///
    /// 顺序 = 「先试 `kid` 指向的代」的默认退化路径：调用方拿不到 `kid` 时
    /// （旧 token 无该字段）也覆盖得到。
    pub fn verification_keys(&self) -> Vec<&[u8]> {
        let mut ordered: Vec<u64> = self.keys.keys().rev().copied().collect();
        if let Some(pos) = ordered.iter().position(|g| *g == self.active) {
            ordered.swap(0, pos);
        }
        ordered
            .into_iter()
            .filter_map(|g| self.keys.get(&g).map(|k| k.as_slice()))
            .collect()
    }

    /// 该 `kid` 是否指向环内某代（`None` = 旧 token 无 `kid`，一律放行到密钥尝试）
    pub fn knows_kid(&self, kid: Option<&str>) -> bool {
        match kid {
            None => true,
            Some(kid) => kid
                .strip_prefix('g')
                .and_then(|g| g.parse::<u64>().ok())
                .is_some_and(|g| self.keys.contains_key(&g)),
        }
    }

    /// 落库形状（内部：唯一写入口，`load_or_create` / `rotate` 共用）
    fn to_file(&self) -> String {
        let keys: BTreeMap<u64, String> = self
            .keys
            .iter()
            .map(|(g, k)| (*g, hex::encode(k)))
            .collect();
        serde_json::to_string(&KeyringFile {
            active: self.active,
            keys,
        })
        .expect("keyring serialize (BTreeMap<u64,String> infallible)")
    }
}

/// 从落库字符串解析密钥环（损坏 → 显性失败，**不**静默降级为新密钥）
///
/// 静默降级在这里是灾难：换一把「顺手生成的新密钥」会让全部已配对设备静默失效，
/// 而系统看起来一切正常（只是「连不上了」）。故解析失败一律 Err。
fn parse_keyring(raw: &str) -> Result<Keyring, String> {
    let file: KeyringFile =
        serde_json::from_str(raw).map_err(|e| format!("stored keyring not valid: {e}"))?;
    if file.active == 0 {
        return Err("stored keyring invalid: active generation must be >= 1".to_string());
    }
    let mut keys = BTreeMap::new();
    for (gen, hexed) in file.keys {
        let key = hex::decode(&hexed).map_err(|e| format!("stored keyring key not hex: {e}"))?;
        if key.len() != JWT_SECRET_KEY_LEN {
            return Err(format!(
                "stored keyring key length mismatch: expected {}, got {}",
                JWT_SECRET_KEY_LEN,
                key.len()
            ));
        }
        keys.insert(gen, key);
    }
    if !keys.contains_key(&file.active) {
        return Err(format!(
            "stored keyring invalid: active generation {} has no key",
            file.active
        ));
    }
    // 有界保留：只留当前代与最近的一代（更老的裁掉——它们的 token 早过宽限期）
    prune_to_max(&mut keys, file.active);
    Ok(Keyring {
        active: file.active,
        keys,
    })
}

/// 只保留 `active` 与其前一代（按代次序取最近的 `MAX_GENERATIONS` 个）
fn prune_to_max(keys: &mut BTreeMap<u64, Vec<u8>>, active: u64) {
    if keys.len() <= MAX_GENERATIONS {
        return;
    }
    // 按「距 active 的代数距离」升序保留，远的删除
    let mut by_distance: Vec<(u64, u64)> = keys.keys().map(|g| (active.abs_diff(*g), *g)).collect();
    by_distance.sort();
    for (_, gen) in by_distance.into_iter().skip(MAX_GENERATIONS) {
        keys.remove(&gen);
    }
}

/// 首启生成 + 之后读回（**每请求一次**调用，无 guest 侧缓存）
///
/// 无缓存是刻意的：单写者（只有中心自己写）+ 无缓存 = 不存在「宿主更新了密钥而 guest
/// 还在用旧值」这一类 bug。性能代价是每次读多一次宿主调用，而宿主侧 secret-store 是
/// 内存 read-through 缓存（`host_api/auth.rs::auth_secret_get`）——不触 DB。
/// ADR 0033 性能实测：crypto 只占每请求 ~6%，往返占 ~94%，多一次缓存读不影响量级。
pub fn load_or_create(store: &impl SecretStore) -> Result<Keyring, String> {
    match store.get(KEYRING_SECRET_ID)? {
        Some(raw) => parse_keyring(&raw),
        None => {
            let ring = fresh_keyring()?;
            store.set(KEYRING_SECRET_ID, &ring.to_file())?;
            Ok(ring)
        }
    }
}

/// 轮换：新生一代作为 `active`，原 `active` 降为上一代（仅验签），更老的裁掉
///
/// 幂等语义：重复触发**会**推进代次（这是「换钥」动作，不是查询），但任何时刻
/// 环内至多 `MAX_GENERATIONS` 代，且任何一步失败都不留下半写状态（单次 UPSERT）。
/// **不撤销既有 token**——撤销是撤销域的职责，轮换只换签发密钥。
pub fn rotate(store: &impl SecretStore) -> Result<Keyring, String> {
    let mut ring = load_or_create(store)?;
    let new_gen = ring.active + 1;
    let mut key = vec![0u8; JWT_SECRET_KEY_LEN];
    getrandom::fill(&mut key).map_err(|e| format!("entropy unavailable: {e}"))?;
    ring.keys.insert(new_gen, key);
    ring.active = new_gen;
    prune_to_max(&mut ring.keys, ring.active);
    store.set(KEYRING_SECRET_ID, &ring.to_file())?;
    Ok(ring)
}

/// 首启密钥环：代次 1，单密钥
fn fresh_keyring() -> Result<Keyring, String> {
    // OsRng 等价熵（getrandom，wasip3 → wasi:random）+ hex 落库
    let mut key = vec![0u8; JWT_SECRET_KEY_LEN];
    getrandom::fill(&mut key).map_err(|e| format!("entropy unavailable: {e}"))?;
    Ok(Keyring {
        active: 1,
        keys: BTreeMap::from([(1u64, key)]),
    })
}

/// 一次性清理 ADR 0033 之前的**死**密钥行（插件属主侧的 `jwt.key`）
///
/// 迁移前本插件就在写这行，但生产签发走宿主代签 ⇒ 它**零生产调用点**。D1 之后
/// 签发权归本插件，新真源是 `jwt.keyring`，故这行是不可达的旧密钥材料：删掉
/// （幂等：不存在也视为成功）。**不清**也能工作，但密钥材料在库里多躺一份是
/// 白给的面。
pub fn purge_legacy_key(store: &impl SecretStore) -> Result<bool, String> {
    let existed = store.get(JWT_SECRET_KEY_ID)?.is_some();
    store.delete(JWT_SECRET_KEY_ID)?;
    Ok(existed)
}

// ==================== wasm 运行时路径（host-auth secret-store） ====================

/// wasm 运行时：读回本插件的密钥环（首启生成），wasm 侧唯一入口
#[cfg(target_arch = "wasm32")]
pub fn keyring_from_host_auth() -> Result<Keyring, String> {
    load_or_create(&WasmKeyStore)
}

/// wasm 运行时：轮换签发密钥（保留上一代用于宽限期验签）
#[cfg(target_arch = "wasm32")]
pub fn rotate_from_host_auth() -> Result<Keyring, String> {
    rotate(&WasmKeyStore)
}

/// wasm 运行时：清理退役的 `jwt.key` 行（activate 期一次性，幂等）
#[cfg(target_arch = "wasm32")]
pub fn purge_legacy_key_from_host_auth() -> Result<bool, String> {
    purge_legacy_key(&WasmKeyStore)
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

    fn delete(&self, key: &str) -> Result<(), String> {
        use bedcode_plugin_api::host::HostAuth;
        bedcode_plugin_api::wasm_host::WasmHost
            .auth_secret_delete(key)
            .map_err(|e| e.message)
    }
}

// ==================== native（cargo test）路径 ====================

/// native（cargo test）路径：密钥依赖宿主，无 host-auth —— 显性失败；
/// 单测经 [`MockSecretStore`] 注入
#[cfg(not(target_arch = "wasm32"))]
pub fn keyring_from_host_auth() -> Result<Keyring, String> {
    Err("host-auth secret-store unavailable outside wasm runtime".to_string())
}

/// native 路径：轮换同样显性失败（无宿主即无密钥环可轮换）
#[cfg(not(target_arch = "wasm32"))]
pub fn rotate_from_host_auth() -> Result<Keyring, String> {
    Err("host-auth secret-store unavailable outside wasm runtime".to_string())
}

/// native 路径：清理同样显性失败
#[cfg(not(target_arch = "wasm32"))]
pub fn purge_legacy_key_from_host_auth() -> Result<bool, String> {
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

    fn delete(&self, key: &str) -> Result<(), String> {
        self.inner.lock().unwrap().retain(|(k, _)| k != key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== 首启 / 读回 ====================

    /// 首启：随机生成 + 落库（hex 长度 = 32B × 2 = 64 字符）
    #[test]
    fn first_call_generates_and_persists() {
        let store = MockSecretStore::new();
        let ring = load_or_create(&store).expect("generate");
        assert_eq!(ring.active, 1);
        assert_eq!(ring.signing_key().expect("signing key").len(), JWT_SECRET_KEY_LEN);
        let stored = store
            .get(KEYRING_SECRET_ID)
            .unwrap()
            .expect("persisted");
        // 落库的是**一个** secret 值（整环），不是多行指针
        assert!(
            store.get(JWT_SECRET_KEY_ID).unwrap().is_none(),
            "新真源是 keyring，不得再写退役的 jwt.key 行"
        );
        let file: KeyringFile = serde_json::from_str(&stored).expect("keyring json");
        assert_eq!(file.active, 1);
        let hexed = file.keys.get(&1).expect("gen 1 key");
        assert_eq!(hexed.len(), 64, "32 字节 → 64 hex 字符落库");
        // 熵非全零（新密钥必须随机）
        assert!(ring.signing_key().expect("key").iter().any(|b| *b != 0));
    }

    /// 二次调用：读回已持久化密钥环（重启稳定，不重新生成）
    #[test]
    fn second_call_reads_back_persisted_keyring() {
        let store = MockSecretStore::new();
        let first = load_or_create(&store).expect("first");
        let second = load_or_create(&store).expect("second");
        assert_eq!(first.active, second.active, "重启后代次稳定");
        assert_eq!(
            first.signing_key().expect("key"),
            second.signing_key().expect("key"),
            "重启后签发密钥必须稳定（读回持久化值）"
        );
    }

    /// 损坏存储（非法 JSON / 非 hex / 长度不符 / active 缺密钥）→ **显性失败**，
    /// 绝不静默生成新密钥（那会让全部已配对设备静默失效）
    #[test]
    fn corrupt_keyring_fails_loudly_instead_of_regenerating() {
        let store = MockSecretStore::new();
        let good = load_or_create(&store).expect("first");

        let cases: Vec<(String, &str)> = vec![
            ("not-json!".to_string(), "not valid"),
            (
                serde_json::json!({"active": 1, "keys": {"1": "zz"}}).to_string(),
                "not hex",
            ),
            (
                serde_json::json!({"active": 1, "keys": {"1": hex::encode([0u8; 16])}}).to_string(),
                "length mismatch",
            ),
            (
                serde_json::json!({"active": 7, "keys": {"1": hex::encode([0u8; 32])}}).to_string(),
                "has no key",
            ),
            (
                serde_json::json!({"active": 0, "keys": {"0": hex::encode([0u8; 32])}}).to_string(),
                ">= 1",
            ),
        ];
        for (raw, needle) in cases {
            let s = MockSecretStore::new();
            s.set(KEYRING_SECRET_ID, &raw).expect("seed");
            let err = load_or_create(&s).expect_err("损坏必须报错");
            assert!(err.contains(needle), "损坏形态 [{needle}] 报错不符: {err}");
            // 关键：报错后**没有**被新密钥覆盖（真源未被静默改写）
            assert_eq!(
                s.get(KEYRING_SECRET_ID).expect("read").as_deref(),
                Some(raw.as_str()),
                "解析失败绝不能顺手重写密钥环"
            );
        }
        // 收尾：确认正常环不受影响（对照组）
        let s = MockSecretStore::new();
        let ring = load_or_create(&s).expect("ok");
        assert_eq!(ring.signing_key().expect("key").len(), JWT_SECRET_KEY_LEN);
        assert!(good.signing_key().is_ok());
    }

    // ==================== 轮换（ADR 0033 D4） ====================

    /// 轮换：新生一代成为 active，原代降为上一代（**仍可验签**——宽限期 7 天）
    #[test]
    fn rotate_keeps_previous_generation_for_grace_window() {
        let store = MockSecretStore::new();
        let before = load_or_create(&store).expect("first");
        let old_key = before.signing_key().expect("key").to_vec();

        let after = rotate(&store).expect("rotate");
        assert_eq!(after.active, 2, "轮换推进代次");
        assert_ne!(
            after.signing_key().expect("key"),
            old_key.as_slice(),
            "新代必须换新密钥"
        );
        // 上一代仍在验签候选里（跨代验签是轮换的全部意义）
        let verify_keys = after.verification_keys();
        assert_eq!(verify_keys.len(), 2, "轮换后应有两代可验签");
        assert!(
            verify_keys.contains(&old_key.as_slice()),
            "宽限期内旧代必须仍能验签"
        );
        // 当前代优先
        assert_eq!(verify_keys[0], after.signing_key().expect("key"));
        // kid 语义
        assert_eq!(after.active_kid(), "g2");
        assert!(after.knows_kid(Some("g2")));
        assert!(after.knows_kid(Some("g1")), "上一代 kid 仍被接受");
        assert!(!after.knows_kid(Some("g99")), "未知 kid 必须被识别出来");
        assert!(after.knows_kid(None), "旧 token 无 kid → 一律走密钥尝试");
    }

    /// 连续轮换两次：环内**恒**至多两代（更老的裁掉——token 早过宽限期）
    #[test]
    fn rotation_is_bounded_and_drops_expired_generations() {
        let store = MockSecretStore::new();
        let gen1_key = load_or_create(&store).expect("g1").signing_key().expect("k").to_vec();
        let gen2_key = rotate(&store).expect("g2").signing_key().expect("k").to_vec();
        let ring3 = rotate(&store).expect("g3");
        assert_eq!(ring3.active, 3);
        assert_eq!(
            ring3.verification_keys().len(),
            MAX_GENERATIONS,
            "环内代次必须有界，不无限累积"
        );
        let keys = ring3.verification_keys();
        assert!(keys.contains(&gen2_key.as_slice()), "上一代保留");
        assert!(
            !keys.contains(&gen1_key.as_slice()),
            "超出宽限期的最早一代必须被裁掉"
        );
        // 落库形状也只含两代（不是只在内存里裁）
        let stored = store.get(KEYRING_SECRET_ID).expect("read").expect("row");
        let file: KeyringFile = serde_json::from_str(&stored).expect("json");
        assert_eq!(file.keys.len(), MAX_GENERATIONS);
        assert!(!file.keys.contains_key(&1));
    }

    /// 轮换后重启：读回的是新代（轮换是持久的，不因重启回退）
    #[test]
    fn rotation_survives_restart() {
        let store = MockSecretStore::new();
        rotate(&store).expect("g2");
        let reloaded = load_or_create(&store).expect("reload");
        assert_eq!(reloaded.active, 2, "轮换必须落盘，重启不回退");
    }

    // ==================== 退役行清理 ====================

    /// 清理 ADR 0033 前的死密钥行：存在则删、不存在则幂等成功
    #[test]
    fn purge_legacy_key_is_idempotent() {
        let store = MockSecretStore::new();
        store.set(JWT_SECRET_KEY_ID, &hex::encode([0x41u8; 32])).unwrap();
        assert!(purge_legacy_key(&store).expect("purge"), "存在时报告已清理");
        assert!(store.get(JWT_SECRET_KEY_ID).unwrap().is_none());
        assert!(
            !purge_legacy_key(&store).expect("purge again"),
            "已清理过 → 幂等成功且不谎报"
        );
    }

    // ==================== native 路径 ====================

    /// native 路径显性失败（无宿主环境不得静默生成进程内密钥）
    #[test]
    fn native_paths_fail_without_host() {
        assert!(keyring_from_host_auth().is_err());
        assert!(rotate_from_host_auth().is_err());
        assert!(purge_legacy_key_from_host_auth().is_err());
    }
}
