//! host-pty 逻辑层 —— 插件私有伪终端原语（ABI v16）
//!
//! spec：`.scratch/2026-09-19-pty-base-service/spec.md`；票据 02（契约定稿 +
//! spawn→ring-fetch 最小贯通）。
//!
//! **零业务代码红线（ADR 0022 / spec D1）**：本模块只做引擎原语——裸 PTY 创建、
//! 字节读写面、句柄登记、属主仲裁、按游标应答。spawn 只收
//! `command + args + env + workingDir + cols/rows`，**不做** shell 包装
//! （`bash -lic` / PowerShell `-Command` / CMD `/K`）、WSL 路径转换、危险字符校验、
//! 默认 shell 探测（那些是宿主业务会话线与插件产品的语义，spec D5）。
//!
//! **与业务会话线零交叉（spec D2）**：插件 PTY 不进 `SessionComponents`、不注册
//! `GlobalOutputManager`、不参与业务会话事件链；共享同一 PTY 引擎
//! （[`PtySession`]）但互不可见。边界对侧是 `host-terminal` / `host-session` /
//! `terminal-hooks`（服务宿主业务会话），三者与本接口不得互相转发。
//!
//! **输出面是纯拉取（spec D3）**：每个 PTY 一条有界环形缓冲 [`PtyRing`]，读线程
//! 单生产者写入，插件按自己的游标 `ring-fetch`。慢插件只损失自己的历史（淘汰最旧并
//! 以 `truncated` 上报缺口），背压绝不回传到读线程；**没有 push 回调**（wasmtime
//! Store 不可重入，异步唤醒插件在语义上不成立）。
//!
//! **宿主广播声明（会话语义下沉票 05，P3 形态 B）**：业务会话也是插件 PTY，
//! 「输出能不能被宿主 server 读走」必须是插件自己的决定——**默认私有**。spawn 的
//! config-json 可选字段 `hostBroadcastSessionId`（= 本句柄服务的会话 id）是 opt-in
//! 声明，宿主据此在**同一张注册表**（[`PTYS`]）内维护只读的「会话 id → pty 句柄」
//! 映射（不新增第二份表；登记随 spawn、摘除随终态，复用句柄生命周期单点）。
//! 宿主 server 直读同进程 `PtyRing`，零跨 WASM 边界；映射访问器
//! [`broadcast_handle_for_session`] 是**本模块唯一**读出口——未声明的句柄任何宿主
//! 广播面都得不到（反向锁：这是行为用例，不是注释）。同属主「同 id 重建」（重启）
//! 允许并存并取最新句柄（`registered_seq`）；与他属主撞 id 在 spawn 显性拒绝。
//!
//! **多消费者并发拉取语义（票 07 的输入，写死在契约里）**：同一句柄的环可被属主
//! 插件（`ring-fetch`）与宿主广播面（直读）**同时**拉取。游标由各调用方自持，
//! [`PtyRing::fetch`] 是纯读（不消费、不推进全局状态）——两个消费的游标互不知晓、
//! 互不影响；淘汰由**产出量**驱动（环满即淘汰最旧），任一消费者的读取都不释放空间，
//! 各自落后于驻留起点都会 `truncated`。环容量必须覆盖最慢消费者的滞后（07 实测），
//! 宿主广播面的读取**不受** [`PLUGIN_PTY_RING_FETCH_MAX_BYTES`] 约束（那是 WASM
//! 边界拷贝限额；宿主直读无此成本）。
//!
//! 票 02 定稿契约并贯通 `spawn` / `ring-fetch`，票 03 补齐数据面 `write` /
//! `resize` / `is-running`，票 04 补齐生命面：`kill` + `pty:exit` 事件（属主私有 topic）+
//! 停用回收，票 05 落地限额与背压：每插件在册条数配额、插件声明的环容量
//! （`spawn` config 的 `ringBytes`，宿主仲裁上下限）、单次写入准入与单次拉取截断。
//!
//! **终止与摘除的单一发布者不变量（票 04）**：`kill()` 只发起终止（引擎侧优雅中断 →
//! 兜底强杀），**不**自己发事件；句柄摘除与事件发布统一由 spawn 时起动的退出监听
//! 任务在「读线程 EOF + 子进程回收」齐备（票 01 的 [`PtyTerminationGate`]）时完成，
//! 且**只有从注册表 `remove` 成功的那一方**才发布事件——自然退出 / 主动 kill / 停用
//! 回收三条路径交汇时，每条 PTY 恰好一条 `pty:exit`，不多发也不漏发。

use bedcode_plugin_api::host::bus::owned_topic;

use crate::enums::PtySessionStatus;
use crate::pty::{PtyRing, PtyRingFetch, PtyRingSink, PtySession, PtyTerminated};
use crate::system::config::AppConfig;
use crate::system::constants::{
    PLUGIN_PTY_MAX_SESSIONS_PER_PLUGIN, PLUGIN_PTY_MAX_WRITE_BYTES, PLUGIN_PTY_RING_BYTES,
    PLUGIN_PTY_RING_FETCH_MAX_BYTES, PLUGIN_PTY_RING_MAX_BYTES,
};
use crate::wasm_core::bus::MessageBus;
use crate::wasm_core::manager::runtime::{block_on_async, WasmHostContext};
use crate::wasm_core::permission::{PERMISSION_PTY_IO, PERMISSION_PTY_SPAWN};
use portable_pty::CommandBuilder;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

/// 非属主操作的统一拒绝文案（属主仲裁，同 mdns / ws 先例）
const NOT_OWNER: &str = "not owner of pty handle";

/// 句柄前缀（`pty-<uuid>`）
const HANDLE_PREFIX: &str = "pty-";

/// 生命周期事件名（topic = `<owner>::pty:exit`，事件名段与 SDK `PTY_EXIT` 常量逐字一致）
const EVENT_EXIT: &str = "pty:exit";

fn denied_spawn() -> String {
    "permission denied: pty:spawn".to_string()
}

fn denied_io() -> String {
    "permission denied: pty:io".to_string()
}

// ==================== 注册表 ====================

/// 在册插件 PTY：句柄属主 + 引擎会话 + 输出环
struct PtyEntry {
    owner: String,
    /// 进程存活性的持有者：`PtySession` 最后一次引用被 drop 即杀子进程
    session: PtySession,
    ring: Arc<Mutex<PtyRing>>,
    /// 宿主广播声明（spawn config 的 `hostBroadcastSessionId`）：`None` = 未声明，
    /// 任何宿主广播面都读不到（安全边界在缺省侧，会话语义下沉票 05）
    broadcast_session_id: Option<String>,
    /// 登记序（全局单调递增）：同属主「同 id 重建」时映射取**最新**一条
    registered_seq: u64,
}

/// 在册登记序计数器（`registered_seq` 的唯一来源，全局单调）
static REGISTER_SEQ: AtomicU64 = AtomicU64::new(0);

/// 全局插件 PTY 注册表（句柄 → 条目；跨插件按 owner 隔离）
static PTYS: LazyLock<Mutex<HashMap<String, PtyEntry>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// 每插件在册条数上限（属主 → 生效配额）
///
/// 真源是 manifest 的 `ptyQuota` 声明：会话引擎下沉 P1（H1）把业务会话搬进本注册表后，
/// 「用户可同时开多少个终端」就成了属主插件自己的产品档位，单一内核常量（8）不再是
/// 合理答案。登记点在插件加载漏斗（与 `PermissionManager::grant_permissions` 同一处，
/// 见 `manager/loader.rs`），区间合法性在解析期已由
/// `manager/validation.rs::validate_pty_quota` 把关，故此处只取不判。
///
/// 表内无记录 = 未声明（既有插件、无头测试上下文）→ 取内核默认档
/// [`PLUGIN_PTY_MAX_SESSIONS_PER_PLUGIN`]，与引入声明字段之前的行为逐字一致。
static QUOTAS: LazyLock<Mutex<HashMap<String, usize>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// 登记某插件的生效配额（加载期调用；`None` = 未声明，落默认档）
pub(crate) fn register_quota(plugin_id: &str, declared: Option<usize>) {
    let effective = declared.unwrap_or(PLUGIN_PTY_MAX_SESSIONS_PER_PLUGIN);
    QUOTAS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(plugin_id.to_string(), effective);
    tracing::debug!(plugin_id = %plugin_id, quota = effective, "host-pty: 配额已登记");
}

/// 某插件当前生效配额（`spawn` 的判据）
fn quota_of(plugin_id: &str) -> usize {
    QUOTAS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(plugin_id)
        .copied()
        .unwrap_or(PLUGIN_PTY_MAX_SESSIONS_PER_PLUGIN)
}

/// 某插件当前在册 PTY 条数（配额判据与副作用断言共用）
fn registered_count_for(plugin_id: &str) -> usize {
    PTYS.lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .filter(|entry| entry.owner == plugin_id)
        .count()
}

/// `spawn` 的 config-json 契约（纯引擎参数，camelCase，spec D5）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpawnConfig {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: Option<HashMap<String, String>>,
    #[serde(default)]
    working_dir: Option<String>,
    #[serde(default)]
    cols: Option<u16>,
    #[serde(default)]
    rows: Option<u16>,
    /// 本条 PTY 输出环容量（字节）：省略取 [`PLUGIN_PTY_RING_BYTES`]，
    /// 超 [`PLUGIN_PTY_RING_MAX_BYTES`] 直接拒绝（宿主仲裁，见 `resolve_ring_bytes`）
    #[serde(default)]
    ring_bytes: Option<u64>,
    /// 宿主广播声明（会话语义下沉票 05）：本句柄服务的会话 id（opt-in——宿主可只读
    /// 订阅本句柄输出）。缺省 = 默认私有，任何宿主广播面都读不到；空串拒绝。
    /// 同属主重建允许并存（取最新），他属主撞 id 拒绝。
    #[serde(default)]
    host_broadcast_session_id: Option<String>,
}

/// 仲裁插件声明的输出环容量：`None` → 默认值；`Some(0)` 或超上限 → `Err`
///
/// **不夹取到上限**：静默降级会让插件按自己声明的历史深度规划上下文、实际却少得多，
/// 与 spec D9 的「配额类失败必须可见」同一分级（数据面只有读侧截断不是错误）。
fn resolve_ring_bytes(declared: Option<u64>) -> Result<u64, String> {
    let capacity = match declared {
        None => PLUGIN_PTY_RING_BYTES,
        Some(0) => return Err("pty spawn: ringBytes must be greater than 0".to_string()),
        Some(bytes) if bytes > PLUGIN_PTY_RING_MAX_BYTES => {
            return Err(format!(
                "pty spawn: ringBytes too large ({bytes} > limit {PLUGIN_PTY_RING_MAX_BYTES})"
            ))
        }
        Some(bytes) => bytes,
    };
    Ok(capacity)
}

/// 当前在册声明了该会话 id 的属主（`hostBroadcastSessionId` 跨属主唯一性的判据）
///
/// 只做查表；冲突仲裁分两层：本函数是「副作用之前」的早失败（顺序路径），
/// 登记处同一把锁内的复查是并发竞态下的权威判据。
fn declared_session_owner(session_id: &str) -> Option<String> {
    PTYS.lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|(_, entry)| entry.broadcast_session_id.as_deref() == Some(session_id))
        .map(|(_, entry)| entry.owner.clone())
}

// ==================== 创建域（pty:spawn） ====================

/// 创建插件私有裸 PTY：成功返回 `pty-<uuid>` 句柄并登记属主
///
/// 失败只回错误、不发布任何事件（无句柄可寻址），且不留注册表项与进程。
pub(crate) fn pty_spawn(host_ctx: &WasmHostContext, plugin_id: &str, config_json: &str) -> Result<String, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PTY_SPAWN, "host_pty_spawn") {
        return Err(denied_spawn());
    }
    let config: SpawnConfig =
        serde_json::from_str(config_json).map_err(|e| format!("pty spawn: invalid config: {e}"))?;
    let command = config.command.trim();
    if command.is_empty() {
        return Err("pty spawn: command must not be empty".to_string());
    }
    // 配额与容量在任何副作用之前判定（不留下开好的 fd / 起动的进程）
    let ring_bytes = resolve_ring_bytes(config.ring_bytes)?;
    let in_use = registered_count_for(plugin_id);
    let quota = quota_of(plugin_id);
    if in_use >= quota {
        return Err(format!(
            "pty spawn: too many ptys for this plugin ({in_use} in use, limit {quota})"
        ));
    }

    // 宿主广播声明（票 05）——空串显性拒绝（不静默当作未声明）；跨属主撞 id 在此
    // 早失败（副作用之前判定，不给冲突场景留下开好的 fd / 起动的进程；并发竞态下的
    // 权威判据在登记处的同一把锁内，见下）
    let raw_broadcast = config.host_broadcast_session_id.as_deref().map(str::trim);
    if let Some(id) = raw_broadcast {
        if id.is_empty() {
            return Err("pty spawn: hostBroadcastSessionId must not be empty".to_string());
        }
        if let Some(owner) = declared_session_owner(id) {
            if owner != *plugin_id {
                return Err(format!(
                    "pty spawn: session id {id} already declared by another plugin ({owner})"
                ));
            }
        }
    }
    let broadcast_session_id = raw_broadcast.map(str::to_string);

    // 裸 argv：宿主不做 shell 解析、不做危险字符校验（参数数组 exec 天然免注入）
    let mut builder = CommandBuilder::new(command);
    for arg in &config.args {
        builder.arg(arg);
    }
    // env 是「追加」语义（继承宿主环境后覆盖同名键）：整体清空会让 PATH 类命令不可用
    if let Some(env) = &config.env {
        for (key, value) in env {
            builder.env(key, value);
        }
    }
    if let Some(dir) = &config.working_dir {
        builder.cwd(dir);
    }

    let terminal = &AppConfig::global().terminal;
    let cols = config.cols.unwrap_or(terminal.default_cols);
    let rows = config.rows.unwrap_or(terminal.default_rows);

    let pty_id = format!("{HANDLE_PREFIX}{}", uuid::Uuid::new_v4());
    let (sink, ring) = PtyRingSink::paired_with_limits(ring_bytes, PtyRing::DEFAULT_MAX_CHUNKS);
    let session = PtySession::with_private_command(pty_id.clone(), cols, rows, builder, sink)
        .map_err(|e| format!("pty spawn: 打开伪终端失败 (plugin {plugin_id}): {e}"))?;
    // **start 之前**订阅终态：广播不补发历史，短命命令（`/bin/true`）完全可能在登记
    // 与监听起动之前就 EOF + 回收完毕——晚订阅即永远等不到事件，句柄与环双双泄漏
    let lifecycle_rx = session.subscribe_lifecycle();

    if let Err(e) = block_on_async(session.start()) {
        // 启动失败：session 在本作用域末被 drop，引擎 Drop 路径负责清理半成品进程
        return Err(format!(
            "pty spawn: 启动子进程失败 (pty_id {pty_id}, plugin {plugin_id}): {e}"
        ));
    }

    PTYS.lock().unwrap_or_else(|e| e.into_inner()).insert(
        pty_id.clone(),
        PtyEntry {
            owner: plugin_id.to_string(),
            session,
            ring,
            broadcast_session_id,
            registered_seq: REGISTER_SEQ.fetch_add(1, Ordering::Relaxed),
        },
    );
    // 退出监听在登记之后起动：保证「摘除 + 发布」时句柄必已在册
    spawn_exit_monitor(
        pty_id.clone(),
        plugin_id.to_string(),
        lifecycle_rx,
        Arc::clone(&host_ctx.message_bus),
    );
    tracing::info!(
        plugin_id = %plugin_id,
        pty_id = %pty_id,
        cols,
        rows,
        args = config.args.len(),
        "host-pty: 插件私有 PTY 已创建"
    );
    Ok(pty_id)
}

// ==================== 数据域（pty:io） ====================

/// 写入输入字节（票 03）
///
/// 分块与让出节奏在引擎侧（`PtySession::write`：4000 字节分块 + 逐块 yield，避免打满
/// PTY 内核缓冲）；本层的 [`PLUGIN_PTY_MAX_WRITE_BYTES`] 是**一次调用的准入上限**，
/// 超限直接 `Err`——静默截断会让插件把「半条命令」喂进交互进程，比失败更糟。
pub(crate) fn pty_write(host_ctx: &WasmHostContext, plugin_id: &str, pty_id: &str, data: &[u8]) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PTY_IO, "host_pty_write") {
        return Err(denied_io());
    }
    if data.len() > PLUGIN_PTY_MAX_WRITE_BYTES {
        return Err(format!(
            "pty write: payload too large ({} bytes > limit {PLUGIN_PTY_MAX_WRITE_BYTES})",
            data.len()
        ));
    }
    let session = session_of(plugin_id, pty_id)?;
    block_on_async(session.write(data)).map_err(|e| format!("pty write: 写入失败 (pty_id {pty_id}): {e}"))
}

/// 调整终端尺寸（票 03）
///
/// 透传到 PTY 尺寸即视为成功；**不承诺同步生效时序**（内核把 winsize 变更以 SIGWINCH
/// 通知前台进程组，全屏程序在下一帧重绘才对齐），插件按输出形态验证。
pub(crate) fn pty_resize(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    pty_id: &str,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PTY_IO, "host_pty_resize") {
        return Err(denied_io());
    }
    let session = session_of(plugin_id, pty_id)?;
    block_on_async(session.resize(cols, rows))
        .map_err(|e| format!("pty resize: 调整尺寸失败 (pty_id {pty_id}, {cols}x{rows}): {e}"))
}

/// 查询进程是否仍在运行（票 03）
///
/// 判据 = `running` 标志 **且** 读线程尚未终结：子进程自然退出时引擎的 `running`
/// 不会翻下（只有 kill/销毁会，业务线依赖这一语义），而插件私有 PTY 释放了 slave
/// fd，EOF 即退出信号，故两路合一才如实。定位是「bus 不缓冲不重放」下丢失
/// `pty:exit` 后的自愈快照，不是事件替代品。
pub(crate) fn pty_is_running(host_ctx: &WasmHostContext, plugin_id: &str, pty_id: &str) -> Result<bool, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PTY_IO, "host_pty_is_running") {
        return Err(denied_io());
    }
    let session = session_of(plugin_id, pty_id)?;
    Ok(running_verdict(session.is_running(), session.output_terminated()))
}

/// `is-running` 的判据组合（纯函数：真值表可确定性锁定，见票 03 变异 C-②）
///
/// 单独抽出来的唯一理由是「进程已自然退出」这一格无法稳定抓窗口——EOF 之后毫秒级
/// 就会被退出监听摘除句柄。故把组合逻辑做成纯判定，四格真值表直接断言；会话状态
/// 的两路输入由 [`PtySession::is_running`] / [`PtySession::output_terminated`] 供。
fn running_verdict(running: bool, output_terminated: bool) -> bool {
    running && !output_terminated
}

/// 终止并销毁（票 04）：优雅 Ctrl-C → 兜底强杀，复用引擎既有语义
///
/// `Ok(())` 表示**终止已发起**；句柄摘除与 `<owner>::pty:exit`（reason=killed）由退出
/// 监听在「EOF + 子进程回收」齐备时完成（见模块头的单一发布者不变量）。因此 kill 后
/// 立刻 `ring-fetch` 仍可能取到尾帧，而后再取即 `pty handle not found`。
pub(crate) fn pty_kill(host_ctx: &WasmHostContext, plugin_id: &str, pty_id: &str) -> Result<(), String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PTY_SPAWN, "host_pty_kill") {
        return Err(denied_spawn());
    }
    let session = session_of(plugin_id, pty_id)?;
    block_on_async(session.kill()).map_err(|e| format!("pty kill: 终止失败 (pty_id {pty_id}): {e}"))
}

// ==================== 生命周期事件与回收（票 04） ====================

/// 退出原因映射（spec D4）：`killed` 以宿主侧 kill 请求位为准
///
/// portable-pty 的 `ExitStatus` 不带 signal（信号终止也报 `code=1`），无法从退出码
/// 区分「被杀」与 `exit 1`——判据只在 [`PtyTerminated::killed`] 上（票 01 定案）。
fn exit_reason_of(terminated: &PtyTerminated) -> &'static str {
    if terminated.killed {
        "killed"
    } else if terminated.status == PtySessionStatus::Error {
        "error"
    } else {
        "stopped"
    }
}

/// 组装 `<owner>::pty:exit` payload（camelCase，与 WIT/SDK 文档一致）
///
/// `exit_code` 为 `None`（回收失败/无句柄）时**省略字段**而非报 0——插件据字段有无
/// 判断「拿不到退出码」，与「退出码就是 0」区分开。
fn pty_exit_payload(pty_id: &str, reason: &str, exit_code: Option<i32>) -> serde_json::Value {
    let mut payload = serde_json::json!({ "ptyId": pty_id, "reason": reason });
    if let Some(code) = exit_code {
        payload["exitCode"] = serde_json::json!(code);
    }
    payload
}

/// 发布退出事件到属主私有 topic（与 mdns / ws 同路：`<owner>::pty:exit`，
/// 非属主订阅被总线命名空间门禁拒绝，他人也伪投递不进）
fn publish_pty_exit(bus: &MessageBus, owner: &str, pty_id: &str, reason: &str, exit_code: Option<i32>) {
    bus.publish(
        &owned_topic(owner, EVENT_EXIT),
        "host",
        pty_exit_payload(pty_id, reason, exit_code),
    );
}

/// 起一条退出监听任务：等终态 → 摘注册表 → 发布事件
///
/// **必须显式派生到 ambient runtime**（`spawn_with_error_boundary_on`），不能用
/// 常规 `spawn_with_error_boundary`：WASI 预打开模式下插件调用跑在**无 runtime
/// handle 的阻塞线程**上（见 `block_on_async` 的 ambient 分支），本函数由
/// `pty_spawn` 的同步路径直接调用，该线程上 `tokio::spawn` 立即 panic
/// （"there is no reactor running"）——panic 穿透会污染 wasmtime Store，让插件
/// 整体失效。ambient 句柄与调用线程的 runtime 状态无关，且任务生命周期本就应该
/// 长于单次宿主调用（它等的是任意时刻才到达的终态事件）。
fn spawn_exit_monitor(
    pty_id: String,
    owner: String,
    lifecycle_rx: tokio::sync::broadcast::Receiver<PtyTerminated>,
    bus: Arc<MessageBus>,
) {
    crate::system::error_boundary::spawn_with_error_boundary_on(
        &crate::wasm_core::manager::runtime::ambient_handle(),
        "pty_exit_monitor",
        reap_and_publish(pty_id, owner, lifecycle_rx, bus),
    );
}

async fn reap_and_publish(
    pty_id: String,
    owner: String,
    mut lifecycle_rx: tokio::sync::broadcast::Receiver<PtyTerminated>,
    bus: Arc<MessageBus>,
) {
    let terminated = match lifecycle_rx.recv().await {
        Ok(terminated) => terminated,
        // 发送端随会话 drop：句柄已由他方摘除（停用回收路径），事件也已由他方发布
        Err(e) => {
            tracing::debug!(pty_id = %pty_id, plugin_id = %owner, error = %e, "PTY 退出监听未收到终态事件即结束");
            return;
        }
    };
    let reason = exit_reason_of(&terminated);
    // 摘除成功者才发布：与停用回收竞争时保证「每条 PTY 恰好一条事件」
    let taken = PTYS.lock().unwrap_or_else(|e| e.into_inner()).remove(&pty_id);
    match taken {
        Some(entry) => {
            // entry 在此 drop：会话最后一次引用释放（进程此时确已退出并被回收）
            drop(entry);
            tracing::info!(
                plugin_id = %owner,
                pty_id = %pty_id,
                reason = %reason,
                exit_code = ?terminated.exit_code,
                "host-pty: 插件私有 PTY 已终止并摘除"
            );
            publish_pty_exit(&bus, &owner, &pty_id, reason, terminated.exit_code);
        }
        None => {
            tracing::debug!(
                plugin_id = %owner,
                pty_id = %pty_id,
                reason = %reason,
                "host-pty: 终态到达时句柄已被摘除（停用回收已发布事件），不重复发"
            );
        }
    }
}

/// 在册句柄快照（`owner = None` → 全部属主；注册表锁内只取 id，回收在锁外做）
fn registered_handles(owner: Option<&str>) -> Vec<String> {
    let table = PTYS.lock().unwrap_or_else(|e| e.into_inner());
    table
        .iter()
        .filter(|(_, entry)| owner.map_or(true, |o| entry.owner == o))
        .map(|(pty_id, _)| pty_id.clone())
        .collect()
}

/// 回收一批在册句柄：摘除 → kill → 按属主补发 `pty:exit`（reason=killed）
///
/// **只有从注册表成功摘除的那一方发布事件**（票 04 单一发布者不变量的半段）：与退出
/// 监听竞争时，监听任务若先摘除则本函数跳过该句柄（事件已由其发布），不多发也不漏发。
/// 事件按 `entry.owner` 投递，故本函数不必知道调用来源（按属主回收 / 全量回收同路）。
fn reclaim_handles(handles: Vec<String>, bus: &MessageBus) -> usize {
    let mut reclaimed = 0;
    for pty_id in handles {
        let entry = PTYS.lock().unwrap_or_else(|e| e.into_inner()).remove(&pty_id);
        let Some(entry) = entry else {
            continue; // 监听任务抢先摘除：事件已由其发布
        };
        let owner = entry.owner.clone();
        if let Err(e) = block_on_async(entry.session.kill()) {
            // 进程此时多已自行结束，kill 失败属可恢复；仍摘除并补发事件（不泄漏句柄）
            tracing::warn!(plugin_id = %owner, pty_id = %pty_id, error = %e, "PTY 回收 kill 降级");
        }
        drop(entry.session);
        drop(entry.ring);
        publish_pty_exit(bus, &owner, &pty_id, "killed", None);
        reclaimed += 1;
    }
    reclaimed
}

/// 插件停用回收：kill 并摘除其全部 PTY，逐条补发 `<owner>::pty:exit`（reason=killed）
///
/// **只碰本人**（同 mdns / ws 的按属主回收）：其他插件在册句柄不受影响。事件由本函数
/// 发布（先摘除后 kill，停用窗口内不再接受任何针对该句柄的调用）；对应的退出监听任务
/// 稍后醒来看不到句柄即静默结束（单一发布者不变量的另一半）。
pub(crate) fn purge_for_plugin(plugin_id: &str, bus: &MessageBus) -> usize {
    let purged = reclaim_handles(registered_handles(Some(plugin_id)), bus);
    if purged > 0 {
        tracing::info!(plugin_id = %plugin_id, purged, "host-pty: 停用回收完成");
    }
    purged
}

/// 引擎层**全量**回收（系统关停路径）：跨属主 kill 并摘除全部在册插件 PTY，
/// 逐条按属主补发 `<owner>::pty:exit`（reason=killed）
///
/// 为什么必须由引擎自持（会话引擎下沉 P1 开放点 4：关停走引擎层全局 kill）：关机时
/// 插件可能已停用 / 超时 / trap，按属主回收依赖「插件停用流程被调到」；引擎自持回收
/// 保证不留孤儿进程，同时事件仍按属主投递（「谁在册谁收到终态」不因回收来源而变）。
///
/// 会话引擎下沉 P1-b 起业务会话也走 `host-pty`，届时关停路径**只需**本函数
/// （业务线 `SessionManager::shutdown` 随 P4 退役）。
pub(crate) fn kill_all_registered(bus: &MessageBus) -> usize {
    let reclaimed = reclaim_handles(registered_handles(None), bus);
    if reclaimed > 0 {
        tracing::info!(reclaimed, "host-pty: 引擎层全量回收完成（系统关停）");
    }
    reclaimed
}

/// 在册插件 PTY 条数（引擎事实：关停守卫 / 诊断用；**非 WIT 原语**、不设权限门、不取参数）
///
/// 在册即存活：终态（读线程 EOF + 子进程回收齐备）由退出监听摘除句柄，故本计数就是
/// 「活着的插件私有 PTY 数」。会话引擎下沉 P1 后，关窗守卫的判据将由会话状态改为
/// 「引擎事实：存活 PTY 计数 > 0」（开放点 4 / spec 验收项）。
pub(crate) fn live_count() -> usize {
    PTYS.lock().unwrap_or_else(|e| e.into_inner()).len()
}

// ==================== 宿主广播面访问器（会话语义下沉票 05） ====================

/// 宿主广播面可读句柄（「会话 id → pty 句柄」只读映射的访问结果）
///
/// **只读边界**：宿主的 WS/HTTP 广播面经本结构直读同进程 [`PtyRing`]（按游标拉取）
/// 与存活快照 / 终态订阅——只用于**读输出 / 观察生命周期**；写输入、改尺寸、终止仍走
/// 属主插件（`host-pty` 原语 / 会话网关），不经本句柄（那些操作的权限判据挂在插件侧）。
pub(crate) struct BroadcastHandle {
    /// 引擎句柄（日志 / `pty:exit` 事件关联）
    pub pty_id: String,
    /// 属主插件（审计 / 事件 topic 关联）
    pub owner: String,
    /// 输出环（唯一读出口；[`PtyRing::fetch`] 纯读，游标调用方自持）
    pub ring: Arc<Mutex<PtyRing>>,
    /// 引擎会话（存活快照 [`PtySession::is_running`] / 终态订阅
    /// [`PtySession::subscribe_lifecycle`] 用）
    pub session: PtySession,
}

/// 按会话 id 取宿主广播句柄（只读映射访问器；**未声明 / 未在册 → `None`**）
///
/// - **反向锁**：`hostBroadcastSessionId` 未声明的句柄**任何**宿主广播面都得不到
///   （行为用例锁定，见 tests）——这是安全边界，不是注释；
/// - **生命周期单点**：登记随 spawn、摘除随终态，复用 [`PTYS`] 同一条生命周期，
///   不新增第二份登记表（结构锁见 tests）；
/// - **同属主「同 id 重建」（重启路径）**：在册可能同存新旧两条，取**最新**
///   （登记序最大）一条；重启前的旧句柄随其终态从映射消失；
/// - **跨属主撞 id 在 spawn 已拒绝**，故此处无需再做归属过滤。
pub(crate) fn broadcast_handle_for_session(session_id: &str) -> Option<BroadcastHandle> {
    let table = PTYS.lock().unwrap_or_else(|e| e.into_inner());
    table
        .iter()
        .filter(|(_, entry)| entry.broadcast_session_id.as_deref() == Some(session_id))
        .max_by_key(|(_, entry)| entry.registered_seq)
        .map(|(pty_id, entry)| BroadcastHandle {
            pty_id: pty_id.clone(),
            owner: entry.owner.clone(),
            ring: Arc::clone(&entry.ring),
            session: entry.session.clone(),
        })
}

/// 按游标拉取输出历史（spec D3）
///
/// `Ok(None)` = 游标已追平产出端（无新字节）；`Ok(Some)` = 自游标起的字节 + 续拉
/// 游标，游标落后于环驻留起点时 `truncated = true`（缺口如实上报，不静默补洞）。
/// 单次返回不超过 [`PLUGIN_PTY_RING_FETCH_MAX_BYTES`]（约束一次 wasm 边界拷贝量）。
pub(crate) fn pty_ring_fetch(
    host_ctx: &WasmHostContext,
    plugin_id: &str,
    pty_id: &str,
    from_offset: u64,
    max_bytes: u32,
) -> Result<Option<PtyRingFetch>, String> {
    if !super::check_permission(host_ctx, plugin_id, PERMISSION_PTY_IO, "host_pty_ring_fetch") {
        return Err(denied_io());
    }
    // 注册表锁内只取环句柄，应答在锁外完成——两把锁不交叉持有
    let ring = with_entry(pty_id, plugin_id, |entry| Arc::clone(&entry.ring))?;
    let budget = max_bytes.min(PLUGIN_PTY_RING_FETCH_MAX_BYTES) as usize;
    let fetched = ring
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .fetch(from_offset, budget);
    if fetched.data.is_empty() && !fetched.truncated {
        return Ok(None);
    }
    Ok(Some(fetched))
}

// ==================== 守卫 ====================

/// 属主校验 + 在注册表锁内读取条目资源（锁内只做查表与克隆，不在锁内 await）
fn with_entry<T>(pty_id: &str, plugin_id: &str, f: impl FnOnce(&PtyEntry) -> T) -> Result<T, String> {
    let table = PTYS.lock().unwrap_or_else(|e| e.into_inner());
    match table.get(pty_id) {
        Some(entry) if entry.owner == plugin_id => Ok(f(entry)),
        Some(_) => Err(NOT_OWNER.to_string()),
        None => Err(not_found(pty_id)),
    }
}

/// 取会话句柄（`PtySession: Clone` 共享同一内部状态，取出后即可在锁外 await）
fn session_of(plugin_id: &str, pty_id: &str) -> Result<PtySession, String> {
    with_entry(pty_id, plugin_id, |entry| entry.session.clone())
}

/// 句柄不在册的拒绝文案（kill / 退出事件摘除后，句柄立即不可寻址）
fn not_found(pty_id: &str) -> String {
    format!("pty handle not found: {pty_id}")
}

// ==================== Tests ====================

// 测试整体迁至 host_impl/tests/pty.rs（#[path] 声明，保持模块树 pty::tests 不变，
// 私有项可见性与 fixture 互斥语义与内联形态完全等价；拆分动机见 review P1）
#[cfg(test)]
#[path = "tests/pty.rs"]
mod tests;
