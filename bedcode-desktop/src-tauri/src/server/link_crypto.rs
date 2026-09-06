//! Link Crypto — 局域网链路报文加密（HTTP/WS 载荷）
//!
//! 规划文档：`.scratch/http-ws-payload-encryption/spec.md`。本模块是链路加密的
//! 宿主侧协议与开关中心，职责四块：
//!
//! 1. **配置域** [`LinkCryptoConfig`]：全部默认关闭（opt-in），持久化于 DB
//!    settings 表单键 [`SETTING_KEY`]；运行期经 [`current_config`] /
//!    [`update_config`] 快照热更新，过滤器逐流量读取、不做注册/注销抖动。
//! 2. **身份密钥 Kd** [`LinkIdentity`]：桌面端静态 X25519 密钥对，首启生成、
//!   落盘 `app_data_dir/link_crypto_identity.json`（与 peer-net node_identity.json
//!   同目录同模式）；指纹 = SHA-256(公钥) 前 16 hex，供移动端 pin 与人工核对。
//!   已存在但损坏时拒绝重建而非静默换钥——静默换钥会让移动端 pin 全部失效，
//!   与 peer-net 身份决策 D3 同理。
//! 3. **HKDF 方向分离派生**：ECDH 共享密钥 → 各方向独立 AES-256 会话密钥，
//!    info 常量是跨端兼容性表面（移动端 TS 实现必须逐字节一致）。
//! 4. **过滤器骨架** [`LinkEncryptionFilter`]：注册进 `TrafficFilterChain` 的
//!    转换型过滤器。本地豁免三分支与通道子开关判定已完整实现；豁免外流量的
//!    真实加解密由 issue 02（HTTP 信封）/ issue 04（WS 帧）在本模块填充。
//!
//! 决策模型（spec §6）：发送方配置决定参与意愿 → 接收方按线上信封自动识别 →
//! 响应绑定与降级检测两条强制安全耦合在 issue 02/04 落地。

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::server::filter::{Direction, FilterContext, TrafficChannel, TrafficFilter, TrafficFilterChain, Verdict};
use crate::system::error::{AppError, Result};

// ==================== 协议核心再导出（issue 09 共享 crate） ====================
// 字节级协议面已抽至 packages/link-crypto（桌面 server 与移动端 event WS
// 共同消费，消除第三份实现）；此处保持既有模块路径与测试可见性不变。
use bedcode_link_crypto as proto;

pub use proto::{
    b64_decode, b64_encode, decrypt_http_body, encrypt_http_body, parse_negotiation, HttpEnvelope,
    WsDirectionCipher, WsTextEnvelope, NEGOTIATION_HEADER, ORIGIN_BINARY, ORIGIN_TEXT,
    PROTOCOL_VERSION, WS_BINARY_HEADER_LEN, WS_FRAME_VERSION, WS_TRANSCRIPT_PREFIX,
    HTTP_INFO_REQUEST, HTTP_INFO_RESPONSE, WS_INFO_CLIENT_TO_SERVER, WS_INFO_SERVER_TO_CLIENT,
};

// ==================== 常量 ====================

/// DB settings 表中的配置键（值为 LinkCryptoConfig 的 JSON 序列化）
pub const SETTING_KEY: &str = "trafficEncryption";

/// 身份密钥落盘文件名（与 node_identity.json 并列同目录）
const IDENTITY_FILE: &str = "link_crypto_identity.json";

/// 身份文件格式版本：密钥语义变化时递增并走迁移/拒绝路径
const IDENTITY_FORMAT_VERSION: u32 = 1;

/// 过滤器注册名（注销与日志定位用）
pub const FILTER_NAME: &str = "link-crypto";

// HKDF info 常量——协议兼容性表面，issue 05 移动端 TS 实现必须逐字节一致
// （已抽至共享 crate，经上方 pub use 保持既有路径可见）

// ==================== 配置域 ====================

/// 链路加密配置域（spec §6）
///
/// 默认值即「功能整体关闭」：`enabled=false` 时过滤器不注册，其余字段不生效；
/// 子通道开关默认 true 是有意设计——用户只需打开主开关即获全通道覆盖，
/// 粒度收窄是显式动作。未知字段一律拒绝（防前端拼写错误的开关被静默吞掉）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkCryptoConfig {
    /// 主开关：false 时过滤器不注册，全服务明文（与现状一致）
    #[serde(default)]
    pub enabled: bool,
    /// HTTP REST 载荷加解密
    #[serde(default = "default_true")]
    pub encrypt_http: bool,
    /// WS 终端通道帧加解密
    #[serde(default = "default_true")]
    pub encrypt_ws_terminal: bool,
    /// WS 事件通道帧加解密
    #[serde(default = "default_true")]
    pub encrypt_ws_event: bool,
    /// 服务端对未协商的老客户端放行明文；false 时非环回未协商请求一律拒绝
    #[serde(default = "default_true")]
    pub allow_plaintext_fallback: bool,
}

fn default_true() -> bool {
    true
}

impl Default for LinkCryptoConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            encrypt_http: true,
            encrypt_ws_terminal: true,
            encrypt_ws_event: true,
            allow_plaintext_fallback: true,
        }
    }
}

/// 运行期配置快照（单例）：过滤器热路径只做一次读锁克隆，无注册/注销抖动
static CONFIG_SNAPSHOT: LazyLock<RwLock<LinkCryptoConfig>> =
    LazyLock::new(|| RwLock::new(LinkCryptoConfig::default()));

/// 读取当前配置快照
pub fn current_config() -> LinkCryptoConfig {
    CONFIG_SNAPSHOT
        .read()
        .map(|guard| guard.clone())
        .unwrap_or_else(|_| {
            // 锁中毒退化为默认值（全关），可用性优先；中毒方已记 error
            tracing::error!("link crypto config snapshot lock poisoned, using default");
            LinkCryptoConfig::default()
        })
}

/// 更新配置快照（调用方负责先持久化成功再更新内存态）
pub fn update_config(config: LinkCryptoConfig) {
    match CONFIG_SNAPSHOT.write() {
        Ok(mut guard) => *guard = config,
        Err(_) => tracing::error!("link crypto config snapshot lock poisoned, update dropped"),
    }
}

/// 从 DB settings 表加载配置；键不存在或 JSON 非法均回退默认值（全关）
///
/// 非法 JSON 只 warn 不报错：配置损坏不应阻断启动，用户重存一次即可修复。
pub fn load_config_from_db(db: &Database) -> LinkCryptoConfig {
    let json = match db.get_setting(SETTING_KEY) {
        Ok(Some(json)) => json,
        Ok(None) => return LinkCryptoConfig::default(),
        Err(e) => {
            tracing::warn!(
                key = SETTING_KEY,
                "read traffic encryption config failed, falling back to default (all off): {e}"
            );
            return LinkCryptoConfig::default();
        }
    };
    serde_json::from_str(&json).unwrap_or_else(|e| {
        tracing::warn!(
            key = SETTING_KEY,
            "traffic encryption config corrupt, falling back to default (all off): {e}"
        );
        LinkCryptoConfig::default()
    })
}

/// 持久化配置到 DB settings 表（JSON 序列化失败属程序错误，上抛）
pub fn persist_config_to_db(db: &Database, config: &LinkCryptoConfig) -> Result<()> {
    let json = serde_json::to_string(config)?;
    db.set_setting(SETTING_KEY, &json)?;
    Ok(())
}

// ==================== 身份密钥 Kd ====================

/// 身份密钥落盘结构
#[derive(Serialize, Deserialize)]
struct IdentityFile {
    version: u32,
    x25519_private_hex: String,
}

/// 桌面端链路加密静态身份密钥（X25519）
///
/// 手写 Debug：只暴露指纹，私钥材料绝不进日志/调试输出。
pub struct LinkIdentity {
    keypair: crate::utils::crypto::x25519::X25519KeyPair,
    fingerprint: String,
}

impl std::fmt::Debug for LinkIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinkIdentity")
            .field("fingerprint", &self.fingerprint)
            .finish_non_exhaustive()
    }
}

impl LinkIdentity {
    /// 加载或生成身份密钥：
    /// - 目录下无文件 → 生成新密钥对并落盘（首次启动）；
    /// - 文件存在且合法 → 加载（重启后指纹不变，pin 不失效）；
    /// - 文件存在但损坏/版本不符 → 报错拒绝重建：静默换钥会让移动端 pin 全部
    ///   失效，宁可本次启动禁用加密并由用户显式处理。
    pub fn load_or_create(dir: &Path) -> Result<Self> {
        let path = dir.join(IDENTITY_FILE);
        if path.exists() {
            return Self::load(&path);
        }

        let keypair = crate::utils::crypto::x25519::x25519_generate();
        let file = IdentityFile {
            version: IDENTITY_FORMAT_VERSION,
            x25519_private_hex: hex::encode(keypair.private()),
        };
        let json = serde_json::to_string_pretty(&file)
            .map_err(|e| AppError::Internal(format!("serialize link identity failed: {e}")))?;
        std::fs::write(&path, json)
            .map_err(|e| AppError::Internal(format!("write {}: {e}", path.display())))?;
        tracing::info!(file = %path.display(), "link crypto identity generated");
        Self::from_keypair(keypair)
    }

    fn load(path: &Path) -> Result<Self> {
        let json = std::fs::read_to_string(path)
            .map_err(|e| AppError::Internal(format!("read {}: {e}", path.display())))?;
        let file: IdentityFile = serde_json::from_str(&json).map_err(|e| {
            AppError::Internal(format!(
                "link identity {} is corrupt, refusing to regenerate (pins would break): {e}",
                path.display()
            ))
        })?;
        if file.version != IDENTITY_FORMAT_VERSION {
            return Err(AppError::Internal(format!(
                "link identity version mismatch: file v{}, expected v{}",
                file.version, IDENTITY_FORMAT_VERSION
            )));
        }
        let mut private = [0u8; crate::utils::crypto::x25519::KEY_LEN];
        hex::decode_to_slice(&file.x25519_private_hex, &mut private).map_err(|e| {
            AppError::Internal(format!("link identity private key invalid hex: {e}"))
        })?;
        let keypair =
            crate::utils::crypto::x25519::X25519KeyPair::from_private(&private);
        Self::from_keypair(keypair)
    }

    fn from_keypair(keypair: crate::utils::crypto::x25519::X25519KeyPair) -> Result<Self> {
        // 指纹先于 move 计算：keypair 随结构体构造被消费
        let fingerprint = fingerprint_of(keypair.public());
        Ok(Self {
            keypair,
            fingerprint,
        })
    }

    /// 公钥（base64，配对响应下发用，issue 03 消费）
    pub fn public_b64(&self) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(self.keypair.public())
    }

    /// 私钥引用（请求解密 / WS 派生用，issue 02/04 消费）
    pub(crate) fn keypair(&self) -> &crate::utils::crypto::x25519::X25519KeyPair {
        &self.keypair
    }

    /// 指纹：SHA-256(公钥) 前 16 hex 小写，设置页展示 + 移动端 pin 展示比对
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
}

/// 公钥指纹：SHA-256 前 16 hex 小写（实现在共享 crate）
fn fingerprint_of(public: &[u8; 32]) -> String {
    proto::fingerprint_of(public)
}

static IDENTITY: OnceLock<LinkIdentity> = OnceLock::new();

/// 进程内初始化身份密钥（幂等：已初始化则直接返回现有指纹）
///
/// 启动装配与命令懒初始化共用入口；失败不 panic——调用方记 error 后按
/// 「本进程加密不可用」处理（配置强制回退全关）。
pub fn init_identity(dir: &Path) -> Result<&'static str> {
    if let Some(existing) = IDENTITY.get() {
        return Ok(existing.fingerprint());
    }
    let identity = LinkIdentity::load_or_create(dir)?;
    // 竞争失败说明并发初始化已成功，取既有实例即可（两者内容一致：
    // load_or_create 对同一文件是确定性的）；set 成功后 get 必为 Some，
    // 理论不可达的 None 以显式错误收口（不得引用局部值充当 'static）
    let _ = IDENTITY.set(identity);
    IDENTITY
        .get()
        .map(|i| i.fingerprint())
        .ok_or_else(|| AppError::Internal("link identity registration lost".to_string()))
}

/// 已初始化的身份指纹（未初始化返回 None，命令层负责懒初始化）
pub fn identity_fingerprint() -> Option<&'static str> {
    IDENTITY.get().map(|i| i.fingerprint())
}

/// 身份密钥访问（03 下发公钥 / 测试消费）；未初始化返回 None
pub fn identity_parts() -> Option<(&'static str, String)> {
    IDENTITY.get().map(|i| (i.fingerprint(), i.public_b64()))
}

/// 身份私钥访问（02/04 内部派生用；未初始化返回 None，调用方 fail-closed）
#[allow(dead_code)] // 预留：ADR 02/04 内部派生路径尚未接入
pub(crate) fn identity_keypair(
) -> Option<&'static crate::utils::crypto::x25519::X25519KeyPair> {
    IDENTITY.get().map(|i| i.keypair())
}

// ==================== HKDF 方向分离派生 ====================

/// HTTP 单发加密的两方向会话密钥
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpTrafficKeys {
    /// 请求体加密密钥（发送方→桌面端）
    pub request: [u8; 32],
    /// 响应体加密密钥（桌面端→发送方），与请求密钥密码学隔离
    pub response: [u8; 32],
}

/// 由 ECDH 共享密钥派生 HTTP 两方向密钥
///
/// salt = 请求路径 ASCII 字节（路径绑定，防信封跨端点搬运）；
/// info 区分方向（request ≠ response，单向泄露不波及另一向）。
pub fn derive_http_traffic_keys(shared_ikm: &[u8], http_path: &str) -> Result<HttpTrafficKeys> {
    let salt = http_path.as_bytes();
    let req = crate::utils::crypto::kdf::hkdf_sha256(Some(salt), shared_ikm, HTTP_INFO_REQUEST, 32)?;
    let resp =
        crate::utils::crypto::kdf::hkdf_sha256(Some(salt), shared_ikm, HTTP_INFO_RESPONSE, 32)?;
    let to_arr = |v: Vec<u8>| {
        let mut out = [0u8; 32];
        out.copy_from_slice(&v);
        out
    };
    Ok(HttpTrafficKeys {
        request: to_arr(req),
        response: to_arr(resp),
    })
}

// 派生单个 WS 方向密码上下文（HKDF OKM 36B：32B key + 4B 随机前缀）——
// 实现已抽至共享 crate（proto 内部）；桌面侧经 derive_server_handshake 间接消费
// ==================== WS 会话加密（issue 04） ====================

// WS 帧常量与来源类型字节（WS_BINARY_HEADER_LEN / WS_FRAME_VERSION /
// WS_TRANSCRIPT_PREFIX / ORIGIN_TEXT / ORIGIN_BINARY）已抽至共享 crate，
// 经文件头 `pub use proto::{...}` 保持既有模块路径与测试可见性不变。

/// 单方向密码上下文与双向密码状态：
/// WsDirectionCipher 实现在共享 crate（文件头再导出，字段形状不变）；
/// 带序号计数器的 WsSessionCiphers 注册表类型留在桌面侧。

/// 一条 WS 连接的双向密码状态（服务端视角；发送/接收序号严格单调）
pub struct WsSessionCiphers {
    pub client_to_server: WsDirectionCipher,
    pub server_to_client: WsDirectionCipher,
    s2c_next_seq: u64,
    c2s_expected_seq: u64,
}

/// 握手产物：服务端临时公钥回执 + 注册用双向密码
pub struct WsHandshake {
    pub server_ek_b64: String,
    pub ciphers: WsSessionCiphers,
}

/// 双 ECDH 握手派生（spec §3）：服务端生成新鲜 s_eph，
/// IKM = ECDH(s_eph,m_eph) ‖ ECDH(Kd,m_eph)，salt = "bc-link-crypto/v1" ‖ m_ek_b64 ‖ s_ek_b64。
/// 字节拼接顺序是跨端兼容性表面，issue 05 移动端 TS 必须一致（金样互验钉死）。
/// 公式实现在共享 crate；本函数只负责身份私钥与临时密钥生命周期。
pub fn derive_ws_session_ciphers(client_ek_b64: &str) -> Result<WsHandshake> {
    let Some(identity) = IDENTITY.get() else {
        return Err(AppError::Internal("link identity unavailable".to_string()));
    };
    let (s_ephemeral_private, _s_public_unused) = proto::generate_ephemeral();
    let hs = proto::derive_server_handshake(client_ek_b64, identity.keypair().private(), &s_ephemeral_private)?;

    Ok(WsHandshake {
        server_ek_b64: hs.server_ek_b64,
        ciphers: WsSessionCiphers {
            client_to_server: hs.client_to_server,
            server_to_client: hs.server_to_client,
            s2c_next_seq: 0,
            c2s_expected_seq: 0,
        },
    })
}

// ---------- 连接密码表（actor 注册 / 过滤器消费，keyed by 对端 addr） ----------

static WS_CIPHERS: LazyLock<Mutex<HashMap<String, WsSessionCiphers>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 握手完成后注册连接密码（此后该连接所有帧进入加密模式）；重复注册覆盖
pub fn ws_register_ciphers(addr: &str, ciphers: WsSessionCiphers) {
    match WS_CIPHERS.lock() {
        Ok(mut map) => {
            map.insert(addr.to_string(), ciphers);
        }
        Err(_) => tracing::error!(addr, "ws cipher registry lock poisoned, register dropped"),
    }
}

/// 断连清理（terminal_ws disconnected 调用）
pub fn ws_remove_ciphers(addr: &str) {
    if let Ok(mut map) = WS_CIPHERS.lock() {
        map.remove(addr);
    }
}

/// 连接是否已处于加密模式（过滤器与 Close-4003 判定共用）
pub fn ws_has_ciphers(addr: &str) -> bool {
    WS_CIPHERS.lock().map(|map| map.contains_key(addr)).unwrap_or(false)
}

// nonce/AAD 构造已抽至共享 crate；AAD 的 Direction 经适配器映射
use proto::ws_nonce;

fn ws_aad(channel_str: &str, direction: Direction, origin: u8) -> Vec<u8> {
    proto::ws_aad(
        channel_str,
        match direction {
            Direction::Inbound => proto::Direction::Inbound,
            Direction::Outbound => proto::Direction::Outbound,
        },
        origin,
    )
}

// WS 文本帧信封（WsTextEnvelope）已抽至共享 crate，经文件头 `pub use proto::{...}`
// 保持既有模块路径可见；字段形状逐字节一致。

/// 加密一条出站文本帧（JSON → 信封 JSON 字符串），并推进发送序号
pub(crate) fn ws_encrypt_outbound_text(
    addr: &str,
    channel_str: &str,
    text: &str,
) -> Result<Vec<u8>> {
    let mut map = WS_CIPHERS.lock().map_err(|_| AppError::Internal("ws cipher lock poisoned".into()))?;
    let c = map.get_mut(addr).ok_or_else(|| AppError::Internal("no ws cipher for addr".into()))?;
    let seq = c.s2c_next_seq;
    let aad = ws_aad(channel_str, Direction::Outbound, ORIGIN_TEXT);
    let sealed = encrypt_ws_payload(&c.server_to_client, seq, text.as_bytes(), &aad)?;
    c.s2c_next_seq = seq.checked_add(1).ok_or_else(|| AppError::Internal("ws seq overflow".into()))?;
    let envelope = WsTextEnvelope { v: WS_FRAME_VERSION, seq, n: b64_encode(&sealed.nonce), ct: b64_encode(&sealed.ciphertext) };
    Ok(serde_json::to_vec(&envelope)?)
}

/// 解密一条入站文本帧（信封 JSON → 原 JSON 字符串），严格校验接收序号
pub(crate) fn ws_decrypt_inbound_text(addr: &str, channel_str: &str, body: &[u8]) -> Result<String> {
    let envelope: WsTextEnvelope = serde_json::from_slice(body)
        .map_err(|e| AppError::Internal(format!("ws text envelope malformed: {e}")))?;
    if envelope.v != WS_FRAME_VERSION {
        return Err(AppError::Internal(format!("unsupported ws frame version {}", envelope.v)));
    }
    let nonce_v = b64_decode(&envelope.n)?;
    let nonce: [u8; 12] = nonce_v.try_into().map_err(|v: Vec<u8>| {
        AppError::Internal(format!("nonce length mismatch: {}", v.len()))
    })?;
    let ciphertext = b64_decode(&envelope.ct)?;

    let mut map = WS_CIPHERS.lock().map_err(|_| AppError::Internal("ws cipher lock poisoned".into()))?;
    let c = map.get_mut(addr).ok_or_else(|| AppError::Internal("no ws cipher for addr".into()))?;
    if envelope.seq != c.c2s_expected_seq {
        return Err(AppError::Internal(format!(
            "ws seq mismatch: expected {}, got {}",
            c.c2s_expected_seq, envelope.seq
        )));
    }
    let plain = crate::utils::crypto::aes_gcm::decrypt(
        &c.client_to_server.key,
        &nonce,
        &ciphertext,
        Some(&ws_aad(channel_str, Direction::Inbound, ORIGIN_TEXT)),
    )?;
    c.c2s_expected_seq += 1;
    String::from_utf8(plain).map_err(|e| AppError::Internal(format!("decrypted text not utf-8: {e}")))
}

struct SealedPayload { nonce: [u8; 12], ciphertext: Vec<u8> }

fn encrypt_ws_payload(
    cipher: &WsDirectionCipher,
    seq: u64,
    plaintext: &[u8],
    aad: &[u8],
) -> Result<SealedPayload> {
    let nonce = ws_nonce(&cipher.nonce_prefix, seq);
    let ciphertext =
        crate::utils::crypto::aes_gcm::encrypt(&cipher.key, &nonce, plaintext, Some(aad))?;
    Ok(SealedPayload { nonce, ciphertext })
}

/// 加密一条出站二进制帧（原帧 → ver+seq+ct），并推进发送序号
pub(crate) fn ws_encrypt_outbound_binary(
    addr: &str,
    channel_str: &str,
    data: &[u8],
) -> Result<Vec<u8>> {
    let mut map = WS_CIPHERS.lock().map_err(|_| AppError::Internal("ws cipher lock poisoned".into()))?;
    let c = map.get_mut(addr).ok_or_else(|| AppError::Internal("no ws cipher for addr".into()))?;
    let seq = c.s2c_next_seq;
    let aad = ws_aad(channel_str, Direction::Outbound, ORIGIN_BINARY);
    let sealed = encrypt_ws_payload(&c.server_to_client, seq, data, &aad)?;
    c.s2c_next_seq = seq.checked_add(1).ok_or_else(|| AppError::Internal("ws seq overflow".into()))?;
    let mut frame = Vec::with_capacity(WS_BINARY_HEADER_LEN + sealed.ciphertext.len());
    frame.push(WS_FRAME_VERSION);
    frame.extend_from_slice(&seq.to_be_bytes());
    frame.extend_from_slice(&sealed.ciphertext);
    Ok(frame)
}

/// 解密一条入站二进制帧（ver+seq+ct → 原帧），严格校验接收序号
pub(crate) fn ws_decrypt_inbound_binary(
    addr: &str,
    channel_str: &str,
    data: &[u8],
) -> Result<Vec<u8>> {
    if data.len() < WS_BINARY_HEADER_LEN {
        return Err(AppError::Internal(format!(
            "ws binary frame too short: {}",
            data.len()
        )));
    }
    if data[0] != WS_FRAME_VERSION {
        return Err(AppError::Internal(format!("unsupported ws frame version {}", data[0])));
    }
    let seq = u64::from_be_bytes(data[1..9].try_into().expect("seq slice is 8 bytes"));
    let ciphertext = &data[WS_BINARY_HEADER_LEN..];

    let mut map = WS_CIPHERS.lock().map_err(|_| AppError::Internal("ws cipher lock poisoned".into()))?;
    let c = map.get_mut(addr).ok_or_else(|| AppError::Internal("no ws cipher for addr".into()))?;
    if seq != c.c2s_expected_seq {
        return Err(AppError::Internal(format!(
            "ws seq mismatch: expected {}, got {}",
            c.c2s_expected_seq, seq
        )));
    }
    let plain = crate::utils::crypto::aes_gcm::decrypt(
        &c.client_to_server.key,
        &ws_nonce(&c.client_to_server.nonce_prefix, seq),
        ciphertext,
        Some(&ws_aad(channel_str, Direction::Inbound, ORIGIN_BINARY)),
    )?;
    c.c2s_expected_seq += 1;
    Ok(plain)
}

/// 过滤器统一分发：按帧来源选择文本/二进制编解码路径
fn ws_decrypt_inbound_text_binary(
    addr: &str,
    channel_str: &str,
    origin: u8,
    data: &[u8],
) -> Result<Vec<u8>> {
    if origin == ORIGIN_TEXT {
        ws_decrypt_inbound_text(addr, channel_str, data).map(String::into_bytes)
    } else {
        ws_decrypt_inbound_binary(addr, channel_str, data)
    }
}

fn ws_encrypt_outbound_text_binary(
    addr: &str,
    channel_str: &str,
    origin: u8,
    plaintext: &[u8],
) -> Result<Vec<u8>> {
    if origin == ORIGIN_TEXT {
        let text = std::str::from_utf8(plaintext)
            .map_err(|e| AppError::Internal(format!("outbound text not utf-8: {e}")))?;
        ws_encrypt_outbound_text(addr, channel_str, text)
    } else {
        ws_encrypt_outbound_binary(addr, channel_str, plaintext)
    }
}

// ==================== HTTP 信封协议（issue 02） ====================

// 协商信号头与协议版本常量（NEGOTIATION_HEADER / PROTOCOL_VERSION）已抽至
// 共享 crate，经文件头 `pub use proto::{...}` 保持既有模块路径可见。

/// 请求级响应密钥缓存 TTL：同请求出入站间隔毫秒级，30s 已是数百倍冗余
const REQUEST_KEY_TTL: Duration = Duration::from_secs(30);

// HTTP 信封结构与编解码辅助（HttpEnvelope / b64_encode / b64_decode /
// parse_negotiation）已抽至共享 crate，经文件头 `pub use proto::{...}`
// 保持既有模块路径与测试可见性不变（错误转换经 AppError 的 From impl）。

/// HTTP AAD 绑定：b"v1" || direction || u32be(path_len) || path
///
/// 路径绑定防信封跨端点搬运；方向绑定防请求/响应载荷互换重放。
/// 实现在共享 crate，此处仅做 Direction 适配。
fn http_aad(direction: Direction, path: &str) -> Vec<u8> {
    proto::http_aad(
        match direction {
            Direction::Inbound => proto::Direction::Inbound,
            Direction::Outbound => proto::Direction::Outbound,
        },
        path,
    )
}

// 信封编解码（encrypt_http_body / decrypt_http_body）已抽至共享 crate，
// 经文件头 `pub use proto::{...}` 保持既有模块路径与测试可见性不变
//（错误经 AppError 的 From<LinkCryptoError> 自动转换，文案不变）。

// ---------- 请求级响应密钥缓存 ----------
//
// filter 是全局单例、出入站两次独立调用，而响应加密密钥只能从该次请求的
// 临时公钥派生（spec §3 实现要点）。入站成功解密时写入，出站命中即取走，
// 未命中（入站被拒/未协商）→ 响应保持明文。
// 移动端每请求全新临时密钥对 → (peer, ek) 天然唯一，无并发覆盖问题。

struct CachedHttpKeys {
    keys: HttpTrafficKeys,
    inserted_at: Instant,
}

static HTTP_KEY_CACHE: LazyLock<Mutex<HashMap<String, CachedHttpKeys>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn cache_key(peer: &str, ek_b64: &str) -> String {
    // \u{1} 作分隔符：peer 与 ek 各自不含该控制字符，杜绝拼接歧义
    format!("{peer}\u{1}{ek_b64}")
}

fn entry_expired(entry: &CachedHttpKeys, now: Instant) -> bool {
    now.duration_since(entry.inserted_at) >= REQUEST_KEY_TTL
}

fn store_http_keys(peer: &str, ek_b64: &str, keys: HttpTrafficKeys) {
    let now = Instant::now();
    match HTTP_KEY_CACHE.lock() {
        Ok(mut map) => {
            map.retain(|_, entry| !entry_expired(entry, now)); // 插入时顺带清扫
            map.insert(cache_key(peer, ek_b64), CachedHttpKeys { keys, inserted_at: now });
        }
        Err(_) => {
            tracing::error!("http key cache lock poisoned, response will stay plaintext");
        }
    }
}

fn take_http_keys(peer: &str, ek_b64: &str) -> Option<HttpTrafficKeys> {
    let now = Instant::now();
    match HTTP_KEY_CACHE.lock() {
        Ok(mut map) => {
            let key = cache_key(peer, ek_b64);
            match map.remove(&key) {
                Some(entry) if !entry_expired(&entry, now) => Some(entry.keys),
                Some(_) => None, // 存在但已过期：视作 miss
                None => None,
            }
        }
        Err(_) => None,
    }
}

// ==================== 过滤器骨架 ====================

/// 明文白名单路由（HTTP）：配对引导期端点——它们本身是 pinning 建立即刻，
/// 必须明文可达；健康检查供探活。豁免不受任何配置开关影响（spec §6 硬约束）。
fn is_plaintext_whitelisted(route: &str) -> bool {
    route.starts_with("/api/auth/") || route == "/health" || route == "/api/health"
}

/// 解析 FilterContext.peer 为 SocketAddr（HTTP = 客户端地址，WS = 连接对端地址）
fn parse_peer(peer: &str) -> Option<std::net::SocketAddr> {
    peer.parse().ok()
}

/// 本地豁免判定（spec §4，硬约束不受开关影响）：
/// ① WsLocal 通道（桌面 WebView 本地终端，构造侧已标）；
/// ② 环回对端（hook 脚本调 /api/plugin/*、本机工具直连 REST）；
/// ③ 配对引导白名单路由。
/// peer 无法解析时视为远端（fail-safe：宁多过滤不放行）。
fn is_exempt(ctx: &FilterContext<'_>) -> bool {
    if matches!(ctx.channel, TrafficChannel::WsLocal) {
        return true;
    }
    if parse_peer(ctx.peer).is_some_and(|addr| addr.ip().is_loopback()) {
        return true;
    }
    matches!(ctx.channel, TrafficChannel::Http) && is_plaintext_whitelisted(ctx.route)
}

impl LinkEncryptionFilter {
    /// 该流量是否应进入加解密处理（豁免外 + 主开关开 + 对应通道子开关开）
    pub fn should_process(ctx: &FilterContext<'_>) -> bool {
        if is_exempt(ctx) {
            return false;
        }
        let config = current_config();
        if !config.enabled {
            return false;
        }
        match ctx.channel {
            TrafficChannel::Http => config.encrypt_http,
            TrafficChannel::WsTerminal => config.encrypt_ws_terminal,
            TrafficChannel::WsEvent => config.encrypt_ws_event,
            // 豁免分支已拦截，此处不可达；保守放行
            TrafficChannel::WsLocal => false,
        }
    }

    /// WS 入站/出站统一入口：连接已注册密码 → 帧级加解密（fail-closed，
    /// Reject 由 terminal_ws 钩子升级为 Close 4003）；未注册（明文会话）→
    /// 按服务端回退策略裁决
    fn on_ws_frame(ctx: &mut FilterContext<'_>) -> Verdict {
        if !ws_has_ciphers(ctx.peer) {
            return if current_config().allow_plaintext_fallback {
                Verdict::Continue
            } else {
                Verdict::Reject(
                    "link encryption required by server policy (allow_plaintext_fallback=false)"
                        .to_string(),
                )
            };
        }
        let origin = if ctx.route == "binary" { ORIGIN_BINARY } else { ORIGIN_TEXT };
        let channel_str = ctx.channel.as_str();
        let result = match ctx.direction {
            Direction::Inbound => {
                ws_decrypt_inbound_text_binary(ctx.peer, channel_str, origin, &ctx.data)
            }
            Direction::Outbound => {
                ws_encrypt_outbound_text_binary(ctx.peer, channel_str, origin, &ctx.data)
            }
        };
        match result {
            Ok(sealed) => {
                crate::server::metrics::MetricsCollector::global().inc_encrypted_frame();
                ctx.data = sealed;
                Verdict::Continue
            }
            Err(e) => {
                crate::server::metrics::MetricsCollector::global().inc_decrypt_failure();
                Verdict::Reject(format!("ws frame crypto failed: {e}"))
            }
        }
    }

    /// HTTP 入站：协商 → ECDH(Kd, ek) 派生 → 解信封 → 缓存响应密钥。
    /// 任一步失败一律 Reject（fail-closed），错误详情进 400 响应体。
    ///
    /// GET/HEAD 无请求体：协商头携带临时公钥即足以派生响应密钥，body
    /// 为空时跳过解信封（无载荷可解），仅缓存密钥供出站加密响应。
    fn on_http_inbound(ctx: &mut FilterContext<'_>) -> Verdict {
        let Some(ek_b64) = parse_negotiation(ctx.negotiation) else {
            // 无协商：按服务端明文回退策略裁决（老客户端兼容 vs 强加密模式）
            return if current_config().allow_plaintext_fallback {
                Verdict::Continue
            } else {
                Verdict::Reject(
                    "link encryption required by server policy (allow_plaintext_fallback=false)"
                        .to_string(),
                )
            };
        };
        let Some(identity) = IDENTITY.get() else {
            return Verdict::Reject("link identity unavailable".to_string());
        };

        let result = (|| -> Result<()> {
            let ek_raw = b64_decode(ek_b64)?;
            let peer_public: [u8; crate::utils::crypto::x25519::KEY_LEN] = ek_raw
                .try_into()
                .map_err(|v: Vec<u8>| {
                    AppError::Internal(format!(
                        "negotiation key length mismatch: expected 32, got {}",
                        v.len()
                    ))
                })?;
            let shared =
                crate::utils::crypto::x25519::x25519_diffie_hellman(identity.keypair(), &peer_public)?;
            let keys = derive_http_traffic_keys(shared.as_bytes(), ctx.route)?;
            // GET/HEAD 空 body：无载荷可解，协商仅用于响应加密（密钥派生已足以
            // 证明对端持有临时私钥）——空 body 直接通过，不尝试解信封
            if !ctx.data.is_empty() {
                let request_key = keys.request;
                let plaintext = decrypt_http_body(
                    &request_key,
                    &ctx.data,
                    &http_aad(Direction::Inbound, ctx.route),
                )?;
                ctx.data = plaintext;
            }
            store_http_keys(ctx.peer, ek_b64, keys);
            Ok(())
        })();

        match result {
            Ok(()) => {
                crate::server::metrics::MetricsCollector::global().inc_encrypted_frame();
                Verdict::Continue
            }
            Err(e) => {
                crate::server::metrics::MetricsCollector::global().inc_decrypt_failure();
                Verdict::Reject(format!("http request decrypt failed: {e}"))
            }
        }
    }

    /// HTTP 出站：入站已成功协商的请求 → 用缓存的 k_resp 加密响应；
    /// 未命中（未协商/入站被拒）→ 保持明文（错误信息需对端可读，不含机密）
    fn on_http_outbound(ctx: &mut FilterContext<'_>) -> Verdict {
        let Some(ek_b64) = parse_negotiation(ctx.negotiation) else {
            return Verdict::Continue;
        };
        let Some(keys) = take_http_keys(ctx.peer, ek_b64) else {
            return Verdict::Continue;
        };
        match encrypt_http_body(&keys.response, &ctx.data, &http_aad(Direction::Outbound, ctx.route)) {
            Ok(envelope_json) => {
                crate::server::metrics::MetricsCollector::global().inc_encrypted_frame();
                ctx.data = envelope_json;
                // 响应加密标记（spec §4）：移动端据 `X-BedCode-Crypto: v1`
                // 识别加密响应并解信封；值必须带 "v" 前缀（与请求侧
                // parse_negotiation 的 `format!("v{PROTOCOL_VERSION}")` 同源），
                // 裸 `1` 会被移动端 `respHeader === 'v1'` 判为不匹配 → 误报降级
                ctx.outbound_headers.push((
                    NEGOTIATION_HEADER.to_string(),
                    format!("v{PROTOCOL_VERSION}"),
                ));
                Verdict::Continue
            }
            Err(e) => {
                crate::server::metrics::MetricsCollector::global().inc_decrypt_failure();
                Verdict::Reject(format!("http response encrypt failed: {e}"))
            }
        }
    }
}

/// 链路加密过滤器（转换型）
///
/// 注册进全局责任链后对所有通道的收发载荷生效；本地豁免流量零介入直通。
pub struct LinkEncryptionFilter;

impl TrafficFilter for LinkEncryptionFilter {
    fn name(&self) -> &str {
        FILTER_NAME
    }

    fn on_inbound(&self, ctx: &mut FilterContext<'_>) -> Verdict {
        if !Self::should_process(ctx) {
            return Verdict::Continue;
        }
        match ctx.channel {
            TrafficChannel::Http => Self::on_http_inbound(ctx),
            TrafficChannel::WsTerminal | TrafficChannel::WsEvent => Self::on_ws_frame(ctx),
            // 豁免分支已拦截，不可达
            TrafficChannel::WsLocal => Verdict::Continue,
        }
    }

    fn on_outbound(&self, ctx: &mut FilterContext<'_>) -> Verdict {
        if !Self::should_process(ctx) {
            return Verdict::Continue;
        }
        match ctx.channel {
            TrafficChannel::Http => Self::on_http_outbound(ctx),
            TrafficChannel::WsTerminal | TrafficChannel::WsEvent => Self::on_ws_frame(ctx),
            TrafficChannel::WsLocal => Verdict::Continue,
        }
    }
}

// ==================== 注册与启动装配 ====================

/// 过滤器是否已在全局链注册（幂等防护：重复 register 会让链上出现同名节点）
static REGISTERED: AtomicBool = AtomicBool::new(false);

/// 按当前快照同步全局链注册状态（幂等；set 命令与启动装配共用）
pub fn sync_registration() {
    let enabled = current_config().enabled;
    let chain = TrafficFilterChain::global();
    if enabled {
        if !REGISTERED.swap(true, Ordering::SeqCst) {
            chain.register(std::sync::Arc::new(LinkEncryptionFilter));
        }
    } else if REGISTERED.swap(false, Ordering::SeqCst) {
        chain.unregister(FILTER_NAME);
    }
}

/// 向指定链注册（测试隔离用：单元测试走独立链实例，不触碰全局单例）
pub fn register_into(chain: &TrafficFilterChain) {
    chain.register(std::sync::Arc::new(LinkEncryptionFilter));
}

/// 启动期装配：懒建身份 → 读 DB 配置进快照 → 同步注册
///
/// 必须在 supervisor.start 之前完成（服务器收到的第一条流量就要被开关裁决）。
/// 身份损坏时强制回退全关并 error 日志——加密不可用是可接受的降级，
/// 静默换钥破坏 pin 是不可接受的错误。
pub async fn init_at_startup(app_handle: &tauri::AppHandle) {
    use tauri::Manager;

    let mut identity_ready = false;
    // Tauri v2 的 app_data_dir 返回 Result<PathBuf>（非 Option）
    match app_handle.path().app_data_dir() {
        Ok(dir) => match init_identity(&dir) {
            Ok(fp) => {
                tracing::info!(fingerprint = fp, "link crypto identity ready");
                identity_ready = true;
            }
            Err(e) => {
                tracing::error!("link crypto identity init failed, encryption stays off: {e}")
            }
        },
        Err(e) => {
            tracing::error!("app data dir unavailable ({e}), link crypto stays off");
        }
    }

    let db = app_handle.state::<Arc<tokio::sync::Mutex<Database>>>();
    let mut config = {
        let guard = db.lock().await;
        load_config_from_db(&guard)
    };
    if config.enabled && !identity_ready {
        tracing::warn!(
            "trafficEncryption enabled in settings but identity unavailable, forcing off this boot"
        );
        config.enabled = false;
    }

    update_config(config);
    sync_registration();
    tracing::info!(
        enabled = current_config().enabled,
        "link crypto initialized"
    );
}

/// 命令层懒初始化：已初始化直接返指纹；否则从 app 数据目录建身份后返回
pub async fn ensure_identity_fingerprint(app_handle: &tauri::AppHandle) -> Result<String> {
    if let Some(fp) = identity_fingerprint() {
        return Ok(fp.to_string());
    }
    use tauri::Manager;
    let dir = app_handle.path().app_data_dir().map_err(|e| {
        AppError::Internal(format!("app data dir unavailable for link identity: {e}"))
    })?;
    Ok(init_identity(&dir)?.to_string())
}

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use std::sync::Mutex;

    use crate::server::filter::Direction;

    /// 触碰全局配置快照的测试必须持此锁串行执行：
    /// cargo test 同模块用例默认多线程并行，否则 enabled 开关互踩产生偶发红
    static SNAPSHOT_LOCK: Mutex<()> = Mutex::new(());

    /// 在「主开关开 + 子通道全开」的隔离快照下执行断言，结束后还原
    fn with_all_enabled<T>(f: impl FnOnce() -> T) -> T {
        let _guard = SNAPSHOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let original = current_config();
        update_config(LinkCryptoConfig {
            enabled: true,
            ..LinkCryptoConfig::default()
        });
        let out = f();
        update_config(original);
        out
    }

    fn mk_ctx<'a>(
        channel: TrafficChannel,
        peer: &'a str,
        route: &'a str,
    ) -> FilterContext<'a> {
        FilterContext {
            channel,
            direction: Direction::Inbound,
            peer,
            route,
            negotiation: "",
            data: b"payload".to_vec(),
            outbound_headers: Vec::new(),
        }
    }

    // ---------- 配置域 ----------

    #[test]
    fn default_config_is_off_with_channels_on() {
        let cfg = LinkCryptoConfig::default();
        assert!(!cfg.enabled, "主开关必须默认关闭（opt-in 设计）");
        assert!(cfg.encrypt_http && cfg.encrypt_ws_terminal && cfg.encrypt_ws_event);
        assert!(cfg.allow_plaintext_fallback);
    }

    #[test]
    fn config_serde_roundtrip_and_unknown_field_rejection() {
        let cfg = LinkCryptoConfig {
            enabled: true,
            ..LinkCryptoConfig::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert_eq!(serde_json::from_str::<LinkCryptoConfig>(&json).unwrap(), cfg);

        // 未知字段拒绝：前端拼错开关名不允许被静默吞掉
        let bad = r#"{"enabled":true,"encryptHttp":false}"#;
        assert!(serde_json::from_str::<LinkCryptoConfig>(bad).is_err());

        // 部分字段省略走 serde default
        let partial = r#"{"enabled":true}"#;
        let parsed = serde_json::from_str::<LinkCryptoConfig>(partial).unwrap();
        assert!(parsed.enabled && parsed.encrypt_http);
    }

    #[test]
    fn config_db_roundtrip_and_corrupt_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(&dir.path().join("test.db")).unwrap();
        db.init_schema().unwrap();

        // 键不存在 → 默认全关
        assert!(!load_config_from_db(&db).enabled);

        let cfg = LinkCryptoConfig {
            enabled: true,
            encrypt_http: false,
            ..LinkCryptoConfig::default()
        };
        persist_config_to_db(&db, &cfg).unwrap();
        assert_eq!(load_config_from_db(&db), cfg);

        // 损坏 JSON → 回退默认不报错
        db.set_setting(SETTING_KEY, "{not-json").unwrap();
        assert!(!load_config_from_db(&db).enabled);
    }

    #[test]
    fn snapshot_update_visible_to_readers() {
        let _guard = SNAPSHOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let original = current_config();
        let mut cfg = original.clone();
        cfg.enabled = !cfg.enabled;
        update_config(cfg);
        assert_eq!(current_config().enabled, !original.enabled);
        // 还原，避免污染其他测试
        update_config(original);
    }

    // ---------- 身份密钥 ----------

    #[test]
    fn identity_load_or_create_is_idempotent_and_stable() {
        let dir = tempfile::tempdir().unwrap();

        let first = LinkIdentity::load_or_create(dir.path()).unwrap();
        let fp1 = first.fingerprint().to_string();
        assert_eq!(fp1.len(), 16, "指纹 = SHA-256 前 16 hex");
        assert!(fp1.chars().all(|c| c.is_ascii_hexdigit()));
        // base64 公钥可解码为 32 字节
        let raw = base64::engine::general_purpose::STANDARD
            .decode(first.public_b64())
            .unwrap();
        assert_eq!(raw.len(), 32);

        // 重启模拟：重新加载指纹不变
        let second = LinkIdentity::load_or_create(dir.path()).unwrap();
        assert_eq!(second.fingerprint(), fp1);
    }

    #[test]
    fn identity_corrupt_file_refuses_regeneration() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(IDENTITY_FILE), "{corrupt").unwrap();

        let err = LinkIdentity::load_or_create(dir.path()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("corrupt") || msg.contains("refusing"),
            "应明确拒绝重建而非静默换钥: {msg}"
        );
    }

    #[test]
    fn init_identity_is_idempotent_in_process() {
        let dir = tempfile::tempdir().unwrap();
        let fp = init_identity(dir.path()).unwrap().to_string();
        // 同进程二次调用返回既有指纹（不因传入不同目录而换钥）
        let again = init_identity(tempfile::tempdir().unwrap().path()).unwrap();
        assert_eq!(again, fp);
    }

    // ---------- HKDF 派生 ----------

    #[test]
    fn http_keys_deterministic_and_direction_isolated() {
        let ikm = [7u8; 32];
        let a = derive_http_traffic_keys(&ikm, "/api/sessions").unwrap();
        let b = derive_http_traffic_keys(&ikm, "/api/sessions").unwrap();
        assert_eq!(a, b, "同输入必须确定（两端各自派生要一致）");
        assert_ne!(a.request, a.response, "方向 info 隔离");

        // salt 参与派生：不同路径不同密钥（防信封跨端点搬运）
        let other = derive_http_traffic_keys(&ikm, "/api/file-tree").unwrap();
        assert_ne!(a.request, other.request);
        assert_ne!(a.response, other.response);
    }

    // ---------- WS 会话加密（issue 04） ----------

    /// 移动端 TS 侧的派生复刻：相同字节序列必须得到相同密钥（跨端兼容锚点）
    #[test]
    fn ws_handshake_interop_with_client_replication() {
        init_identity(tempfile::tempdir().unwrap().path()).unwrap();
        use crate::utils::crypto::x25519::{x25519_diffie_hellman, x25519_generate};

        let client_eph = x25519_generate();
        let m_ek_b64 = b64_encode(client_eph.public());
        let handshake = derive_ws_session_ciphers(&m_ek_b64).unwrap();

        // 客户端复算：eph-eph ‖ eph-Kd，transcript salt 相同
        let (_, kd_pub_b64) = identity_parts().unwrap();
        let kd_pub: [u8; 32] = b64_decode(&kd_pub_b64).unwrap().try_into().unwrap();
        let s_pub: [u8; 32] = b64_decode(&handshake.server_ek_b64)
            .unwrap()
            .try_into()
            .unwrap();
        let eph_eph = x25519_diffie_hellman(&client_eph, &s_pub).unwrap();
        let auth = x25519_diffie_hellman(&client_eph, &kd_pub).unwrap();
        let mut ikm = [0u8; 64];
        ikm[..32].copy_from_slice(eph_eph.as_bytes());
        ikm[32..].copy_from_slice(auth.as_bytes());
        let salt = [
            WS_TRANSCRIPT_PREFIX,
            m_ek_b64.as_bytes(),
            handshake.server_ek_b64.as_bytes(),
        ]
        .concat();
        let c2s = crate::utils::crypto::kdf::hkdf_sha256(
            Some(&salt),
            &ikm,
            WS_INFO_CLIENT_TO_SERVER,
            36,
        )
        .unwrap();
        let s2c = crate::utils::crypto::kdf::hkdf_sha256(
            Some(&salt),
            &ikm,
            WS_INFO_SERVER_TO_CLIENT,
            36,
        )
        .unwrap();

        assert_eq!(&c2s[..32], &handshake.ciphers.client_to_server.key);
        assert_eq!(&c2s[32..], &handshake.ciphers.client_to_server.nonce_prefix);
        assert_eq!(&s2c[..32], &handshake.ciphers.server_to_client.key);
        assert_eq!(&s2c[32..], &handshake.ciphers.server_to_client.nonce_prefix);
    }

    #[test]
    fn ws_frame_codec_roundtrip_and_seq_discipline() {
        init_identity(tempfile::tempdir().unwrap().path()).unwrap();
        let client_eph = crate::utils::crypto::x25519::x25519_generate();
        let handshake =
            derive_ws_session_ciphers(&b64_encode(client_eph.public())).unwrap();

        // 注册前克隆客户端视角的两把方向密钥（容器含私有序号不可整体克隆）
        let c2s_cipher = handshake.ciphers.client_to_server.clone();
        let s2c_cipher = handshake.ciphers.server_to_client.clone();
        ws_register_ciphers("t:1", handshake.ciphers);
        assert!(ws_has_ciphers("t:1"));

        // 客户端侧封帧助手：与移动端 TS 公式一致（c2s 密钥 + Inbound AAD）。
        // 单机测试必须双端模拟：服务端入站解密只认 c2s，拿 s2c 加密的
        // 出站帧回灌必然 AEAD 失败（方向隔离本就是协议设计）。
        let seal_client_text =
            |seq: u64, channel: &str, text: &str| -> Vec<u8> {
                let sealed = encrypt_ws_payload(
                    &c2s_cipher,
                    seq,
                    text.as_bytes(),
                    &ws_aad(channel, Direction::Inbound, ORIGIN_TEXT),
                )
                .unwrap();
                serde_json::to_vec(&WsTextEnvelope {
                    v: WS_FRAME_VERSION,
                    seq,
                    n: b64_encode(&sealed.nonce),
                    ct: b64_encode(&sealed.ciphertext),
                })
                .unwrap()
            };

        // 文本往返（客户端 → 服务端）
        let control = r#"{"type":"subscribe"}"#;
        let sealed = seal_client_text(0, "ws-terminal", control);
        assert_eq!(
            ws_decrypt_inbound_text("t:1", "ws-terminal", &sealed).unwrap(),
            control
        );

        // 重放上一帧 → seq mismatch 拒绝
        assert!(ws_decrypt_inbound_text("t:1", "ws-terminal", &sealed).is_err());

        // 篡改密文 → 解密失败（合法新序号帧上翻转 ct 末字节）
        let mut tampered: WsTextEnvelope = serde_json::from_slice(
            &seal_client_text(1, "ws-event", "x"),
        )
        .unwrap();
        let mut ct = b64_decode(&tampered.ct).unwrap();
        let last = ct.len() - 1;
        ct[last] ^= 0xFF;
        tampered.ct = b64_encode(&ct);
        let tampered = serde_json::to_vec(&tampered).unwrap();
        assert!(ws_decrypt_inbound_text("t:1", "ws-event", &tampered).is_err());

        // 二进制往返（客户端 → 服务端；TBv2 输出帧模拟。篡改帧解密失败不推进
        // 接收序号，故此处仍用 seq 1）
        let frame = vec![0xABu8; 37];
        let bin_seq = 1u64;
        let nonce = ws_nonce(&c2s_cipher.nonce_prefix, bin_seq);
        let ciphertext = crate::utils::crypto::aes_gcm::encrypt(
            &c2s_cipher.key,
            &nonce,
            &frame,
            Some(&ws_aad("ws-terminal", Direction::Inbound, ORIGIN_BINARY)),
        )
        .unwrap();
        let mut enc = Vec::with_capacity(WS_BINARY_HEADER_LEN + ciphertext.len());
        enc.push(WS_FRAME_VERSION);
        enc.extend_from_slice(&bin_seq.to_be_bytes());
        enc.extend_from_slice(&ciphertext);
        assert_eq!(enc[0], 1, "帧头版本字节");
        assert_eq!(
            ws_decrypt_inbound_binary("t:1", "ws-terminal", &enc).unwrap(),
            frame
        );

        // 服务端出站帧可被持有 s2c 密钥的客户端解开（跨端兼容锚点）
        let outbound = ws_encrypt_outbound_binary("t:1", "ws-terminal", &frame).unwrap();
        let out_seq = u64::from_be_bytes(outbound[1..9].try_into().unwrap());
        let out_plain = crate::utils::crypto::aes_gcm::decrypt(
            &s2c_cipher.key,
            &ws_nonce(&s2c_cipher.nonce_prefix, out_seq),
            &outbound[WS_BINARY_HEADER_LEN..],
            Some(&ws_aad("ws-terminal", Direction::Outbound, ORIGIN_BINARY)),
        )
        .unwrap();
        assert_eq!(out_plain, frame);

        ws_remove_ciphers("t:1");
        assert!(!ws_has_ciphers("t:1"), "断连清理后应无密码");
    }

    // ---------- 过滤器豁免与开关判定 ----------

    #[test]
    fn ws_local_channel_always_exempt() {
        let mut ctx = mk_ctx(TrafficChannel::WsLocal, "203.0.113.9:5000", "text");
        assert!(!LinkEncryptionFilter::should_process(&ctx));
        assert_eq!(
            LinkEncryptionFilter.on_inbound(&mut ctx),
            Verdict::Continue
        );
        assert_eq!(ctx.data, b"payload", "豁免流量不得改写");
    }

    #[test]
    fn loopback_peers_exempt_ipv4_and_ipv6() {
        for peer in ["127.0.0.1:51000", "[::1]:51000"] {
            let ctx = mk_ctx(TrafficChannel::Http, peer, "/api/sessions");
            assert!(!LinkEncryptionFilter::should_process(&ctx), "环回 {peer} 应豁免");
        }
    }

    #[test]
    fn unparseable_peer_treated_as_remote_fail_safe() {
        // peer 解析失败（如 "unknown"）不能当环回放行；需主开关开才进入处理判定
        with_all_enabled(|| {
            let ctx = mk_ctx(TrafficChannel::Http, "unknown", "/api/sessions");
            assert!(LinkEncryptionFilter::should_process(&ctx));
        });
    }

    #[test]
    fn auth_and_health_routes_whitelisted_for_http_only() {
        let _guard = SNAPSHOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        for route in [
            "/api/auth/pairing",
            "/api/auth/qr-connect",
            "/health",
            "/api/health",
        ] {
            let ctx = mk_ctx(TrafficChannel::Http, "192.168.1.50:4444", route);
            assert!(
                !LinkEncryptionFilter::should_process(&ctx),
                "白名单路由 {route} 应豁免"
            );
        }
        // 白名单只对 HTTP 生效；WS 帧类别叫 "text"/"binary"，天然不命中前缀。
        // WS 非豁免路径会读快照，先临时开启避免依赖其它用例留下的状态
        update_config(LinkCryptoConfig {
            enabled: true,
            ..LinkCryptoConfig::default()
        });
        let ws_ctx = mk_ctx(TrafficChannel::WsTerminal, "192.168.1.50:4444", "text");
        assert!(LinkEncryptionFilter::should_process(&ws_ctx));
        update_config(LinkCryptoConfig::default());
    }

    #[test]
    fn remote_non_whitelisted_processed_when_enabled() {
        with_all_enabled(|| {
            let ctx = mk_ctx(TrafficChannel::Http, "192.168.1.50:4444", "/api/sessions");
            assert!(LinkEncryptionFilter::should_process(&ctx));
            // 骨架期直通不改数据
            let mut mutable = ctx;
            assert_eq!(
                LinkEncryptionFilter.on_inbound(&mut mutable),
                Verdict::Continue
            );
            assert_eq!(mutable.data, b"payload");

            let term = mk_ctx(TrafficChannel::WsTerminal, "192.168.1.50:4444", "binary");
            assert!(LinkEncryptionFilter::should_process(&term));
            let event = mk_ctx(TrafficChannel::WsEvent, "192.168.1.50:4444", "text");
            assert!(LinkEncryptionFilter::should_process(&event));
        });
    }

    #[test]
    fn master_switch_off_blocks_everything() {
        let _guard = SNAPSHOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        update_config(LinkCryptoConfig::default()); // enabled=false
        let ctx = mk_ctx(TrafficChannel::Http, "192.168.1.50:4444", "/api/sessions");
        assert!(!LinkEncryptionFilter::should_process(&ctx));
    }

    #[test]
    fn per_channel_switch_gates_individually() {
        let _guard = SNAPSHOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let original = current_config();
        update_config(LinkCryptoConfig {
            enabled: true,
            encrypt_http: false,
            ..LinkCryptoConfig::default()
        });
        let http = mk_ctx(TrafficChannel::Http, "192.168.1.50:4444", "/api/sessions");
        assert!(!LinkEncryptionFilter::should_process(&http), "子开关关应跳过该通道");
        let term = mk_ctx(TrafficChannel::WsTerminal, "192.168.1.50:4444", "text");
        assert!(LinkEncryptionFilter::should_process(&term), "其余通道不受影响");
        update_config(original);
    }

    #[test]
    fn registration_into_fresh_chain_is_named_and_idempotent_per_instance() {
        let chain = TrafficFilterChain::new();
        register_into(&chain);
        assert_eq!(chain.list_names(), vec![FILTER_NAME.to_string()]);
        // 独立实例互不干扰；全局单例不被本测试污染
        let global_names = TrafficFilterChain::global().list_names();
        assert!(
            !global_names.iter().any(|n| n == FILTER_NAME),
            "单元测试不得触碰全局链"
        );
    }

    // ---------- HTTP 协议层（issue 02） ----------

    #[test]
    fn negotiation_parsing_accepts_only_v1_pair() {
        assert_eq!(parse_negotiation("v1 QUJD"), Some("QUJD"));
        assert_eq!(parse_negotiation("  v1   QUJD  "), Some("QUJD"), "容忍多余空白");
        assert_eq!(parse_negotiation("v2 QUJD"), None, "未知版本视作未协商");
        assert_eq!(parse_negotiation("v1"), None);
        assert_eq!(parse_negotiation("v1 A B"), None, "多段非法");
        assert_eq!(parse_negotiation("v1 "), None, "空公钥拒绝");
        assert_eq!(parse_negotiation(""), None);
    }

    #[test]
    fn http_codec_roundtrip_and_tamper_rejection() {
        let key = [5u8; 32];
        let aad = http_aad(Direction::Inbound, "/api/sessions");
        let plain = br#"{"ok":true,"n":42}"#.to_vec();

        let sealed = encrypt_http_body(&key, &plain, &aad).unwrap();
        assert_eq!(decrypt_http_body(&key, &sealed, &aad).unwrap(), plain);

        // 篡改密文任一字节 → GCM 校验失败
        let mut tampered = sealed.clone();
        let last = sealed.len() - 2; // 收尾引号前的 base64 字符必属 ct 字段
        tampered[last] = if tampered[last] == b'A' { b'B' } else { b'A' };
        assert!(decrypt_http_body(&key, &tampered, &aad).is_err());

        // AAD 不匹配（换路径）→ 拒绝：防信封跨端点搬运
        assert!(decrypt_http_body(&key, &sealed, &http_aad(Direction::Inbound, "/other")).is_err());
        // 方向互换 → 拒绝
        assert!(decrypt_http_body(&key, &sealed, &http_aad(Direction::Outbound, "/api/sessions")).is_err());

        // 版本不符 / nonce 长度不符
        let bad_version = br#"{"v":9,"n":"QUJD","ct":"QUJD"}"#;
        assert!(decrypt_http_body(&key, bad_version, &aad).is_err());
        let bad_nonce = br#"{"v":1,"n":"AAAA","ct":"AAAA"}"#;
        assert!(decrypt_http_body(&key, bad_nonce, &aad).is_err());
        // 非信封 JSON
        assert!(decrypt_http_body(&key, b"plain text", &aad).is_err());
    }

    #[test]
    fn request_key_cache_is_take_once_and_ttl_bounded() {
        let keys = HttpTrafficKeys { request: [1u8; 32], response: [2u8; 32] };
        store_http_keys("peer-a", "EK==", keys.clone());
        // 命中即取走
        assert_eq!(take_http_keys("peer-a", "EK=="), Some(keys.clone()));
        // 二次 miss（一次性）
        assert_eq!(take_http_keys("peer-a", "EK=="), None);

        // TTL 判定纯函数
        let now = Instant::now();
        let fresh = CachedHttpKeys { keys: keys.clone(), inserted_at: now };
        let stale = CachedHttpKeys { keys, inserted_at: now - REQUEST_KEY_TTL };
        assert!(!entry_expired(&fresh, now));
        assert!(entry_expired(&stale, now));
    }

    #[test]
    fn http_full_request_response_cycle_through_filter() {
        let dir = tempfile::tempdir().unwrap();
        init_identity(dir.path()).unwrap();

        with_all_enabled(|| {
            use crate::utils::crypto::x25519::{x25519_diffie_hellman, x25519_generate};

            // 客户端侧模拟：临时密钥对 + 对 Kd 公钥 ECDH 派生两方向密钥
            let client_eph = x25519_generate();
            let (_, kd_pub_b64) = identity_parts().unwrap();
            let kd_pub: [u8; 32] = b64_decode(&kd_pub_b64).unwrap().try_into().unwrap();
            let shared = x25519_diffie_hellman(&client_eph, &kd_pub).unwrap();
            let keys = derive_http_traffic_keys(shared.as_bytes(), "/api/sessions").unwrap();
            let negotiation = format!("v1 {}", b64_encode(client_eph.public()));

            // ---- 入站：加密请求体 → 过滤器解密为明文 ----
            let req_plain = br#"{"q":"list sessions"}"#.to_vec();
            let req_sealed =
                encrypt_http_body(&keys.request, &req_plain, &http_aad(Direction::Inbound, "/api/sessions"))
                    .unwrap();
            let mut inbound = FilterContext {
                channel: TrafficChannel::Http,
                direction: Direction::Inbound,
                peer: "192.168.1.9:55001",
                route: "/api/sessions",
                negotiation: &negotiation,
                data: req_sealed,
                outbound_headers: Vec::new(),
            };
            assert_eq!(LinkEncryptionFilter.on_inbound(&mut inbound), Verdict::Continue);
            assert_eq!(inbound.data, req_plain, "handler 应收到明文");

            // ---- 出站：明文响应 → 过滤器加密 → 客户端用 k_resp 解回 ----
            let resp_plain = br#"{"code":0,"data":[1,2]}"#.to_vec();
            let mut outbound = FilterContext {
                channel: TrafficChannel::Http,
                direction: Direction::Outbound,
                peer: "192.168.1.9:55001",
                route: "/api/sessions",
                negotiation: &negotiation,
                data: resp_plain.clone(),
                outbound_headers: Vec::new(),
            };
            assert_eq!(LinkEncryptionFilter.on_outbound(&mut outbound), Verdict::Continue);
            assert_ne!(outbound.data, resp_plain, "响应应已加密");
            // 加密响应必须回协商头标记（spec §4）：客户端据 `X-BedCode-Crypto: v1`
            // 识别并解密，缺失/值不符会导致客户端误判明文降级（值必须是 "v1"，
            // 与移动端 `respHeader === 'v1'` 判定一致，非裸 "1"）
            assert_eq!(
                outbound.outbound_headers,
                vec![(NEGOTIATION_HEADER.to_string(), format!("v{PROTOCOL_VERSION}"))],
                "加密响应应注入 X-BedCode-Crypto: v1 标记头"
            );
            let decrypted =
                decrypt_http_body(&keys.response, &outbound.data, &http_aad(Direction::Outbound, "/api/sessions"))
                    .unwrap();
            assert_eq!(decrypted, resp_plain);

            // ---- 缓存一次性：第二个同 peer/ek 响应无密钥可取 → 明文直通 ----
            let mut second = FilterContext {
                channel: TrafficChannel::Http,
                direction: Direction::Outbound,
                peer: "192.168.1.9:55001",
                route: "/api/sessions",
                negotiation: &negotiation,
                data: b"late".to_vec(),
                outbound_headers: Vec::new(),
            };
            assert_eq!(LinkEncryptionFilter.on_outbound(&mut second), Verdict::Continue);
            assert_eq!(second.data, b"late");
        });
    }

    #[test]
    fn http_fail_closed_paths() {
        let dir = tempfile::tempdir().unwrap();
        init_identity(dir.path()).unwrap();

        let _guard = SNAPSHOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let original = current_config();

        // ① 强加密模式（fallback=false）：未协商请求直接拒绝
        update_config(LinkCryptoConfig { enabled: true, allow_plaintext_fallback: false, ..LinkCryptoConfig::default() });
        let mut no_neg = mk_ctx(TrafficChannel::Http, "192.168.1.9:55002", "/api/sessions");
        assert!(matches!(
            LinkEncryptionFilter.on_inbound(&mut no_neg),
            Verdict::Reject(msg) if msg.contains("required")
        ));

        // ② 已协商但载荷损坏 → Reject 且原因含 decrypt failed
        update_config(LinkCryptoConfig { enabled: true, ..LinkCryptoConfig::default() });
        let client_eph = crate::utils::crypto::x25519::x25519_generate();
        let negotiation = format!("v1 {}", b64_encode(client_eph.public()));
        let mut garbage = FilterContext {
            channel: TrafficChannel::Http,
            direction: Direction::Inbound,
            peer: "192.168.1.9:55003",
            route: "/api/sessions",
            negotiation: &negotiation,
            data: b"definitely-not-an-envelope".to_vec(),
            outbound_headers: Vec::new(),
        };
        assert!(matches!(
            LinkEncryptionFilter.on_inbound(&mut garbage),
            Verdict::Reject(msg) if msg.contains("decrypt failed")
        ));

        // ③ 非法公钥（非 base64/长度错）→ Reject
        let mut bad_key = FilterContext {
            channel: TrafficChannel::Http,
            direction: Direction::Inbound,
            peer: "192.168.1.9:55004",
            route: "/api/sessions",
            negotiation: "v1 !!!not-base64!!!",
            data: vec![0u8; 16],
            outbound_headers: Vec::new(),
        };
        assert!(matches!(LinkEncryptionFilter.on_inbound(&mut bad_key), Verdict::Reject(_)));

        update_config(original);
    }

    /// GET/HEAD 无请求体协商：空 body + 协商头 → 直接通过并缓存响应密钥，
    /// 出站时响应以 k_resp 加密并回标记头（修复：移动端 GET 请求此前无 body
    /// 可加密但协商必须成立，否则响应密钥缓存缺失、响应加密失效）
    #[test]
    fn http_get_with_empty_body_negotiates_and_encrypts_response() {
        let dir = tempfile::tempdir().unwrap();
        init_identity(dir.path()).unwrap();

        with_all_enabled(|| {
            use crate::utils::crypto::x25519::{x25519_diffie_hellman, x25519_generate};

            let client_eph = x25519_generate();
            let (_, kd_pub_b64) = identity_parts().unwrap();
            let kd_pub: [u8; 32] = b64_decode(&kd_pub_b64).unwrap().try_into().unwrap();
            let shared = x25519_diffie_hellman(&client_eph, &kd_pub).unwrap();
            let keys = derive_http_traffic_keys(shared.as_bytes(), "/api/sessions").unwrap();
            let negotiation = format!("v1 {}", b64_encode(client_eph.public()));

            // 入站：空 body（GET）→ 通过，不解信封（无载荷可解）
            let mut inbound = FilterContext {
                channel: TrafficChannel::Http,
                direction: Direction::Inbound,
                peer: "192.168.1.9:55005",
                route: "/api/sessions",
                negotiation: &negotiation,
                data: Vec::new(),
                outbound_headers: Vec::new(),
            };
            assert_eq!(LinkEncryptionFilter.on_inbound(&mut inbound), Verdict::Continue);
            assert!(inbound.data.is_empty(), "空 body 应原样透传");

            // 出站：响应加密 + 标记头（客户端据头解密）
            let resp_plain = br#"{"code":0,"data":[]}"#.to_vec();
            let mut outbound = FilterContext {
                channel: TrafficChannel::Http,
                direction: Direction::Outbound,
                peer: "192.168.1.9:55005",
                route: "/api/sessions",
                negotiation: &negotiation,
                data: resp_plain.clone(),
                outbound_headers: Vec::new(),
            };
            assert_eq!(LinkEncryptionFilter.on_outbound(&mut outbound), Verdict::Continue);
            assert_ne!(outbound.data, resp_plain, "GET 响应应加密");
            assert_eq!(
                outbound.outbound_headers,
                vec![(NEGOTIATION_HEADER.to_string(), format!("v{PROTOCOL_VERSION}"))]
            );
            let decrypted =
                decrypt_http_body(&keys.response, &outbound.data, &http_aad(Direction::Outbound, "/api/sessions"))
                    .unwrap();
            assert_eq!(decrypted, resp_plain);
        });
    }
}
