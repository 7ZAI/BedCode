//! Terminal Session Center Plugin (WASM)
//!
//! 终端会话中心（`com.bedcode.terminal-session`）：把「谁能连 → 连上有什么 → 会话里跑什么」
//! 三域的产品语义收敛为单一权威插件（spec `.scratch/2026-09-19-terminal-session-plugin`）。
//!
//! 已落地：
//! - 票 03 骨架：可构建（wasm32-wasip3，cdylib 直出 Component）、可加载、可激活
//! - 票 04 pairing 域：配对码 / QR token 生命周期语义（[`pairing`]），宿主命令面
//!   与 server 配对端点经 `utils/auth/auth_center.rs` 桥接调本插件互调 api
//! - 票 05 trust / consent 域：设备信任统一视图与撤销（[`trust`]，真源 = 内核
//!   `pairings` 表经 host-auth 记录面）、首连确认决策（[`consent`]）、认证策略
//!   导出（[`policy`]）——`auth-policy` capability 一并改指本插件
//! - 票 08-10 会话语义：配置真源私有库（[`config`]）、创建编排（[`launch`]）、
//!   动作编排与尺寸裁决（[`actions`]）→ 票 11-12 注解槽与去任务字段
//! - 票 15 任务域后端①：Agent 集成与会话状态（[`task`]）——本插件**自己注册**
//!   生命周期 / 输入监听并主动推进集成注入（编排反转），旧 auto-task 后端仍在
//!   原位（contract 在票 17）；票 16 追加定时任务与 HTTP 面
//!
//! 形态：rust-ts（后端 WASM + 贡献式前端）；`sandbox: inline`；`kind` 取默认
//! Application（带 UI 贡献、必须可停可删，故不用 System 形态）。
//!
//! **能力来源红线（D2）**：本插件后端只经宿主 `host-*` 基础服务取能力，缺口一律在
//! 既有 WIT interface 上追加函数，禁止新开私有 import 接口或让宿主为单一插件特化。
//! 本票的能力映射：配对 / 密钥 = `host-auth`（secret-store + 记录面）；信任与同意 =
//! `host-auth` 记录面 + `host-peer`；会话配置（票 08）= `host-plugin-database`
//! （私有库真源，`storage` 权限）+ `host-session` 配置面（读 legacy 主库做迁移）；
//! 无 `fs` / `network` 面。
//!
//! **对称性说明（D7 故障半径）**：三域同实例后配对侧 trap 会连带会话/任务回调，
//! 故各域独立 `Result` 边界、互不持锁；域内状态锁只在单次操作内持有（无跨域锁）。

/// 会话动作域（票 10）：重启 / 移除 / 改名编排 + 尺寸正统端裁决
mod actions;
/// 认证记录域（2026-09-22 下沉）：配对设备 + 连接历史真源 = 本插件私有库，
/// 宿主 host-auth 记录面原语退役后本域自持（见模块文档）
pub mod auth_records;
/// 认证链 HTTP 编排域（票 07）：/api/auth/* 七端点 + JWT 签发 + 挑战状态机
/// （密钥托管与信任表留宿主，经 host-auth 原语回调统一认证，见模块文档）
pub mod auth_http;
/// 会话配置域（票 08）：真源在本插件私有库，见模块文档
pub mod config;
mod consent;
/// 设备与配对域命令面（票 14）：设备页前端的配对码 / QR / 网络信息 / 设备列表 /
/// 连接历史 / 有效期设置入口（非互调 api，不进 manifest.api）
mod device_face;
/// 设备派生视图（票 11）：在线判定 + 真实会话数 + 任务状态合并且注解槽写面
mod devices;
/// 执行环境平台事实（票 13）：WSL 发行版枚举（host-platform 原语）
mod environment;
/// 会话创建编排（票 09）：命名唯一化 / config→launch spec 映射 / 两阶段启动决策
mod launch;
/// 文件浏览域（票 03）：文件树 / 内容 / diff（host-fs + host-process，见模块文档）
pub mod file_browse;
/// 终端输出拉取域（票 04）：经 host-session output-ring-fetch 原语拉取会话输出字节
mod output;
mod pairing;
mod policy;
/// 快捷指令域（票 02 第 4 域）：私有库持久化 + HTTP 查询面 + 迁移导入
pub mod quick_actions;
/// 私有库表名域前缀统一与幂等重命名迁移（票 16 / spec D5）
pub mod schema;
/// 会话登记域（会话引擎整体下沉 P1）：会话真源自宿主 `session/` 迁入本插件，
/// 当前处于双写阶段（宿主仍是权威，见模块文档）
pub mod session;
/// 任务域（票 15-16）：Agent 集成与会话状态 + 队列/定时后端（见模块文档）
pub mod task;
/// 按键组合 → 转义字节翻译（票 06 下沉）：宿主 pty 只收裸字节，本插件自译自写
mod keys;
mod trust;

use bedcode_plugin_api::events::{InputSubmittedEvent, SessionLifecycleEvent};
use bedcode_plugin_api::host::{HostBus, HostLog, HostSession, HostStorage, HostTimer};
use bedcode_plugin_api::types::PluginManifest;
use bedcode_plugin_api::wasm::WasmPlugin;
use bedcode_plugin_api::wasm_host::WasmHost;
use bedcode_plugin_api::{plugin_api, BusMessage, CommandArgs};
use pairing::code::PairingCode;
use pairing::qr::QrTokenManager;
use session::model::SessionStatus;
use std::sync::{Mutex, OnceLock};

/// 插件互调 api 声明（ADR 0017）：trait 方法名 ↔ manifest.api 条目
/// （`com.bedcode.terminal-session.<method>`，`#[api(...)]` 覆盖为连字符名），宏在编译期
/// 比对防漂移。
///
/// api 面按域分组命名（spec D2「配对 / 信任 / 同意 / 会话 / 任务」）——票 05 落
/// pairing（八项）+ trust（两项）+ consent（一项）；session / task 归票 07 起。
/// `trust-list` 同时是宿主桥接的**探活锚点**（注册表含它 ⇔ 本插件已激活且互调
/// 面已登记，见宿主 `auth_center::SESSION_MARKER_API`）。
#[plugin_api(manifest = "../plugin.json")]
pub trait SessionApi {
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
    /// 文本（`QR token expired` / `QR token already used` / `No active QR token`），
    /// 宿主 `auth_controller::qr_connect` 按子串匹配分类用户提示
    #[api("qr-code-verify")]
    fn qr_code_verify(token: String) -> Result<serde_json::Value, String>;

    /// 清除当前 QR token
    #[api("qr-code-clear")]
    fn qr_code_clear() -> Result<(), String>;

    // ==================== 票 05：trust / consent 域 ====================

    /// 统一信任视图（pairing + peer 合并）→ `{ devices: TrustedDeviceDto[], peerError: string|null }`
    #[api("trust-list")]
    fn trust_list() -> Result<serde_json::Value, String>;

    /// 撤销统一条目 `{id}` → `{ removed, kind }`
    /// （pairing 走 host-auth 记录面软删内核 `pairings`，peer 走 host-peer）
    #[api("trust-revoke")]
    fn trust_revoke(id: String) -> Result<serde_json::Value, String>;

    /// 首连确认决策：peer 信息 + 可选用户意向 → 放行 / 拒绝 / 需确认
    /// （消费方 file-transfer 两阶段调用：阶段 1 无意向评估信任；阶段 2 回传意向）
    #[api("consent-decide")]
    fn consent_decide(
        request: consent::model::ConsentRequest,
    ) -> Result<consent::model::ConsentDecision, String>;

    // ==================== 票 08：会话配置域（真源 = 本插件私有库） ====================

    /// 配置列表（插件侧业务排序；宿主命令面据此薄转发）
    /// → `SessionConfig[]`（camelCase，与宿主 DTO 逐字同形）
    #[api("config-list")]
    fn config_list() -> Result<serde_json::Value, String>;

    /// 配置写入：`id` 缺省/空 → 新建（插件生成 UUID）；命中 → 覆盖；非空未命中 →
    /// 显性报错。入参为 `ConfigDraft`（camelCase），返回写入后的完整配置
    #[api("config-upsert")]
    fn config_upsert(draft: serde_json::Value) -> Result<serde_json::Value, String>;

    /// 配置删除 → 是否命中（未知 id 幂等 false）
    #[api("config-delete")]
    fn config_delete(id: String) -> Result<bool, String>;

    // ==================== 票 02：快捷指令域（第 4 域，私有库真源） ====================

    /// 一次性幂等导入（宿主 handoff 推送的 legacy 主库行）→ `ImportReport`
    /// （`{alreadyMigrated, imported, skippedExisting}`）。marker 已在 → 整体跳过
    /// （一次性语义，否则插件侧删除会被 legacy 行复活）。这是宿主侧
    /// `quick_actions_migration` 的唯一写入通道（spec 决策 6：不新增 host 原语）。
    #[api("quick-actions-import")]
    fn quick_actions_import(rows: serde_json::Value) -> Result<serde_json::Value, String>;

    // ==================== 2026-09-22 认证记录下沉：互调查询面 + 迁移导入 ====================

    /// 一次性幂等导入（宿主 handoff 推送的 legacy 主库 pairings / connection_history
    /// 行）→ `MigrationReport`（`{alreadyMigrated, importedPairings, importedHistory,
    /// credentialColumnsStripped, failed}`）。marker 已在 → 整体跳过。这是宿主侧
    /// `auth_records_migration` 的唯一写入通道（凭据列剥离由插件侧完成）。
    #[api("auth-records-import")]
    fn auth_records_import(rows: serde_json::Value) -> Result<serde_json::Value, String>;

    /// 活跃配对设备列表（`PairedDeviceInfo[]`，pairedAt 倒序）——其他插件经
    /// ADR 0017 互调查询认证中心获取配对记录。区别于 `trust-list`：只回配对
    /// 段（不含 peer），供设备页 / 文件传输等按设备寻址的消费方使用。
    #[api("devices-list")]
    fn devices_list() -> Result<serde_json::Value, String>;

    /// 设备连接历史（`device-id` = 配对记录 id；倒序）——其他插件查询认证中心
    /// 获取连接记录的互调面。
    #[api("history-list")]
    fn history_list(device_id: String) -> Result<serde_json::Value, String>;

    /// 连接计数 / last_seen 刷新（宿主 WS 认证路径回调：移动端持 JWT 开 WS 时
    /// 经此通知认证中心更新配对记录）。入参 `{ fingerprint }`。
    /// 插件未激活 → 宿主静默跳过（记录缺失不阻断认证，降级语义与旧
    /// `update_pairing_last_seen` 失败 warn 一致）。
    #[api("connection-touch")]
    fn connection_touch(fingerprint: String) -> Result<(), String>;

    /// 断开回填（宿主 WS 断链路径回调）：认证中心按指纹解析 device_id 并回填
    /// 最近一条 open 连接的断开时间。入参 `{ fingerprint }`。
    /// 插件未激活 → 宿主静默跳过（连接历史缺失不阻断断开语义）。
    #[api("connection-close")]
    fn connection_close(fingerprint: String) -> Result<(), String>;

    // ==================== 票 09：会话创建编排（命名唯一化 + launch spec + 两阶段） ====================

    /// 编排会话创建：命名唯一化 / config→launch spec 映射 / 两阶段启动决策在
    /// 本插件完成，再经 `host-session.create-with-spec` 交宿主执行。入参
    /// `{configId, cols?, rows?, start?}`（start 缺省 true）→ `{sessionId}`。
    #[api("session-create")]
    fn session_create(draft: serde_json::Value) -> Result<serde_json::Value, String>;

    // ==================== 票 10：会话动作（重启 / 移除 / 改名 / 尺寸裁决） ====================

    /// 重启会话：存在性预检（失败同步可见）→ `host-session.restart`（宿主异步执行，
    /// 同一 session id 重建并启动）。入参 `{sessionId}` → `{sessionId, name}`
    #[api("session-restart")]
    fn session_restart(draft: serde_json::Value) -> Result<serde_json::Value, String>;

    /// 移除会话：存在性预检 → `host-session.remove`（同步执行，失败可见）。
    /// 入参 `{sessionId}` → `{sessionId, removed}`
    #[api("session-remove")]
    fn session_remove(draft: serde_json::Value) -> Result<serde_json::Value, String>;

    /// 改名：`host-session.rename` → `{sessionId, name, previousName}`；
    /// 未知会话 / 空名显性报错
    #[api("session-rename")]
    fn session_rename(draft: serde_json::Value) -> Result<serde_json::Value, String>;

    /// 尺寸裁决 + 执行：读内核登记事实（`get.canonicalRenderer`）→ 插件侧裁决
    /// （正统端判定 / 覆盖确认策略）→ 仅在可应用时调 `host-session.resize`
    /// （只登记与执行）。入参 `{sessionId, cols, rows, requester, force?}`
    /// → `ResizeOutcome`（`{status:'applied', canonical}` |
    /// `{status:'needsConfirmation', currentCanonical}`）
    #[api("session-resize")]
    fn session_resize(draft: serde_json::Value) -> Result<serde_json::Value, String>;

    // ==================== 票 11：注解槽写面 + 设备派生视图 ====================

    /// 会话注解槽写入：入参 `{sessionId, key, value}`——expand 期双写的「写面」
    /// （旧任务字段面容不动，注解槽并行落值，spec D5）。宿主只做权限门 + 会话
    /// 存在性 + 参数形状校验后原样透传；键名语义归本插件（任务域键 `taskStatus` /
    /// `taskReason` 等），内核绝不解释。→ `{ok: true}`
    #[api("annotate")]
    fn annotate(draft: serde_json::Value) -> Result<serde_json::Value, String>;

    /// 设备派生视图：连接清单 = 在线判定 + 真实会话数 + 任务状态合并
    /// （spec D3「派生视图（在线判定 + 会话数 + 任务状态合并）」；替代宿主
    /// `get_connected_devices` 的硬编码 0）。原始事实（连接注册表 / 配对表 /
    /// 会话列表）全部来自宿主原语，本插件只做派生与组织。
    /// → `{connections: DerivedConnection[]}`
    #[api("devices-connect-list")]
    fn devices_connect_list() -> Result<serde_json::Value, String>;

    // ==================== 会话引擎下沉 P1：会话登记域读取面 ====================

    /// 全部会话的对外视图 → `{sessions: SessionInfoView[]}`（按 `createdAt, id` 稳定序）。
    /// 形状与宿主 `SessionInfoView` 逐字段一致（产出口在 `session::view`，有形状锁）。
    #[api("session-list")]
    fn session_list() -> Result<serde_json::Value, String>;

    /// 单个会话的对外视图；入参 `{sessionId}` → `SessionInfoView` | `null`（不在册）
    #[api("session-get")]
    fn session_get(draft: serde_json::Value) -> Result<Option<serde_json::Value>, String>;

    // ==================== 会话引擎下沉 P1-b：停止 / 输入写入（真源切换新增） ====================

    /// 停止会话：登记 `Stopping` + 发起 `host-pty.kill`（终态由 `pty:exit` 事件
    /// 收尾并广播 `SessionStopped`）。入参 `{sessionId}` → `{sessionId, stopped}`；
    /// 已终态 / 并发停止在途 → 幂等成功。
    #[api("session-close")]
    fn session_close(draft: serde_json::Value) -> Result<serde_json::Value, String>;

    /// 写入输入：`{sessionId, data, special?}`；`special = true` 时绕过提交行重建
    /// 直接写字节（对齐内核 `send_special_key` 的不对称），默认走提交行重建 +
    /// 任务域观察。
    #[api("session-input")]
    fn session_input(draft: serde_json::Value) -> Result<serde_json::Value, String>;
}

/// 终端会话中心插件 — 生命周期 + 状态命令 + pairing 互调 api
pub struct SessionPlugin;

/// 配对码状态（跨调用持久；wasip3 的 thread_local 是真 TLS，实例状态必须
/// static Mutex —— 教训见 devices 线 handoff §3）
static CURRENT_CODE: Mutex<Option<PairingCode>> = Mutex::new(None);

/// QR token 管理器状态（OnceLock：仅运行时首次访问初始化一次，返回 &'static）
static QR_MANAGER: OnceLock<QrTokenManager> = OnceLock::new();

impl SessionApi for SessionPlugin {
    fn pairing_code_generate(ttl: u64) -> Result<serde_json::Value, String> {
        pair_code_generate(ttl)
    }

    fn pairing_code_status() -> Result<Option<serde_json::Value>, String> {
        pair_code_status()
    }

    fn pairing_code_verify(code: String) -> Result<bool, String> {
        pair_code_verify(&code)
    }

    fn pairing_code_clear() -> Result<(), String> {
        *CURRENT_CODE
            .lock()
            .map_err(|e| format!("pairing code lock: {e}"))? = None;
        Ok(())
    }

    fn qr_code_generate(ttl: u64) -> Result<serde_json::Value, String> {
        qr_generate(ttl)
    }

    fn qr_code_status() -> Result<Option<serde_json::Value>, String> {
        qr_status()
    }

    fn qr_code_verify(token: String) -> Result<serde_json::Value, String> {
        qr_verify(&token)
    }

    fn qr_code_clear() -> Result<(), String> {
        qr_manager().clear();
        Ok(())
    }

    // ==================== 票 05：trust / consent 域 ====================

    fn trust_list() -> Result<serde_json::Value, String> {
        trust::list_via_host()
    }

    fn trust_revoke(id: String) -> Result<serde_json::Value, String> {
        trust::revoke_via_host(&id)
    }

    fn consent_decide(
        request: consent::model::ConsentRequest,
    ) -> Result<consent::model::ConsentDecision, String> {
        consent::ops::decide_consent_via_host(request)
    }

    // ==================== 票 08：会话配置域 ====================

    fn config_list() -> Result<serde_json::Value, String> {
        config::list_via_host()
    }

    fn config_upsert(draft: serde_json::Value) -> Result<serde_json::Value, String> {
        config::upsert_via_host(draft)
    }

    fn config_delete(id: String) -> Result<bool, String> {
        config::delete_via_host(&id)
    }

    // ==================== 票 02：快捷指令域（第 4 域） ====================

    fn quick_actions_import(rows: serde_json::Value) -> Result<serde_json::Value, String> {
        quick_actions::import_via_host(rows)
    }

    // ==================== 2026-09-22 认证记录下沉：互调查询面 + 迁移导入 ====================

    fn auth_records_import(rows: serde_json::Value) -> Result<serde_json::Value, String> {
        auth_records::import_via_host(rows)
    }

    fn devices_list() -> Result<serde_json::Value, String> {
        auth_records::paired_list()
    }

    fn history_list(device_id: String) -> Result<serde_json::Value, String> {
        auth_records::history_list(&device_id)
    }

    fn connection_touch(fingerprint: String) -> Result<(), String> {
        auth_records::touch(&fingerprint)
    }

    fn connection_close(fingerprint: String) -> Result<(), String> {
        auth_records::close_open_connection(&fingerprint)
    }

    // ==================== 票 09：会话创建编排 ====================

    fn session_create(draft: serde_json::Value) -> Result<serde_json::Value, String> {
        launch::create_via_host(&draft)
    }

    // ==================== 票 10：会话动作 ====================

    fn session_restart(draft: serde_json::Value) -> Result<serde_json::Value, String> {
        actions::restart_via_host(&draft)
    }

    fn session_remove(draft: serde_json::Value) -> Result<serde_json::Value, String> {
        actions::remove_via_host(&draft)
    }

    fn session_rename(draft: serde_json::Value) -> Result<serde_json::Value, String> {
        actions::rename_via_host(&draft)
    }

    fn session_resize(draft: serde_json::Value) -> Result<serde_json::Value, String> {
        actions::resize_via_host(&draft)
    }

    // ==================== 票 11：注解槽写面 + 设备派生视图 ====================

    fn annotate(draft: serde_json::Value) -> Result<serde_json::Value, String> {
        let session_id = draft
            .get("sessionId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "sessionId required".to_string())?;
        let key = draft
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "key required".to_string())?;
        let value = draft.get("value").and_then(|v| v.as_str()).unwrap_or("");
        devices::annotate_via_host(session_id, key, value)?;
        Ok(serde_json::json!({ "ok": true }))
    }

    fn devices_connect_list() -> Result<serde_json::Value, String> {
        devices::connect_list_via_host()
    }

    // ==================== 会话引擎下沉 P1：会话登记域读取面 ====================

    fn session_list() -> Result<serde_json::Value, String> {
        session::list_views_via_host()
    }

    fn session_get(draft: serde_json::Value) -> Result<Option<serde_json::Value>, String> {
        let session_id = draft
            .get("sessionId")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "session-get: sessionId required".to_string())?;
        session::view_via_host(session_id)
    }

    // ==================== 会话引擎下沉 P1-b：停止 / 输入写入 ====================

    fn session_close(draft: serde_json::Value) -> Result<serde_json::Value, String> {
        actions::close_via_host(&draft)
    }

    fn session_input(draft: serde_json::Value) -> Result<serde_json::Value, String> {
        let session_id = draft
            .get("sessionId")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "session-input: sessionId required".to_string())?;
        let data = draft
            .get("data")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        // 票 06：特殊键以组合串（specialKey）下发，本插件自译自写；
        // 缺省则按普通输入处理（data 内容）。
        let special_key = draft
            .get("specialKey")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());
        session::input_via_pty(session_id, data, special_key)?;
        Ok(serde_json::json!({ "sessionId": session_id }))
    }
}

// ==================== pairing 域核心（api 面与后续命令面共享同一状态源） ====================

/// 生成配对码（宿主 `PairingCode` serde 形状：code / created_at / expires_in 剩余秒）
fn pair_code_generate(ttl: u64) -> Result<serde_json::Value, String> {
    let now = pairing::jwt::now_secs();
    let code = PairingCode::generate_with_ttl_at(ttl, now);
    *CURRENT_CODE
        .lock()
        .map_err(|e| format!("pairing code lock: {e}"))? = Some(code.clone());
    serde_json::to_value(code).map_err(|e| format!("pairing code serialize: {e}"))
}

/// 当前配对码状态（过滤过期）→ null | 宿主 `PairingCode` 形状
fn pair_code_status() -> Result<Option<serde_json::Value>, String> {
    let now = pairing::jwt::now_secs();
    let guard = CURRENT_CODE
        .lock()
        .map_err(|e| format!("pairing code lock: {e}"))?;
    match guard.as_ref().filter(|c| !c.is_expired_at(now)) {
        Some(code) => serde_json::to_value(code)
            .map(Some)
            .map_err(|e| format!("pairing code serialize: {e}")),
        None => Ok(None),
    }
}

/// 验证配对码（一次性：成功即消耗；过期顺带清除）→ valid 布尔
fn pair_code_verify(input: &str) -> Result<bool, String> {
    let mut guard = CURRENT_CODE
        .lock()
        .map_err(|e| format!("pairing code lock: {e}"))?;
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

/// 生成 QR token → `{token, ttl, remaining}`
fn qr_generate(ttl: u64) -> Result<serde_json::Value, String> {
    let manager = qr_manager();
    let token = manager.generate(ttl);
    let (_, ttl, remaining) = manager.get_active().expect("fresh token active");
    Ok(serde_json::json!({ "token": token, "ttl": ttl, "remaining": remaining }))
}

/// 当前 QR token 状态（过滤过期/已用）→ null | `{token, ttl, remaining}`
fn qr_status() -> Result<Option<serde_json::Value>, String> {
    let active = qr_manager().get_active();
    Ok(active.map(|(token, ttl, remaining)| serde_json::json!({ "token": token, "ttl": ttl, "remaining": remaining })))
}

/// 验证 QR token（一次性：成功即清除）→ `{valid, reason?}`
fn qr_verify(input: &str) -> Result<serde_json::Value, String> {
    match qr_manager().verify(input) {
        Ok(()) => Ok(serde_json::json!({ "valid": true })),
        Err(e) => Ok(serde_json::json!({ "valid": false, "reason": e.message() })),
    }
}

impl WasmPlugin for SessionPlugin {
    const ID: &'static str = "com.bedcode.terminal-session";

    fn manifest() -> PluginManifest {
        // ADR-0005 单一真源：plugin.json（与 `#[plugin_api]` 防漂移比对同一份）
        serde_json::from_str(include_str!("../../plugin.json"))
            .expect("plugin.json must be valid PluginManifest")
    }

    fn activate() -> anyhow::Result<()> {
        let host = WasmHost;
        host.log_info("Terminal Session Center plugin activated");
        // 订阅互调请求 topic（宏生成）：`bedcode.api.<api>` 逐个订阅，宿主去重幂等
        SessionApiDispatcher::register()?;
        // 密钥托管探活（host-auth secret-store；明文不落日志，只记存在性与长度）。
        // 失败不阻断激活：凭据按需生成，认证路径显性报错而非静默降级（D7 分段语义）
        match pairing::keys::jwt_key_from_host_auth() {
            Ok(key) => host.log_info(&format!(
                "jwt key ready via host-auth secret-store (len {})",
                key.len()
            )),
            Err(e) => host.log_warn(&format!("jwt key unavailable at activate: {}", e)),
        }
        // 票 16（spec D5）：私有库表名前缀统一迁移——**必须在任何建表之前**跑。
        // 顺序是本票唯一的静默风险点：先建后改会让统一名以空表先占位，迁移因
        // 「新旧名同时存在」跳过该条，旧表里的真实数据留在无人读的名字下（现象是
        // 升级后列表变空而不是报错）。失败只降级：不改名则各域仍按旧名读写
        // （票 15 的实现已随旧名建表），产品面可用但命名不统一，warn 留痕。
        match schema::migrate_via_host() {
            Ok(report) => {
                if !report.renamed.is_empty() {
                    host.log_info(&format!(
                        "private schema renamed to domain prefixes: {:?} (indexes dropped: {})",
                        report.renamed, report.indexes_dropped
                    ));
                }
                if !report.ambiguous.is_empty() {
                    host.log_warn(&format!(
                        "private schema has both legacy and prefixed names, untouched: {:?} \
                         (需人工判定合并方向，见票 16)",
                        report.ambiguous
                    ));
                }
            }
            Err(e) => host.log_warn(&format!(
                "private schema prefix migration failed (names left as-is): {}",
                e
            )),
        }
        // 票 08：配置真源在本插件私有库——建表（幂等）。
        //
        // **失败不阻断激活**（与上方密钥探活同口径，D7 故障隔离）：配置存储不可用时
        // 只降级配置面——配对 / 信任 / 同意 / auth-policy 必须照常工作，不能让一个
        // 「私有库建不出来」把整个插件（连带认证路径）打成 Degraded。宿主侧对此有
        // 降级轨兜底（命令面回落主库投影，见 spec 票 08 §4），故配置面故障 ≠ 产品面故障。
        //
        // **v24**：legacy 迁移通道（host-session config-list/get 读取面）随
        // `session_configs` 表退役删除——私有库即真源，无迁移步骤。
        match config::ensure_schema_via_host() {
            Ok(()) => host.log_info("session config store ready (no legacy migration; private store is source of truth)"),
            Err(e) => host.log_warn(&format!(
                "session config schema init failed at activate (config face degraded): {}",
                e
            )),
        }
        // 会话引擎整体下沉 P1-b（`.scratch/2026-09-23-session-engine-downsink`）：
        // 会话登记域建表（幂等）+ 进程启动对账（清空上一进程遗留的会话行——会话与
        // PTY 同生命周期，同为进程内存）。
        //
        // **P1-b 起失败必须阻断激活**：本域已是会话真源，建表失败 = 会话面不可用，
        // 继续激活只会让创建 / 停止 / 输入全线报「存储不可用」的半生不熟状态——
        // 显性失败让宿主把插件标记为不可用，用户路径立刻可见（不再降级镜像）。
        session::ensure_schema_via_host().map_err(|e| {
            anyhow::anyhow!("session registry schema init failed at activate (session face unavailable): {e}")
        })?;
        host.log_info("session registry store ready (private tables sessions / session_annotations)");
        // 票 02：快捷指令域（第 4 域）建表（幂等）。与配置面同口径——失败只降级
        // 快捷指令面：配对 / 信任 / 会话 / 任务必须照常工作。迁移数据由宿主侧
        // handoff（quick_actions_migration）经互调 api 推送，不在此拉取。
        match quick_actions::ensure_schema_via_host() {
            Ok(()) => host.log_info("quick action store ready (private table quick_actions)"),
            Err(e) => host.log_warn(&format!(
                "quick action schema init failed at activate (quick action face degraded): {}",
                e
            )),
        }
        // 2026-09-22 认证记录下沉：认证记录域（配对设备 + 连接历史）建表（幂等）。
        // 与配置面同口径——失败只降级认证记录域：配对 / 信任 / 认证链按「无记录」
        // 降级（撤销是唯一显式拒绝信号，读取失败从宽放行），不阻断激活（D7）。
        // 存量数据由宿主侧 handoff（auth_records_migration）经互调 api 推送，不在此拉取。
        match auth_records::ensure_schema_via_host() {
            Ok(()) => host.log_info(
                "auth records store ready (private tables auth_pairings / auth_connection_history)",
            ),
            Err(e) => host.log_warn(&format!(
                "auth records schema init failed at activate (auth records face degraded): {}",
                e
            )),
        }
        // 任务域建表（幂等，六张表：task_history / task_session_mapping /
        // task_session_settings / task_queue / task_preset / task_scheduled）。
        // 与配置面同口径——失败只降级任务域：配对 / 信任 / 会话面必须照常工作
        // （D7 故障隔离，不让任务表建不出来把整个插件连带认证路径打成 Degraded）。
        match task::ensure_schema_via_host(&host) {
            Ok(()) => host.log_info(
                "task domain schema ready (task_history / task_session_mapping / \
                 task_session_settings / task_queue / task_preset / task_scheduled)",
            ),
            Err(e) => host.log_warn(&format!(
                "task domain schema init failed at activate (task face degraded): {}",
                e
            )),
        }
        // 票 16 启动恢复（ADR 0003）：上次进程退出前处于 creating 态的定时任务，其
        // 会话已随进程销毁、Created 事件永不到达（新进程的会话不属于该任务），
        // 直接终结避免永久卡死。放在建表之后、定时器注册之前（表不在就无从更新）。
        match task::scheduled::recover_creating_jobs(&host) {
            Ok(count) if count > 0 => host.log_warn(&format!(
                "recover_creating_jobs: {} job(s) marked failed (session lost on restart)",
                count
            )),
            Ok(_) => host.log_debug("recover_creating_jobs: no interrupted scheduled job"),
            Err(e) => host.log_warn(&format!(
                "scheduled job recovery failed at activate (scheduled face degraded): {}",
                e
            )),
        }

        // 票 03（会话引擎下沉）：**不再注册宿主的两条观察面**。
        //
        // 历史形态是插件在 activate 里调 `host-session.lifecycle-register` /
        // `input-register`，宿主把「会话生命周期」与「用户提交的输入行」回调回来。
        // P1-b 真源下沉后这两条通道的**生产流量已归零**：创建/终态由本插件自驱
        // （`launch::spawn_session` 先行 Creating/Created、`<owner>::pty:exit` 驱动终态），
        // 提交行重建在本域 `session::input_via_pty` 内完成——回调只剩宿主内核直连路径
        // （测试）能触发。宿主侧的注册表 / 派发点 / 内核逐帧输入修饰链随票 03 一并删除，
        // 留着「代码在、永远不触发」正是下一处断链的种子。
        //
        // 导出面（`events.on-session-lifecycle` / `events.on-input-submitted` /
        // `terminal-hooks`）在 WIT 里**只登记不动**：interface 级删除与 ABI bump
        // 统一在票 10 定稿（契约硬约束只 bump 一次）。

        // 会话引擎下沉 P1-b：订阅本属主私有 PTY 退出事件（`<owner>::pty:exit`）——
        // 会话终态（自然退出 / kill）由该事件驱动（见 `session::on_pty_exit`）。
        // **失败必须阻断激活**：订阅不到 = 会话终态收尾失效（会话永久卡 Running
        // / Stopping，任务域同步卡 in_progress）——这是真源能力，不是可降级面。
        host.bus_subscribe(&bedcode_plugin_api::host::pty_event_topic(
            bedcode_plugin_api::host::PTY_EXIT,
            Self::ID,
        ))
        .map_err(|e| {
            anyhow::anyhow!("pty exit subscription failed (session lifecycle driver): {e}")
        })?;
        host.log_info("pty exit event subscribed (session lifecycle driver)");

        // 任务域定时器（清单第 4 项：定时器属本域能力面）：驱动队列「延迟 clear
        // 到点发送」与「执行中静默超时」两个周期步骤。回调按命令名分域计数失败，
        // 单域失败只降级本域（D7）——票 16 把定时任务域并入同一 tick 的分发表。
        match host.timer_register(task::SCHEDULER_INTERVAL_SECS, task::SCHEDULER_TICK_COMMAND) {
            Ok(()) => host.log_info(&format!(
                "task scheduler timer registered: interval={}s",
                task::SCHEDULER_INTERVAL_SECS
            )),
            Err(e) => host.log_warn(&format!(
                "task scheduler timer registration failed (queue delay face degraded): {}",
                e
            )),
        }
        Ok(())
    }

    fn deactivate() -> anyhow::Result<()> {
        let host = WasmHost;
        host.log_info("Terminal Session Center plugin deactivated");
        // 票 15：插件停用时撤离所有项目的 Agent 集成（与旧插件同语义）——残留集成
        // 在插件停用后仍会被 agent 调用，指向已停止的端点。
        let result = task::hooks::cleanup_all_agent_integrations(&host);
        host.log_info(&format!(
            "Agent integration cleanup on deactivate: cleaned={}, skipped={}, failed={}",
            result.cleaned, result.skipped, result.failed
        ));
        Ok(())
    }

    /// 认证策略导出（票 05 随 trust 自认证中心搬入）：宿主 server 中间件验签后
    /// 取本插件策略。结构/claims/时效策略 + 信任撤销检查（见 `policy` 模块）；
    /// 默认拒绝由 SDK 提供，本插件覆盖为真实策略。
    fn verify_device_token_policy(token: &str) -> Result<String, String> {
        policy::verify_device_token(token)
    }

    /// 总线消息入口：互调请求先经宏生成的分派器（命中 api topic 则处理并回复）；
    /// 本属主私有 `pty:exit` 事件（会话终态驱动）在此分流处理。
    fn on_message(msg: &BusMessage) -> anyhow::Result<()> {
        // pty:exit（引擎按属主投递；topic = `<owner>::pty:exit`，payload
        // `{ ptyId, reason, exitCode? }` camelCase）→ 会话终态收尾
        if msg.topic
            == bedcode_plugin_api::host::pty_event_topic(bedcode_plugin_api::host::PTY_EXIT, Self::ID)
        {
            let pty_id = msg
                .payload
                .get("ptyId")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let reason = msg
                .payload
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("stopped")
                .to_string();
            let exit_code = msg
                .payload
                .get("exitCode")
                .and_then(|v| v.as_i64())
                .map(|c| c as i32);
            if !pty_id.is_empty() {
                session::on_pty_exit(&pty_id, &reason, exit_code);
            }
            return Ok(());
        }
        SessionApiDispatcher::dispatch::<Self>(msg)?;
        Ok(())
    }

    /// 会话生命周期回调（票 15 编排反转；**P1-b 起为兼容面**）
    ///
    /// 宿主 `SessionManager` 不再产生会话生命周期事件（创建/停止/退出全部由本
    /// 插件自驱：Creating 在 `launch::spawn_session` 内先行、Created 随后、终态
    /// 由 `<owner>::pty:exit` 事件驱动）。本回调只剩**内核直连路径**（测试）与
    /// 旧通道的观察面：各分支只做登记域对账（会话不在册则跳过），不驱动主流程。
    fn on_session_lifecycle(event: &SessionLifecycleEvent) -> anyhow::Result<()> {
        match event {
            // Creating：agent 集成已由本插件自驱（launch::spawn_session 先于
            // spawn），此处无需处理——保留匹配以免误判未知变体
            SessionLifecycleEvent::Creating { .. } => Ok(()),
            // Created：定时任务域的会话就绪信号（按 session_id 精确匹配 creating
            // 态的 job）；兼容面里同时补发待处理的重启前端事件与登记对账
            SessionLifecycleEvent::Created {
                session_id,
                config_id,
                ..
            } => {
                let host = WasmHost;
                host.log_debug(&format!(
                    "on_session_lifecycle: Created event session_id={} config_id={}",
                    session_id, config_id
                ));
                actions::flush_pending_restart(&host, session_id);
                task::scheduled::handle_session_created(&host, session_id, config_id);
                let _ = session::note_status(session_id, SessionStatus::Running);
                Ok(())
            }
            // Stopped：任务域意外退出兜底（agent 的 Stop hook 没机会推送终态时，
            // in_progress / asking 的任务行与队列项会永久卡在运行中）
            SessionLifecycleEvent::Stopped { session_id, .. } => {
                let host = WasmHost;
                host.log_debug(&format!(
                    "on_session_lifecycle: Stopped event session_id={}",
                    session_id
                ));
                task::state::interrupt_running_tasks_on_session_end(&host, session_id);
                let _ = session::note_status(session_id, SessionStatus::Stopped);
                Ok(())
            }
            // 其余变体：`Stopping` 时把登记域记录推到过渡态；本域不关心的变体保持忽略
            other => {
                if let SessionLifecycleEvent::Stopping { session_id, .. } = other {
                    let _ = session::note_status(session_id, SessionStatus::Stopping);
                }
                Ok(())
            }
        }
    }

    /// 提交输入行回调（票 15）：把用户提交的输入当作任务记录写入本域真源
    ///
    /// 作为**纯观察通知**，任何一步失败都只降级本域（记日志返回，不向上抛错——
    /// 回调出错不影响输入本身，D7）。
    /// 提交输入行回调（票 15；P1-b 起为兼容面）：把用户提交的输入当作任务记录
    ///
    /// 生产路径已改走本插件 `session-input` 互调 api（提交行重建在插件侧完成，
    /// 直接调 [`task::state::handle_submitted_input`]）；本回调只剩宿主内核创建
    /// 路径（测试）与旧输入通道的观察面，与 `session-input` 共享同一过滤链。
    /// **纯观察通知**：失败只降级任务域（记日志返回，不向上抛错，D7）。
    fn on_input_submitted(event: &InputSubmittedEvent) -> anyhow::Result<()> {
        task::state::handle_submitted_input(&WasmHost, &event.session_id, &event.text);
        Ok(())
    }

    fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        match name {
            // 状态命令：宿主闭环测试据此断言产物可加载可激活、manifest 声明生效
            "session.status" => {
                let manifest = Self::manifest();
                Ok(serde_json::json!({
                    "plugin": Self::ID,
                    "version": env!("CARGO_PKG_VERSION"),
                    "domains": ["pairing", "trust", "consent", "config", "session", "devices", "environment", "task"],
                    "permissions": manifest.permissions,
                    "api": manifest.api,
                    // P1 双写期诊断：会话登记域镜像规模（读取失败只降级该字段）
                    "sessionRegistry": session::diagnostics_via_host(),
                }))
            }

            // ==================== 票 05 命令面（authoring 期桥接与闭环测试入口） ====================
            // 与互调 api 面共享同一实现（`*_via_host`）：宿主命令面未来经 api 转发，
            // 命令面保留供宿主闭环测试与调试直调。

            // 统一信任视图 → {devices, peerError}
            "session.trust.list" => trust::list_via_host().map_err(anyhow::Error::msg),

            // 撤销统一条目 {id} → {removed, kind}
            "session.trust.revoke" => {
                let id = args
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("id required"))?;
                trust::revoke_via_host(id).map_err(anyhow::Error::msg)
            }

            // 首连确认决策：args 即 ConsentRequest（camelCase wire）
            "session.consent.decide" => {
                let request: consent::model::ConsentRequest = serde_json::from_value(args)
                    .map_err(|e| anyhow::anyhow!("invalid consent request: {}", e))?;
                let decision =
                    consent::ops::decide_consent_via_host(request).map_err(anyhow::Error::msg)?;
                serde_json::to_value(decision)
                    .map_err(|e| anyhow::anyhow!("decision serialize: {}", e))
            }

            // ==================== 票 08 命令面（配置真源在插件私有库） ====================
            // 宿主命令面（create/list/get/delete/update）改薄转发到这三条；
            // 命令面同时供宿主闭环测试直调（不经总线）。

            // 配置列表 → SessionConfig[]（camelCase）
            "session.config.list" => config::list_via_host().map_err(anyhow::Error::msg),

            // 配置写入 {id?, name?, environment?, wslDistro?, workingDir?, command?, autoStart?}
            // → 写入后的完整配置；未知 id 显性报错
            "session.config.upsert" => config::upsert_via_host(args).map_err(anyhow::Error::msg),

            // 配置删除 {id} → {removed}
            "session.config.delete" => {
                let id = args
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("id required"))?;
                config::delete_via_host(id)
                    .map(|removed| serde_json::json!({ "removed": removed }))
                    .map_err(anyhow::Error::msg)
            }

            // ==================== 票 13 命令面（会话页前端取数） ====================
            // 会话页迁入本插件前端（P3）：创建 / 停止 / 环境事实三条命令面无互调 api
            // 对应（宿主命令面是给宿主 UI 的兼容接缝，插件前端只走这里）。

            // 创建会话（两阶段与映射编排在 launch 域）{configId, cols?, rows?, start?}
            // → {sessionId}
            "session.create" => launch::create_via_host(&args).map_err(anyhow::Error::msg),

            // 停止会话 {sessionId} → {sessionId, stopped}
            "session.close" => actions::close_via_host(&args).map_err(anyhow::Error::msg),

            // WSL 发行版枚举 → {distros: string[]}（宿主无 WSL 时显性报错）
            "session.environment.wsl-distros" => {
                environment::wsl_distros_via_host().map_err(anyhow::Error::msg)
            }

            // ==================== 票 10 命令面（会话动作：宿主命令面薄转发 + 闭环测试入口） ====================
            // 宿主命令面（restart/delete/resize）经桥接转发到这四条 api；命令面保留
            // 供宿主闭环测试直调（不经总线），与本插件互调 api 共享同一实现。

            // 重启 {sessionId} → {sessionId, name}
            "session.action.restart" => {
                actions::restart_via_host(&args).map_err(anyhow::Error::msg)
            }

            // 移除 {sessionId} → {sessionId, removed}
            "session.action.remove" => actions::remove_via_host(&args).map_err(anyhow::Error::msg),

            // 改名 {sessionId, name} → {sessionId, name, previousName}
            "session.action.rename" => actions::rename_via_host(&args).map_err(anyhow::Error::msg),

            // 尺寸裁决 {sessionId, cols, rows, requester, force?} → ResizeOutcome
            "session.action.resize" => actions::resize_via_host(&args).map_err(anyhow::Error::msg),

            // ==================== 票 04 命令面（终端输出数据面） ====================
            // 输出环拉取 {sessionId, fromOffset, maxBytes?} → null | {data, nextOffset,
            // truncated}。经 host-session.output-ring-fetch 原语（WIT list<u8> 直传）
            // 拉会话输出原始字节；游标由前端自持（slow consumer 只损失自己的历史）。
            "session.output.pull" => output::pull_via_host(&args).map_err(anyhow::Error::msg),

            // ==================== 票 11 命令面（注解槽写面 + 设备派生视图） ====================
            // 与互调 api 面共享同一实现（`*_via_host`）；命令面保留供宿主闭环测试直调。

            // 注解槽写入 {sessionId, key, value} → {ok: true}（expand 期双写的写面）
            "session.annotate" => {
                let session_id = args
                    .get("sessionId")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("sessionId required"))?;
                let key = args
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("key required"))?;
                let value = args.get("value").and_then(|v| v.as_str()).unwrap_or("");
                devices::annotate_via_host(session_id, key, value).map_err(anyhow::Error::msg)?;
                Ok(serde_json::json!({ "ok": true }))
            }

            // 设备派生视图：连接清单 → {connections: [...]}（在线判定 + 会话数 + 任务状态）
            "session.devices.connect-list" => {
                devices::connect_list_via_host().map_err(anyhow::Error::msg)
            }

            // ==================== 票 14 命令面（设备与配对页 / 设置分组前端取数） ====================
            // 设备页与设置分组迁入本插件前端（P3）：配对码、QR、网络信息、设备列表、
            // 连接历史与有效期设置**无互调 api 对应**（宿主命令面是给宿主 UI 的兼容
            // 接缝，插件前端只走这里）。配对码 / QR 复用 crate 根状态机；两项有效期
            // 读 host-config、写 host-auth（键白名单同两键）。

            // 生成配对码（TTL 取自认证域设置项）→ PairingCode 形状
            "session.pairing.generate" => {
                device_face::pairing_generate_via_host().map_err(anyhow::Error::msg)
            }

            // 当前配对码（过滤过期）→ PairingCode | null
            "session.pairing.status" => {
                device_face::pairing_status_via_host().map_err(anyhow::Error::msg)
            }

            // 清除配对码 → {cleared: true}
            "session.pairing.clear" => {
                device_face::pairing_clear_via_host().map_err(anyhow::Error::msg)?;
                Ok(serde_json::json!({ "cleared": true }))
            }

            // 生成 QR 连接信息 {host?} → {host, port, token, remainingSecs}
            "session.qr.generate" => {
                let host = args.get("host").and_then(|v| v.as_str());
                device_face::qr_generate_via_host(host).map_err(anyhow::Error::msg)
            }

            // 当前 QR 连接信息 {host?} → 同形 | null
            "session.qr.info" => {
                let host = args.get("host").and_then(|v| v.as_str());
                device_face::qr_info_via_host(host).map_err(anyhow::Error::msg)
            }

            // 清除 QR token → {cleared: true}
            "session.qr.clear" => {
                device_face::qr_clear_via_host().map_err(anyhow::Error::msg)?;
                Ok(serde_json::json!({ "cleared": true }))
            }

            // 网络信息（端口 + 本机 IPv4）→ {port, addresses}
            "session.network.info" => {
                device_face::network_info_via_host().map_err(anyhow::Error::msg)
            }

            // 已配对设备（活跃记录，按 pairedAt 倒序）→ PairedDeviceInfo[]
            "session.devices.paired-list" => {
                device_face::paired_list_via_host().map_err(anyhow::Error::msg)
            }

            // 撤销设备 → 复用统一信任视图的撤销编排（真源内核 pairings 软删 + 清历史）
            "session.devices.revoke" => {
                let id = args
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("id required"))?;
                trust::revoke_via_host(id).map_err(anyhow::Error::msg)
            }

            // 连接历史 {deviceId} → ConnectionHistory[]
            "session.devices.history-list" => {
                let device_id = args
                    .get("deviceId")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("deviceId required"))?;
                device_face::history_list_via_host(device_id).map_err(anyhow::Error::msg)
            }

            // 清空连接历史 {deviceId} → {cleared}
            "session.devices.history-clear" => {
                let device_id = args
                    .get("deviceId")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("deviceId required"))?;
                device_face::history_clear_via_host(device_id).map_err(anyhow::Error::msg)
            }

            // 读两项有效期 → {pairingCodeTtl, qrTokenTtl}
            "session.settings.ttl.get" => {
                device_face::ttl_get_via_host().map_err(anyhow::Error::msg)
            }

            // 写一项有效期 {key, value} → 写入后的两项（键白名单仲裁在宿主原语）
            "session.settings.ttl.set" => {
                let key = args
                    .get("key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("key required"))?;
                let value = args
                    .get("value")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| anyhow::anyhow!("value required"))?;
                device_face::ttl_set_via_host(key, value).map_err(anyhow::Error::msg)
            }

            // ==================== 票 15 命令面（任务域后端①：Agent 集成与会话状态） ====================
            // 命令名暂前缀 `session.task.*`（票 17 随 UI 迁入一并收口为最终命名）。
            // 本域命令**无互调 api 对应**（* 见票 13/14 先例）：任务是本插件自己的
            // 产品面，宿主命令门面不给任务 sematics 留位置。
            //
            // 宿主定时器回调到 `session.task.scheduler-tick`：按命令名分域计数失败，
            // 单域失败只降级本域（D7）。

            // 定时器 tick {now_utc} → {ticked, failed: [[domain, reason], ...]}
            "session.task.scheduler-tick" => {
                let now_utc = CommandArgs::new(args).str_or("now_utc", "");
                if now_utc.is_empty() {
                    return Err(anyhow::anyhow!("session.task.scheduler-tick: missing now_utc"));
                }
                Ok(task::tick_via_host(&WasmHost, &now_utc))
            }

            // 清理单个项目的全部 Agent 集成 {working_dir} → {success, message}
            "session.task.cleanup-project-hooks" => {
                let working_dir = CommandArgs::new(args).str_or("working_dir", "");
                let result = task::hooks::cleanup_project_all_integrations(&WasmHost, &working_dir);
                Ok(serde_json::json!({
                    "success": result.success,
                    "message": result.message,
                }))
            }

            // 会话当前任务状态 {session_id} → task_status 对象
            "session.task.get-status" => {
                let session_id = CommandArgs::new(args).str_or("session_id", "");
                task::state::get_task_status(&WasmHost, &session_id)
            }

            // 任务历史 {session_id?, status?, agent?, source?, since?, until?, limit?, offset?}
            "session.task.history-list" => {
                let filter = task_history_filter_from_args(&CommandArgs::new(args));
                task::state::list_task_history(&WasmHost, &filter)
            }

            // 任务历史统计（同筛选条件）
            "session.task.history-stats" => {
                let filter = task_history_filter_from_args(&CommandArgs::new(args));
                task::state::task_history_stats(&WasmHost, &filter)
            }

            // 运行中会话（含最新任务摘要）→ {sessions}
            "session.task.running-sessions" => {
                let sessions = task::state::list_running_sessions(&WasmHost);
                Ok(serde_json::json!({ "sessions": sessions }))
            }

            // 上报宿主平台 → {platform}（hooks 的 python 解释器按平台选择）
            "session.task.set-platform" => {
                let platform = CommandArgs::new(args).str_or("platform", "");
                if platform.is_empty() {
                    return Err(anyhow::anyhow!("set-platform: missing platform"));
                }
                // 白名单校验：仅接受宿主 OS 平台名，非法值直接拒绝
                if !["windows", "linux", "macos", "android", "ios"].contains(&platform.as_str()) {
                    return Err(anyhow::anyhow!(
                        "set-platform: unknown platform: {}",
                        platform
                    ));
                }
                WasmHost.storage_set("platform", &serde_json::json!(platform))?;
                Ok(serde_json::json!({ "platform": platform }))
            }

            // 会话开关 {session_id, auto_execute?, auto_answer?}（未给字段保持当前值）
            "session.task.set-auto-mode" => {
                let a = CommandArgs::new(args);
                let session_id = a.str_or("session_id", "");
                if session_id.is_empty() {
                    return Err(anyhow::anyhow!("set-auto-mode: missing session_id"));
                }
                let auto_execute = a.value("auto_execute").and_then(|v| v.as_bool());
                let auto_answer = a
                    .value("auto_answer")
                    .and_then(|v| v.as_bool())
                    .or_else(|| a.value("auto_approve").and_then(|v| v.as_bool()));
                task::state::set_auto_mode(&WasmHost, &session_id, auto_execute, auto_answer)
            }

            // 会话开关当前值 {session_id} → {auto_execute, auto_answer}
            "session.task.session-settings" => {
                let session_id = CommandArgs::new(args).str_or("session_id", "");
                if session_id.is_empty() {
                    return Err(anyhow::anyhow!("get-session-settings: missing session_id"));
                }
                task::state::get_session_settings(&WasmHost, &session_id)
            }

            // 支持自动任务的 agent 能力清单 → {agents}
            "session.task.supported-agents" => {
                let agents = task::agent::list_supported();
                Ok(serde_json::json!({ "agents": agents }))
            }

            // 会话配置 + 是否受任务域支持（agent 能力 join）→ {configs}
            "session.task.session-configs" => {
                let configs = task::list_configs_with_support_via_host();
                Ok(serde_json::json!({ "configs": configs }))
            }

            // ==================== 预设任务 ====================
            "session.task.preset-list" => {
                let presets = task::preset::list_presets(&WasmHost);
                Ok(serde_json::json!({ "presets": presets }))
            }
            "session.task.preset-create" => {
                let prompt = CommandArgs::new(args).str_or("prompt", "");
                if prompt.is_empty() {
                    return Err(anyhow::anyhow!("create-preset-task: missing prompt"));
                }
                let preset_id = task::preset::create_preset(&WasmHost, &prompt);
                task::preset::broadcast_preset_changed(&WasmHost, &preset_id, "create");
                Ok(serde_json::json!({ "preset_id": preset_id }))
            }
            "session.task.preset-delete" => {
                let preset_id = CommandArgs::new(args).str_or("preset_id", "");
                if preset_id.is_empty() {
                    return Err(anyhow::anyhow!("delete-preset-task: missing preset_id"));
                }
                if !task::preset::delete_preset(&WasmHost, &preset_id) {
                    return Err(anyhow::anyhow!(
                        "delete-preset-task: preset not found: {}",
                        preset_id
                    ));
                }
                task::preset::broadcast_preset_changed(&WasmHost, &preset_id, "delete");
                Ok(serde_json::json!({ "deleted": true }))
            }
            "session.task.preset-update" => {
                let a = CommandArgs::new(args);
                let preset_id = a.str_or("preset_id", "");
                let prompt = a.str_or("prompt", "").trim().to_string();
                if preset_id.is_empty() || prompt.is_empty() {
                    return Err(anyhow::anyhow!(
                        "update-preset-task: missing preset_id or prompt"
                    ));
                }
                if !task::preset::update_preset(&WasmHost, &preset_id, &prompt) {
                    return Err(anyhow::anyhow!(
                        "update-preset-task: preset not found: {}",
                        preset_id
                    ));
                }
                task::preset::broadcast_preset_changed(&WasmHost, &preset_id, "update");
                Ok(serde_json::json!({ "updated": true }))
            }
            // 预设入队（一次性消耗：入队后删除预设行）
            "session.task.preset-enqueue" => {
                let a = CommandArgs::new(args);
                let session_id = a.str_or("session_id", "");
                let preset_id = a.str_or("preset_id", "");
                if session_id.is_empty() {
                    return Err(anyhow::anyhow!("add-preset-to-queue: missing session_id"));
                }
                if preset_id.is_empty() {
                    return Err(anyhow::anyhow!("add-preset-to-queue: missing preset_id"));
                }
                let (task_id, position) =
                    task::preset::add_preset_to_queue(&WasmHost, &session_id, &preset_id)
                        .map_err(anyhow::Error::msg)?;
                dispatch_if_eligible_and_broadcast(&session_id, "add", None, None);
                task::preset::broadcast_preset_changed(&WasmHost, &preset_id, "enqueue");
                Ok(serde_json::json!({ "task_id": task_id, "position": position }))
            }

            // ==================== 任务队列 ====================

            // 队列列表 {session_id} → {tasks, active_task, session_id}
            "session.task.queue-list" => {
                let session_id = CommandArgs::new(args).str_or("session_id", "");
                let tasks = task::queue::list_queue(&WasmHost, &session_id);
                let active_task = task::queue::list_active_task(&WasmHost, &session_id);
                Ok(serde_json::json!({
                    "tasks": tasks,
                    "active_task": active_task,
                    "session_id": session_id,
                }))
            }
            // 入队 {session_id, prompt} → {task_id, position}
            "session.task.queue-add" => {
                let a = CommandArgs::new(args);
                let session_id = a.str_or("session_id", "");
                let prompt = a.str_or("prompt", "");
                if session_id.is_empty() {
                    return Err(anyhow::anyhow!("add-task: missing session_id"));
                }
                if prompt.is_empty() {
                    return Err(anyhow::anyhow!("add-task: missing prompt"));
                }
                let (task_id, position) = task::queue::add_task(&WasmHost, &session_id, &prompt);
                dispatch_if_eligible_and_broadcast(&session_id, "add", None, None);
                Ok(serde_json::json!({ "task_id": task_id, "position": position }))
            }
            // 取消队列项（仅 waiting / executing）{session_id, task_id}
            "session.task.queue-cancel" => {
                let a = CommandArgs::new(args);
                let session_id = a.str_or("session_id", "");
                let task_id = a.str_or("task_id", "");
                if session_id.is_empty() {
                    return Err(anyhow::anyhow!("cancel-task: missing session_id"));
                }
                if task_id.is_empty() {
                    return Err(anyhow::anyhow!("cancel-task: missing task_id"));
                }
                if !task::queue::cancel_task(&WasmHost, &session_id, &task_id) {
                    return Err(anyhow::anyhow!(
                        "cancel-task: task not found or not cancellable (only waiting/executing)"
                    ));
                }
                Ok(serde_json::json!({ "cancelled": true }))
            }
            // 移除队列项（仅 pending）{session_id, task_id}
            "session.task.queue-remove" => {
                let a = CommandArgs::new(args);
                let session_id = a.str_or("session_id", "");
                let task_id = a.str_or("task_id", "");
                if session_id.is_empty() {
                    return Err(anyhow::anyhow!("remove-task: missing session_id"));
                }
                if task_id.is_empty() {
                    return Err(anyhow::anyhow!("remove-task: missing task_id"));
                }
                if !task::queue::remove_task(&WasmHost, &session_id, &task_id) {
                    return Err(anyhow::anyhow!("remove-task: task not found: {}", task_id));
                }
                broadcast_queue_after(&session_id, "remove", None, None);
                Ok(serde_json::json!({ "removed": true }))
            }
            // 清空队列 {session_id} → {cleared}
            "session.task.queue-clear" => {
                let session_id = CommandArgs::new(args).str_or("session_id", "");
                if session_id.is_empty() {
                    return Err(anyhow::anyhow!("clear-queue: missing session_id"));
                }
                let cleared = task::queue::clear_queue(&WasmHost, &session_id);
                broadcast_queue_after(&session_id, "clear", None, None);
                Ok(serde_json::json!({ "cleared": cleared }))
            }
            // 改写队列项 prompt（仅 pending）{session_id, task_id, prompt}
            "session.task.queue-update" => {
                let a = CommandArgs::new(args);
                let session_id = a.str_or("session_id", "");
                let task_id = a.str_or("task_id", "");
                let prompt = a.str_or("prompt", "");
                if session_id.is_empty() {
                    return Err(anyhow::anyhow!("update-task: missing session_id"));
                }
                if task_id.is_empty() {
                    return Err(anyhow::anyhow!("update-task: missing task_id"));
                }
                if prompt.is_empty() {
                    return Err(anyhow::anyhow!("update-task: missing prompt"));
                }
                if !task::queue::update_task(&WasmHost, &session_id, &task_id, &prompt) {
                    return Err(anyhow::anyhow!(
                        "update-task: task not found or not pending: {}",
                        task_id
                    ));
                }
                broadcast_queue_after(&session_id, "update", None, None);
                Ok(serde_json::json!({ "updated": true }))
            }
            // 重排队列 {session_id, task_ids[]}（需为该会话 pending 项的全集）
            "session.task.queue-reorder" => {
                let a = CommandArgs::new(args);
                let session_id = a.str_or("session_id", "");
                let task_ids: Vec<String> = a
                    .value("task_ids")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                if session_id.is_empty() {
                    return Err(anyhow::anyhow!("reorder-queue: missing session_id"));
                }
                if task_ids.is_empty() {
                    return Err(anyhow::anyhow!("reorder-queue: missing task_ids"));
                }
                if !task::queue::reorder_queue(&WasmHost, &session_id, &task_ids) {
                    return Err(anyhow::anyhow!(
                        "reorder-queue: id set mismatch for session {}",
                        session_id
                    ));
                }
                broadcast_queue_after(&session_id, "reorder", None, None);
                Ok(serde_json::json!({ "reordered": true }))
            }

            // ==================== 票 16 命令面：定时任务（scheduled 域） ====================
            // 与旧 auto-task 的 4 条 auto-task.*scheduled* 命令逐字段同形（返回体
            // 键名与错误文案都对齐），移动端与桌面任务面板的调用面不因搬迁而变。

            // 定时任务列表 → {jobs[]}（按 trigger_at 升序，插件侧组织）
            "session.task.scheduled-list" => {
                let jobs = task::scheduled::list_jobs(&WasmHost);
                Ok(serde_json::json!({ "jobs": jobs }))
            }
            // 创建 {name?, config_id, trigger_at, prompts[]} → {job_id}
            "session.task.scheduled-create" => {
                let a = CommandArgs::new(args);
                let name = a.str_or("name", "");
                let config_id = a.str_or("config_id", "");
                let trigger_at = a.str_or("trigger_at", "");
                let prompts: Vec<String> = a
                    .value("prompts")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                            .filter(|s| !s.is_empty())
                            .collect()
                    })
                    .unwrap_or_default();
                if config_id.is_empty() {
                    return Err(anyhow::anyhow!("create-scheduled-job: missing config_id"));
                }
                if trigger_at.is_empty() {
                    return Err(anyhow::anyhow!("create-scheduled-job: missing trigger_at"));
                }
                if prompts.is_empty() {
                    return Err(anyhow::anyhow!("create-scheduled-job: missing prompts"));
                }
                match task::scheduled::create_job_with_broadcast(
                    &WasmHost,
                    &name,
                    &config_id,
                    &trigger_at,
                    &prompts,
                ) {
                    Some(job_id) => Ok(serde_json::json!({ "job_id": job_id })),
                    None => Err(anyhow::anyhow!(
                        "create-scheduled-job: failed to create job for config {}",
                        config_id
                    )),
                }
            }
            // 删除 {job_id} → {deleted}（仅 pending / missed / failed / executed）
            "session.task.scheduled-delete" => {
                let job_id = CommandArgs::new(args).str_or("job_id", "");
                if job_id.is_empty() {
                    return Err(anyhow::anyhow!("delete-scheduled-job: missing job_id"));
                }
                if task::scheduled::delete_job_with_broadcast(&WasmHost, &job_id) {
                    Ok(serde_json::json!({ "deleted": true }))
                } else {
                    Err(anyhow::anyhow!(
                        "delete-scheduled-job: job not found or not deletable: {}",
                        job_id
                    ))
                }
            }
            // 重置 {job_id, trigger_at?} → {reset, job_id, status}
            // （仅 missed / failed 可重置；trigger_at 缺省保留原触发时间）
            "session.task.scheduled-reset" => {
                let a = CommandArgs::new(args);
                let job_id = a.str_or("job_id", "");
                if job_id.is_empty() {
                    return Err(anyhow::anyhow!("reset-scheduled-job: missing job_id"));
                }
                let trigger_at = a.str_or("trigger_at", "");
                let trigger_param = if trigger_at.is_empty() {
                    None
                } else {
                    Some(trigger_at.as_str())
                };
                if task::scheduled::reset_job_with_broadcast(&WasmHost, &job_id, trigger_param) {
                    Ok(serde_json::json!({ "reset": true, "job_id": job_id, "status": "pending" }))
                } else {
                    Err(anyhow::anyhow!(
                        "reset-scheduled-job: job not found or not resettable (only missed/failed): {}",
                        job_id
                    ))
                }
            }

            // ==================== 票 16：HTTP 端点入口（path 段与旧插件逐字一致） ====
            // 宿主 `ANY /api/plugin/com.bedcode.terminal-session/{path}` 命中后调本命令，
            // `path` 即去掉前缀的相对段。分派表与 manifest `contributes.httpEndpoints`
            // 声明清单同源（task::HTTP_ENDPOINTS + [BUSINESS_HTTP_ENDPOINTS]，契约
            // 用例锁死）。
            "_http_endpoint" => {
                let a = CommandArgs::new(args);
                let method = a.str_or("method", "");
                let path = a.str_or("path", "");
                let body = a.value_owned("body").unwrap_or(serde_json::Value::Null);
                let query = a.value_owned("query").unwrap_or_else(|| serde_json::json!({}));
                Ok(handle_http_endpoint(
                    &WasmHost, &method, &path, &body, &query,
                ))
            }

            other => Err(anyhow::anyhow!("Unknown command: {}", other)),
        }
    }
}

/// 获取/初始化 QR token 管理器（OnceLock static，跨调用共享）
fn qr_manager() -> &'static QrTokenManager {
    QR_MANAGER.get_or_init(QrTokenManager::new)
}

/// 从命令参数组装任务历史查询筛选条件（CommandArgs 统一字段提取）
fn task_history_filter_from_args(
    args: &CommandArgs,
) -> task::state::TaskHistoryFilter {
    let opt = |key: &str| -> Option<String> {
        let v = args.str_or(key, "");
        if v.is_empty() {
            None
        } else {
            Some(v)
        }
    };
    task::state::TaskHistoryFilter {
        session_id: opt("session_id"),
        status: opt("status"),
        agent: opt("agent"),
        source: opt("source"),
        since: opt("since"),
        until: opt("until"),
        limit: args.value("limit").and_then(|v| v.as_i64()).unwrap_or(100),
        offset: args.value("offset").and_then(|v| v.as_i64()).unwrap_or(0),
    }
}

/// 入队后在「自动执行开启 + 会话空闲 + 队列无在途项」时才立即调度
///
/// 与手动入队同语义（对照组：旧 auto-task 插件的 add-task / add-preset-to-queue）：
/// `has_active_task` 拦截「会话已有运行中任务」，`has_inflight_task` 拦截「队列仍有
/// 在途项」——此刻调度会把在途 executing 项误归档为 done 并广播。
fn dispatch_if_eligible_and_broadcast(
    session_id: &str,
    action: &str,
    task_id: Option<&str>,
    status: Option<&str>,
) {
    let host = WasmHost;
    if task::state::auto_execute_on(&host, session_id)
        && !task::state::has_active_task(&host, session_id)
        && !task::queue::has_inflight_task(&host, session_id)
    {
        task::queue::try_dispatch_next(&host, session_id);
    }
    broadcast_queue_after(session_id, action, task_id, status);
}

/// 队列变更后按当前待执行数广播（数量恒读取自 `pending_count`，不复用入参估计值）
fn broadcast_queue_after(
    session_id: &str,
    action: &str,
    task_id: Option<&str>,
    status: Option<&str>,
) {
    let host = WasmHost;
    let count = task::queue::pending_count(&host, session_id);
    task::queue::broadcast_queue_changed(&host, session_id, count, action, task_id, status);
}

// ==================== 票 02：业务域 HTTP 端点（configs / quick-actions） ====================

/// 业务域 HTTP 端点清单（票 02）：`configs`（会话配置查询面）与 `quick-actions`
/// （快捷指令查询面）的插件侧实现。宿主网关别名表（`server/gateway.rs`
/// `BUSINESS_ROUTES`）按 manifest `contributes.httpEndpoints` 逐字声明才转发，
/// 因此本清单 = 网关 /api/configs 与 /api/quick-actions 的接管声明。
/// 与 [`task::HTTP_ENDPOINTS`] 一起构成 plugin.json `httpEndpoints` 的单一事实源
/// （契约用例锁死）。
pub const BUSINESS_HTTP_ENDPOINTS: &[&str] = &["configs", "quick-actions"];

/// 文件浏览域 + 工作区 git 域 HTTP 端点（票 03/04）：网关别名表
/// （/api/file-* 等 5 条 + /api/git/* 3 条）的插件接管声明。
/// `file_browse::HTTP_ENDPOINTS` = 5 条文件端点 + 3 条 git 端点（同域模块承载）
pub const FILE_BROWSE_HTTP_ENDPOINTS: &[&str] = file_browse::HTTP_ENDPOINTS;

/// 工作区 git 域 HTTP 端点（票 04）：网关 /api/git/* 的插件接管声明
/// （[`FILE_BROWSE_HTTP_ENDPOINTS`] 的 git 子集视图，供并集测试分域计数）
pub const GIT_HTTP_ENDPOINTS: &[&str] = &["git/branches", "git/status", "git/checkout"];

/// 认证链 HTTP 端点（票 07）：网关 /api/auth/* 七条公开路由的插件接管声明
pub const AUTH_HTTP_ENDPOINTS: &[&str] = auth_http::AUTH_HTTP_ENDPOINTS;

/// 免凭证（`auth: "none"`）端点清单——plugin.json `httpEndpoints` 里对象条目的唯一真源
///
/// 票 08 裁决 1：宿主对 HTTP 端点的缺省档已翻成最严档 `jwt`（未声明 auth 即要求
/// 移动端 JWT 验签）。本清单是**仅有的两批**必须免凭证可达的端点，逐条理由：
/// - `task-status` / `session-mode`：Claude Code / codex / pi / opencode 的 hook 脚本由
///   插件注入 PTY 环境，拿不到 JWT，只能按环回地址匿名调用（宿主转发的 `caller`
///   字段给 `localhost`，插件据此可自行区分，裁决 3）；
/// - `auth/*` 七条：本身是「拿 token 之前」的公开入口（配对 / QR / 重认证 / 生物绑定），
///   与网关别名表 `RouteAuth::Public` 同一批；票 08 起两处取较严者，故这里必须声明
///   `none`，否则移动端配对链路会被判 401。验签执行点仍在插件 auth 域 + `host-auth`。
pub const NO_AUTH_HTTP_ENDPOINTS: &[&str] = &[
    "task-status",
    "session-mode",
    "auth/pairing",
    "auth/verify",
    "auth/qr-connect",
    "auth/reauth",
    "auth/biometric-challenge",
    "auth/biometric-verify",
    "auth/biometric-bind",
];

/// 业务域 HTTP 分派入口（先业务域后任务域，路径全等匹配）
///
/// 返回体形状固定为 `{status, body, contentType?}`（`http_response` 辅助），
/// 宿主 `plugin_controller` 提取。未知业务路径落任务域由它自答 404。
fn handle_http_endpoint(
    host: &WasmHost,
    method: &str,
    path: &str,
    body: &serde_json::Value,
    query: &serde_json::Value,
) -> serde_json::Value {
    match path {
        // GET /api/configs（网关别名）→ 配置真源列表，wire 与宿主 ConfigItem 同形
        "configs" => handle_configs_http(host, method, body, query),
        // GET /api/quick-actions（网关别名）→ 快捷指令真源列表
        "quick-actions" => handle_quick_actions_http(host, method, body, query),
        // 票 03/04：文件浏览域 + 工作区 git 域（file-tree / file-tree-children /
        // file-content / diff-tree / file-diff / git/branches / git/status /
        // git/checkout）——网关别名表同源，未知业务路径落任务域自答 404
        "file-tree" | "file-tree-children" | "file-content" | "diff-tree" | "file-diff"
        | "git/branches" | "git/status" | "git/checkout" => {
            file_browse::handle_http_endpoint(host, method, path, body, query)
        }
        // 票 07：认证链（公开路由，JWT 之前的入口——编排归插件，经 host-auth
        // 原语回调宿主统一认证）
        "auth/pairing"
        | "auth/verify"
        | "auth/qr-connect"
        | "auth/reauth"
        | "auth/biometric-challenge"
        | "auth/biometric-verify"
        | "auth/biometric-bind" => auth_http::handle_http_endpoint(host, method, path, body, query),
        _ => task::handle_http_via_host(host, method, path, body, query),
    }
}

/// `GET configs`：配置真源（私有库）→ `{configs: ConfigItem[]}`
///
/// 形状与宿主 `config_controller::list_configs` 逐字节一致（双轨对照测试锚点）：
/// `{code: 0, message: "ok", data: {configs: [...]}}`，条目只含 6 个 wire 字段且
/// `wslDistro` 显式 null。排序沿用配置域业务排序（name 升序，宿主旧实现同序）。
fn handle_configs_http(
    host: &WasmHost,
    method: &str,
    _body: &serde_json::Value,
    _query: &serde_json::Value,
) -> serde_json::Value {
    if method != "GET" {
        return bedcode_plugin_api::http_response::error(
            405,
            &format!("Method not allowed: {method}"),
        );
    }
    let items = match crate::config::list_http_items_via_host() {
        Ok(items) => items,
        Err(e) => {
            host.log_warn(&format!("configs http: {}", e));
            return bedcode_plugin_api::http_response::error(500, "config store unavailable");
        }
    };
    bedcode_plugin_api::http_response::ok_with_data(serde_json::json!({
        "configs": items
    }))
}

/// `GET quick-actions`：快捷指令真源（私有库）→ `{actions: QuickActionItem[]}`
///
/// 形状与宿主 `config_controller::list_quick_actions` 逐字节一致（双轨对照测试锚点）：
/// `{code: 0, message: "ok", data: {actions: [...]}}`，条目只含 5 个 wire 字段且
/// `icon` / `color` 显式 null。排序沿用 `sort_order` 升序（宿主旧实现同序）。
fn handle_quick_actions_http(
    host: &WasmHost,
    method: &str,
    _body: &serde_json::Value,
    _query: &serde_json::Value,
) -> serde_json::Value {
    if method != "GET" {
        return bedcode_plugin_api::http_response::error(
            405,
            &format!("Method not allowed: {method}"),
        );
    }
    let items = match quick_actions::list_http_items_via_host() {
        Ok(items) => items,
        Err(e) => {
            host.log_warn(&format!("quick-actions http: {}", e));
            return bedcode_plugin_api::http_response::error(500, "quick action store unavailable");
        }
    };
    bedcode_plugin_api::http_response::ok_with_data(serde_json::json!({
        "actions": items
    }))
}

bedcode_plugin_api::wasm_entry!(SessionPlugin);

#[cfg(test)]
mod tests {
    use super::*;

    /// 共享静态状态（`CURRENT_CODE` / `QR_MANAGER`）测试串行化锁：native 下
    /// `static Mutex` 是真共享，并发用例互踩状态
    static PAIRING_STATE_LOCK: Mutex<()> = Mutex::new(());

    fn pairing_state_guard() -> std::sync::MutexGuard<'static, ()> {
        PAIRING_STATE_LOCK.lock().expect("pairing state lock")
    }

    fn reset_pairing_state() {
        *CURRENT_CODE.lock().expect("lock") = None;
        qr_manager().clear();
    }

    /// manifest 与 crate 常量的身份一致性：id / rustLibrary 任一处改名而另一处
    /// 未跟，宿主加载即按目录绑定校验失败——在单测阶段先撞红
    #[test]
    fn manifest_identity_matches_plugin_constants() {
        let manifest = SessionPlugin::manifest();
        assert_eq!(manifest.id, SessionPlugin::ID);
        assert_eq!(manifest.id, "com.bedcode.terminal-session");
        assert_eq!(manifest.rust_library, "bedcode_plugin_terminal_session");
        assert_eq!(env!("CARGO_PKG_NAME"), "bedcode-plugin-terminal-session");
    }

    /// 能力面声明只随已落地语义增长：票 05 = pairing 八项 + trust 两项 + consent 一项；
    /// 票 08 = config 三项（配置真源私有库）；票 09 = session-create（创建编排）；
    /// 票 10 = 会话动作四项 + 配置面只读化（v22，`session:config` 权限随写原语退役）；
    /// 票 11 = annotate（注解槽写面）+ devices-connect-list（设备派生视图）；
    /// 会话下沉票 04 = `connection:read`（连接清单换独立原语 `host-connection` 的判据位）。
    /// 权限 = `auth`（host-auth 密钥托管 + 记录面）+ `peer`（consent 取可信集 /
    /// trust peer 段）+ `storage`（票 08 私有库）+ `session:read`（配置读取面 + 会话
    /// 记录）+ `session:write`（创建与动作原语）。未经评审不得预声明（D2 权限
    /// 与能力映射一对一对应）
    #[test]
    fn declares_only_landed_domain_surface() {
        let manifest = SessionPlugin::manifest();
        assert_eq!(
            manifest.permissions,
            vec![
                "auth".to_string(),
                // 票 15 任务域按 D2 能力映射补的权限位：
                // `fs:read` / `fs:write`（Agent 集成写项目 hook）、
                // `terminal:input`（终端窗口键盘输入经宿主命令面
                // `plugin_terminal_send_input` 转入本插件 `session-input`）
                // + `terminal:observe`（输入行监听，ADR 0001；P1-b 起提交行观察
                // 已在 `session::input_via_pty` 内自驱，宿主注册面降为兼容通道）、
                // `broadcast`（任务/模式/队列状态广播）、
                // `timer:schedule`（队列延迟 clear 与静默超时的周期驱动）
                "broadcast".to_string(),
                // 票 04：在册连接清单从 `host-session.connections-list` 迁独立原语
                // `host-connection.connections-list`，权限判据同步换挂 `connection:read`
                // （审计单值化：谁能读连接清单只由本位回答，`session:read` 不再代答）
                "connection:read".to_string(),
                "fs:read".to_string(),
                "fs:write".to_string(),
                "peer".to_string(),
                // 票 03：文件浏览域 git diff 经 host-process run-sync（同步执行并
                // 捕获输出；与 process:run 同信任域——执行任意命令，声明即信任）
                "process:run".to_string(),
                // 票 08：`session:read`（config-list 精简列表 + get 全量行）——
                // v22 起配置面只读，`session:config` 权限已随写原语退役；
                // 读取面是迁移读 legacy 主库的唯一通道
                // 会话引擎下沉 P1-b：业务会话改走 host-pty 原语——
                // `pty:spawn`（spawn/kill，高风险面）+ `pty:io`（write/resize/
                // ring_fetch/is_running 数据面），会话真源切换的必要能力
                // 排序形态 = manifest-gen 的 `[...permissions].sort()`（ASCII 升序，
                // pty:io < pty:spawn）：本 pin 读的是构建链归一后的清单，非手改顺序
                "pty:io".to_string(),
                "pty:spawn".to_string(),
                "session:read".to_string(),
                // 票 09/10：host-session.create-with-spec 与四项会话动作原语
                "session:write".to_string(),
                // 票 08：host-plugin-database（配置真源私有库）
                "storage".to_string(),
                // 票 21（v20 host-task）：`task:run` ——git 域 diff_file_tree 三路只读
                // 命令改走 execute-batch 并行（池线程真并发，替代 run-sync 串行）
                "task:run".to_string(),
                "terminal:input".to_string(),
                "terminal:observe".to_string(),
                "timer:schedule".to_string(),
                // 票 17：任务队列弹窗的终端工具栏入口（`ui.registerTerminalToolbarItem`
                // 与 `ui.registerInputExtension` 同权限门），纯前端贡献面
                // 会话引擎下沉 P1-b：`pty:spawn`/`pty:io` 按词汇表 apiMap 归位
                // （manifest-gen RUST_PERMISSION_RULES 与 apiMap 逐字对应）
                // 清单顺序 = manifest-gen 的 ASCII 升序口径（release 构建会重排）
                "ui:input".to_string(),
                "ui:settings".to_string(),
                "ui:sidebar".to_string(),
            ],
            "spec D2 权限表：认证 auth/peer + 进程 process:run（票 03 git）+ 会话 \
             session:read/session:config/session:write + storage + ui:input/ui:sidebar/ui:settings"
        );
        let mut expected = SessionApiDispatcher::API_NAMES
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        expected.sort();
        let mut actual = manifest.api.clone();
        actual.sort();
        assert_eq!(actual, expected, "manifest api 必须与 trait 声明一致");
        assert_eq!(
            manifest.api.len(),
            31,
            "pairing 八项 + trust 两项（list/revoke）+ consent 一项（decide）+ config 三项 + \
             session-create 一项（票 09）+ 会话动作四项（票 10 restart/remove/rename/resize）\
             + 票 11 annotate + devices-connect-list 两项 + 票 02 quick-actions-import 一项 + \
             2026-09-22 认证记录下沉五项（auth-records-import / devices-list / history-list / \
             connection-touch / connection-close）+ 会话引擎下沉 P1 登记域读取面两项 \
             （session-list / session-get）+ P1-b 停止/输入两项（session-close / session-input）"
        );
    }

    /// 命令面：状态命令回传 manifest 声明（含四域），未知命令显性报错（禁止静默）
    #[test]
    fn invoke_command_status_roundtrips_manifest_and_rejects_unknown() {
        let status = SessionPlugin::invoke_command("session.status", serde_json::json!({}))
            .expect("session.status 可调用");
        assert_eq!(status["plugin"], "com.bedcode.terminal-session");
        assert_eq!(
            status["domains"],
            serde_json::json!([
                "pairing",
                "trust",
                "consent",
                "config",
                "session",
                "devices",
                "environment",
                // 票 15：任务域后端①（Agent 集成 + 会话状态）落地
                "task"
            ])
        );
        assert_eq!(
            status["permissions"],
            serde_json::json!(SessionPlugin::manifest().permissions)
        );

        let err = SessionPlugin::invoke_command("session.ghost", serde_json::Value::Null)
            .expect_err("未知命令必须报错");
        assert!(
            err.to_string().contains("Unknown command"),
            "错误信息需含命令名上下文, got: {err}"
        );
    }

    /// 命令面参数校验：trust.revoke 缺 id / consent.decide 载荷非法 → 显性报错
    /// （不猜测语义，不静默走空 id）
    #[test]
    fn invoke_command_rejects_malformed_args() {
        let err = SessionPlugin::invoke_command("session.trust.revoke", serde_json::json!({}))
            .expect_err("缺 id 必须报错");
        assert!(err.to_string().contains("id required"), "got: {err}");

        let err = SessionPlugin::invoke_command(
            "session.consent.decide",
            serde_json::json!({ "nodeId": 1 }),
        )
        .expect_err("载荷非法必须报错");
        assert!(
            err.to_string().contains("invalid consent request"),
            "got: {err}"
        );
    }

    /// native 命令面：宿主原语不可用（非 wasm 目标）→ 显性失败，不静默返回空视图
    /// （空列表会被消费方读成「没有任何信任设备」，是危险的默认值）
    #[test]
    fn native_trust_command_fails_loudly() {
        let err = SessionPlugin::invoke_command("session.trust.list", serde_json::json!({}))
            .expect_err("native 必须显性失败");
        assert!(
            err.to_string().contains("unavailable outside wasm runtime"),
            "got: {err}"
        );
    }

    /// 生成 → 宿主 DTO 形状（code/created_at/expires_in 三字段，expires_in=剩余）
    #[test]
    fn ticket13_command_face_fails_loudly_on_native() {
        // 三条命令均依赖 wasm 专属宿主原语（create-with-spec / close / host-platform），
        // native（cargo test）下链接中无对应符号 → 显性失败。静默返回空视图会被
        // 消费方读成「会话已创建 / 已停止 / 没有发行版」，是最危险的默认值。
        for cmd in [
            "session.create",
            "session.close",
            "session.environment.wsl-distros",
        ] {
            let err = SessionPlugin::invoke_command(
                cmd,
                serde_json::json!({ "configId": "c1", "sessionId": "s1" }),
            )
            .expect_err("native 必须显性失败");
            assert!(
                err.to_string().contains("unavailable outside wasm runtime"),
                "{cmd} 错误信息缺上下文: {err}"
            );
        }
    }

    /// 票 15 命令面接线：任务域命令已挂进 `invoke_command` 分发，未知命令显性报错
    ///
    /// `supported-agents` 是纯计算面（agent registry，无宿主调用），native 下即可
    /// 断言其输出——它同时是「搬迁前后同一输入 → 同一输出」的对照锚点：返回集合
    /// 必须与旧插件 `agent::list_supported()` 逐项一致（此处按名钉死，改名即红）。
    #[test]
    fn task_command_face_routes_agent_registry_and_rejects_unknown() {
        let v = SessionPlugin::invoke_command("session.task.supported-agents", serde_json::json!({}))
            .expect("supported-agents 已接线");
        assert_eq!(
            v["agents"],
            serde_json::json!(["claude", "pi", "codex", "opencode"]),
            "agent 能力清单必须与旧插件顺序与内容一致（对照基准）"
        );

        let err = SessionPlugin::invoke_command("session.task.ghost", serde_json::Value::Null)
            .expect_err("未知任务命令必须报错");
        assert!(
            err.to_string().contains("Unknown command"),
            "命令名漂移必须显性报错, got: {err}"
        );
    }

    /// 生成 → 宿主 DTO 形状（code/created_at/expires_in 三字段，expires_in=剩余）
    #[test]
    fn pairing_generate_returns_host_dto_shape() {
        let _guard = pairing_state_guard();
        reset_pairing_state();
        let v = pair_code_generate(300).expect("generate");
        assert_eq!(v["code"].as_str().expect("code").len(), 6);
        assert!(v["code"]
            .as_str()
            .unwrap()
            .chars()
            .all(|c| c.is_ascii_digit()));
        assert!(
            pairing::code::parse_rfc3339_utc(v["created_at"].as_str().expect("created_at"))
                .is_some(),
            "created_at 必须 RFC3339: {}",
            v["created_at"]
        );
        let expires = v["expires_in"].as_u64().expect("expires_in");
        assert!(expires <= 300, "expires_in 为剩余秒，必须 <= ttl");
    }

    /// 生成 → 状态 → 验证 → 一次性失效 → 清除（宿主桥接贯穿的同一状态机）
    #[test]
    fn pairing_verify_is_single_use() {
        let _guard = pairing_state_guard();
        reset_pairing_state();
        let v = pair_code_generate(300).expect("generate");
        let code = v["code"].as_str().expect("code").to_string();

        let status = pair_code_status().expect("status");
        assert_eq!(status.expect("当前码存在")["code"], code.as_str());

        assert!(pair_code_verify(&code).expect("verify"));
        assert!(
            !pair_code_verify(&code).expect("reuse"),
            "一次性：二次验证失败"
        );
        assert!(
            pair_code_status().expect("status after").is_none(),
            "消耗后无当前码"
        );
    }

    /// 篡改（错误码）拒绝后正确码仍可用；清除后一律失败
    #[test]
    fn pairing_rejects_tampered_code_and_after_clear() {
        let _guard = pairing_state_guard();
        reset_pairing_state();
        let v = pair_code_generate(300).expect("generate");
        let code = v["code"].as_str().expect("code").to_string();
        let tampered = if code == "000000" { "111111" } else { "000000" };
        assert!(!pair_code_verify(tampered).expect("wrong"), "错误码拒绝");
        // 错误码不消耗：正确码仍可验证（宿主 verify_and_consume 同语义）
        assert!(pair_code_verify(&code).expect("correct after wrong"));

        reset_pairing_state();
        pair_code_generate(300).expect("generate");
        SessionPlugin::pairing_code_clear().expect("clear");
        assert!(
            !pair_code_verify("anything").expect("after clear"),
            "清除后验证失败"
        );
        assert!(pair_code_status().expect("status").is_none());
    }

    /// QR：生成 → 状态 → 验证消耗（一次性）→ 清除，reason 分类与宿主文案同构
    #[test]
    fn qr_generate_verify_single_use_and_clear() {
        let _guard = pairing_state_guard();
        reset_pairing_state();
        let v = qr_generate(300).expect("generate");
        let token = v["token"].as_str().expect("token").to_string();
        assert_eq!(token.len(), 32, "QR token 32 hex 字符");
        assert!(v["remaining"].as_u64().expect("remaining") <= 300);

        assert_eq!(
            qr_status().expect("status").expect("活跃 token")["token"],
            token.as_str()
        );
        assert_eq!(qr_verify(&token).expect("verify")["valid"], true);

        let again = qr_verify(&token).expect("reuse");
        assert_eq!(again["valid"], false);
        assert_eq!(
            again["reason"], "No active QR token",
            "成功消费即清除（宿主同语义）"
        );

        qr_generate(300).expect("generate");
        SessionPlugin::qr_code_clear().expect("clear");
        let none = qr_verify("ghost").expect("no active");
        assert_eq!(none["reason"], "No active QR token");
        assert!(qr_status().expect("status after").is_none());
    }
}
