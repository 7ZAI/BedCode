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
//! 票 02 定稿契约并贯通 `spawn` / `ring-fetch`，票 03 补齐数据面 `write` /
//! `resize` / `is-running`，票 04 补齐生命面：`kill` + `pty:exit.<owner>` 事件 +
//! 停用回收，票 05 落地限额与背压：每插件在册条数配额、插件声明的环容量
//! （`spawn` config 的 `ringBytes`，宿主仲裁上下限）、单次写入准入与单次拉取截断。
//!
//! **终止与摘除的单一发布者不变量（票 04）**：`kill()` 只发起终止（引擎侧优雅中断 →
//! 兜底强杀），**不**自己发事件；句柄摘除与事件发布统一由 spawn 时起动的退出监听
//! 任务在「读线程 EOF + 子进程回收」齐备（票 01 的 [`PtyTerminationGate`]）时完成，
//! 且**只有从注册表 `remove` 成功的那一方**才发布事件——自然退出 / 主动 kill / 停用
//! 回收三条路径交汇时，每条 PTY 恰好一条 `pty:exit`，不多发也不漏发。

use crate::enums::PtySessionStatus;
use crate::plugin::bus::MessageBus;
use crate::plugin::manager::wasm_runtime::{block_on_async, WasmHostContext};
use crate::plugin::permission::{PERMISSION_PTY_IO, PERMISSION_PTY_SPAWN};
use crate::pty::{PtyRing, PtyRingFetch, PtyRingSink, PtySession, PtyTerminated};
use crate::system::config::AppConfig;
use crate::system::constants::plugin::{
    PLUGIN_PTY_MAX_SESSIONS_PER_PLUGIN, PLUGIN_PTY_MAX_WRITE_BYTES, PLUGIN_PTY_RING_BYTES,
    PLUGIN_PTY_RING_FETCH_MAX_BYTES, PLUGIN_PTY_RING_MAX_BYTES,
};
use portable_pty::CommandBuilder;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

/// 非属主操作的统一拒绝文案（属主仲裁，同 mdns / ws 先例）
const NOT_OWNER: &str = "not owner of pty handle";

/// 句柄前缀（`pty-<uuid>`）
const HANDLE_PREFIX: &str = "pty-";

/// 生命周期事件名（topic = `pty:exit.<owner>`，与 SDK `PTY_EXIT` 常量逐字一致）
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
}

/// 全局插件 PTY 注册表（句柄 → 条目；跨插件按 owner 隔离）
static PTYS: LazyLock<Mutex<HashMap<String, PtyEntry>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

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
    if in_use >= PLUGIN_PTY_MAX_SESSIONS_PER_PLUGIN {
        return Err(format!(
            "pty spawn: too many ptys for this plugin ({in_use} in use, limit {PLUGIN_PTY_MAX_SESSIONS_PER_PLUGIN})"
        ));
    }

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
/// `Ok(())` 表示**终止已发起**；句柄摘除与 `pty:exit.<owner>`（reason=killed）由退出
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

/// 组装 `pty:exit.<owner>` payload（camelCase，与 WIT/SDK 文档一致）
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

/// 发布退出事件到属主作用域 topic（与 mdns / ws 同路：owner 内嵌在 topic 里，
/// 非属主不知道也无法收到他人的事件流）
fn publish_pty_exit(bus: &MessageBus, owner: &str, pty_id: &str, reason: &str, exit_code: Option<i32>) {
    bus.publish(
        &format!("{EVENT_EXIT}.{owner}"),
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
        &crate::plugin::manager::wasm_runtime::ambient_handle(),
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

/// 插件停用回收：kill 并摘除其全部 PTY，逐条补发 `pty:exit.<owner>`（reason=killed）
///
/// **只碰本人**（同 mdns / ws 的按属主回收）：其他插件在册句柄不受影响。事件由本函数
/// 发布（先摘除后 kill，停用窗口内不再接受任何针对该句柄的调用）；对应的退出监听任务
/// 稍后醒来看不到句柄即静默结束（单一发布者不变量的另一半）。
pub(crate) fn purge_for_plugin(plugin_id: &str, bus: &MessageBus) -> usize {
    let owned: Vec<String> = {
        let table = PTYS.lock().unwrap_or_else(|e| e.into_inner());
        table
            .iter()
            .filter(|(_, entry)| entry.owner == plugin_id)
            .map(|(pty_id, _)| pty_id.clone())
            .collect()
    };

    let mut purged = 0;
    for pty_id in owned {
        let entry = PTYS.lock().unwrap_or_else(|e| e.into_inner()).remove(&pty_id);
        let Some(entry) = entry else {
            continue; // 监听任务抢先摘除：事件已由其发布
        };
        if let Err(e) = block_on_async(entry.session.kill()) {
            // 进程此时多已自行结束，kill 失败属可恢复；仍摘除并补发事件（不泄漏句柄）
            tracing::warn!(plugin_id = %plugin_id, pty_id = %pty_id, error = %e, "停用回收 kill 降级");
        }
        drop(entry.session);
        drop(entry.ring);
        publish_pty_exit(bus, plugin_id, &pty_id, "killed", None);
        purged += 1;
    }
    if purged > 0 {
        tracing::info!(plugin_id = %plugin_id, purged, "host-pty: 停用回收完成");
    }
    purged
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::manager::wasm_runtime::host_impl::tests::{build_host_ctx, grant_permissions};
    use std::time::{Duration, Instant};

    // ==================== 测试脚手架 ====================

    /// 常驻配置：进程阻塞在 `read`，句柄在整个断言窗口内保持在册
    ///
    /// **为什么不能再用 `/bin/true` 这类短命命令**：票 04 起「终态事件即摘除」——进程
    /// 一退出，退出监听任务就把句柄与环从注册表摘掉，随后任何 `ring-fetch` /
    /// `is-running` 都会得到 `pty handle not found` 而非预期的行为。凡以「句柄仍在」
    /// 为前提的用例一律用本配置（数据面/生命面的载体必须是活进程）。
    #[cfg(target_os = "linux")]
    const ALIVE: &str = r#"{"command":"/bin/sh","args":["-c","read go"]}"#;

    /// 常驻 + 先输出一段文本：`ring-fetch` 主干用例的载体（输出后有内容可读，进程不死）
    #[cfg(target_os = "linux")]
    fn alive_with_output(text: &str) -> String {
        format!(r#"{{"command":"/bin/sh","args":["-c","echo {text}; read go"]}}"#)
    }

    /// 授权两域并 spawn，返回可直接驱动宿主函数的上下文
    #[cfg(target_os = "linux")]
    fn ctx_with_pty(plugin_id: &str, config_json: &str) -> Arc<WasmHostContext> {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, plugin_id, &[PERMISSION_PTY_SPAWN, PERMISSION_PTY_IO]);
        pty_spawn(&ctx, plugin_id, config_json).expect("spawn 应成功");
        ctx
    }

    #[cfg(target_os = "linux")]
    fn spawn_ok(plugin_id: &str, config_json: &str) -> String {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, plugin_id, &[PERMISSION_PTY_SPAWN, PERMISSION_PTY_IO]);
        pty_spawn(&ctx, plugin_id, config_json).expect("spawn 应成功")
    }

    /// 取回某句柄的环（注册表内部视图，仅测试用）
    #[cfg(target_os = "linux")]
    fn ring_of(pty_id: &str) -> Arc<Mutex<PtyRing>> {
        PTYS.lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(pty_id)
            .map(|entry| Arc::clone(&entry.ring))
            .expect("句柄应在册")
    }

    /// 环上当前驻留的全部字节（一次持锁读取）
    #[cfg(target_os = "linux")]
    fn resident_text(ring: &Arc<Mutex<PtyRing>>) -> String {
        let ring = ring.lock().unwrap_or_else(|e| e.into_inner());
        let (min, max) = ring.watermarks();
        let fetched = ring.fetch(min, (max - min) as usize);
        String::from_utf8_lossy(&fetched.data).into_owned()
    }

    /// 轮询环直至出现期望内容（真 PTY 产出是异步的：断言内容，不断言时序）
    #[cfg(target_os = "linux")]
    fn wait_for_output(ring: &Arc<Mutex<PtyRing>>, want: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let text = resident_text(ring);
            if text.contains(want) || Instant::now() >= deadline {
                return text;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[cfg(target_os = "linux")]
    fn unique_tag(prefix: &str) -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        format!(
            "BEDCODE_PTY_{prefix}_{}",
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        )
    }

    // ==================== 权限同步点漂移锁 ====================

    /// 漂移锁：权限五同步点必须同时认识 `pty:spawn` / `pty:io`
    ///
    /// 漏任一处（SDK 合法集合 / 打包 CLI / 前端合法集合 / 宿主能力清单 / host_impl
    /// 权限门）都会造成「manifest 声明了却被静默丢弃」或「前端放行宿主拒绝」，
    /// 票面按未完成处理。SDK 与能力清单走行为断言，纯文本集合（TS/JS）走字面量断言。
    #[test]
    fn permission_sync_points_all_know_pty_domains() {
        for domain in ["pty:spawn", "pty:io"] {
            // ① SDK 合法集合：未列入 VALID_PERMISSIONS 的权限会在授权时被过滤掉
            let pm = crate::plugin::permission::PermissionManager::new();
            let granted = pm.grant_permissions("com.bedcode.sync", &[domain.to_string()]);
            assert!(granted.contains(domain), "SDK VALID_PERMISSIONS 缺 {domain}");
            assert!(
                pm.check("com.bedcode.sync", domain),
                "SDK 授权后 check 应为真: {domain}"
            );

            // ② 打包 CLI + ③ 前端合法集合（CARGO_MANIFEST_DIR = bedcode-desktop/src-tauri）
            let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
            let cli = std::fs::read_to_string(manifest_dir.join("../packages/plugin-sdk-desktop/bin/cli.js"))
                .expect("打包 CLI 可读");
            let frontend = std::fs::read_to_string(manifest_dir.join("../src/plugin/permission.ts"))
                .expect("前端 permission.ts 可读");
            let literal = format!("'{domain}'");
            assert!(cli.contains(&literal), "打包 CLI 合法集合缺 {domain}");
            assert!(frontend.contains(&literal), "前端合法集合缺 {domain}");
        }

        // ④ 宿主能力清单（manifest dependencies 可达性）
        let registry = crate::plugin::manager::capability::CapabilityRegistry::new();
        assert!(registry.is_available("host-pty"), "能力清单缺 host-pty");

        // ⑤ host_impl 权限门：本模块全部函数都以 check_permission 打头（见 pty_spawn），
        //    上面的权限三态用例即为该同步点的行为证据。
    }

    // ==================== 权限门（三态） ====================

    /// 反例：完全未授权 → spawn 拒绝，注册表零副作用
    #[test]
    fn spawn_without_permission_is_denied() {
        let ctx = build_host_ctx();
        let err = pty_spawn(&ctx, "com.bedcode.no-pty", r#"{"command":"/bin/true"}"#).unwrap_err();
        assert_eq!(err, "permission denied: pty:spawn");
        assert_eq!(
            registered_count_for("com.bedcode.no-pty"),
            0,
            "权限拒绝不得留下任何句柄"
        );
    }

    /// 反例：只有数据面权限（pty:io）→ 创建域仍拒绝（两域独立）
    #[test]
    fn spawn_with_io_permission_only_is_denied() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, "com.bedcode.io-only", &[PERMISSION_PTY_IO]);
        let err = pty_spawn(&ctx, "com.bedcode.io-only", r#"{"command":"/bin/true"}"#).unwrap_err();
        assert_eq!(err, "permission denied: pty:spawn");
        assert_eq!(registered_count_for("com.bedcode.io-only"), 0);
    }

    /// 反例：只有创建域权限 → ring-fetch 拒绝（spawn 与 io 互不越界）
    #[cfg(target_os = "linux")]
    #[test]
    fn ring_fetch_without_io_permission_is_denied() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, "com.bedcode.spawn-only", &[PERMISSION_PTY_SPAWN]);
        let pty_id = pty_spawn(&ctx, "com.bedcode.spawn-only", ALIVE).expect("spawn");

        let err = pty_ring_fetch(&ctx, "com.bedcode.spawn-only", &pty_id, 0, 1024).unwrap_err();
        assert_eq!(err, "permission denied: pty:io");
    }

    // ==================== 参数校验（fail-visible，无副作用） ====================

    /// 反例：command 空白 / 缺 command / 非法 JSON → Err 且不产生句柄
    #[test]
    fn spawn_rejects_invalid_config_without_side_effects() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, "com.bedcode.bad-config", &[PERMISSION_PTY_SPAWN]);
        for bad in [r#"{"command":"   "}"#, r#"{"args":["x"]}"#, "not json"] {
            let err = pty_spawn(&ctx, "com.bedcode.bad-config", bad).unwrap_err();
            assert!(
                err.contains("command") || err.contains("invalid config"),
                "非法配置应回明确错误，got: {err}"
            );
        }
        assert_eq!(registered_count_for("com.bedcode.bad-config"), 0);
    }

    // ==================== 属主隔离 ====================

    /// 反例：他人句柄不可寻址；未知句柄回 not found
    #[cfg(target_os = "linux")]
    #[test]
    fn ring_fetch_on_foreign_handle_is_not_owner() {
        let pty_id = spawn_ok("com.bedcode.owner-a", ALIVE);
        let ctx = build_host_ctx();
        grant_permissions(&ctx, "com.bedcode.owner-b", &[PERMISSION_PTY_SPAWN, PERMISSION_PTY_IO]);

        assert_eq!(
            pty_ring_fetch(&ctx, "com.bedcode.owner-b", &pty_id, 0, 1024).unwrap_err(),
            NOT_OWNER
        );
        let missing = pty_ring_fetch(&ctx, "com.bedcode.owner-b", "pty-does-not-exist", 0, 1024).unwrap_err();
        assert!(missing.contains("not found"), "got: {missing}");
    }

    /// 反例：生命面（kill）同样先做属主仲裁（越权不得转化为对他人进程的终止）
    #[cfg(target_os = "linux")]
    #[test]
    fn kill_still_enforces_owner_before_its_own_gate() {
        let pty_id = spawn_ok("com.bedcode.owner-a", ALIVE);
        let ctx = build_host_ctx();
        grant_permissions(&ctx, "com.bedcode.owner-b", &[PERMISSION_PTY_SPAWN, PERMISSION_PTY_IO]);

        assert_eq!(pty_kill(&ctx, "com.bedcode.owner-b", &pty_id).unwrap_err(), NOT_OWNER);
    }

    /// 反例：数据面同样受属主仲裁（write / resize / is-running 三函数）
    #[cfg(target_os = "linux")]
    #[test]
    fn io_apis_enforce_owner() {
        let pty_id = spawn_ok("com.bedcode.owner-a", ALIVE);
        let ctx = build_host_ctx();
        grant_permissions(&ctx, "com.bedcode.owner-b", &[PERMISSION_PTY_SPAWN, PERMISSION_PTY_IO]);

        assert_eq!(
            pty_write(&ctx, "com.bedcode.owner-b", &pty_id, b"ls\n").unwrap_err(),
            NOT_OWNER
        );
        assert_eq!(
            pty_resize(&ctx, "com.bedcode.owner-b", &pty_id, 100, 30).unwrap_err(),
            NOT_OWNER
        );
        assert_eq!(
            pty_is_running(&ctx, "com.bedcode.owner-b", &pty_id).unwrap_err(),
            NOT_OWNER
        );
    }

    /// 反例（票 06 矩阵分格）：**完全不授权**的插件调用全部 6 个函数 → 一律权限拒绝，
    /// 且证明权限门先于属主仲裁与句柄查表（用不存在的句柄也必须回权限错，而不是 not-found）
    #[test]
    fn every_api_without_any_permission_is_denied_before_any_lookup() {
        let ctx = build_host_ctx();
        let probe = "pty-never-registered";
        let cases: [(&str, Result<(), String>); 6] = [
            (
                "spawn",
                pty_spawn(&ctx, "com.bedcode.bare", r#"{"command":"/bin/true"}"#).map(|_| ()),
            ),
            ("write", pty_write(&ctx, "com.bedcode.bare", probe, b"x").map(|_| ())),
            (
                "resize",
                pty_resize(&ctx, "com.bedcode.bare", probe, 80, 24).map(|_| ()),
            ),
            ("kill", pty_kill(&ctx, "com.bedcode.bare", probe).map(|_| ())),
            (
                "ring-fetch",
                pty_ring_fetch(&ctx, "com.bedcode.bare", probe, 0, 1024).map(|_| ()),
            ),
            (
                "is-running",
                pty_is_running(&ctx, "com.bedcode.bare", probe).map(|_| ()),
            ),
        ];
        for (api, result) in cases {
            let err = result.expect_err("未授权调用必须被拒");
            assert!(
                err.starts_with("permission denied: pty:"),
                "{api} 必须先撞权限门，got: {err}"
            );
            assert!(!err.contains("not found"), "{api} 不得越过权限门去查句柄: {err}");
        }
        assert_eq!(registered_count_for("com.bedcode.bare"), 0, "拒绝不得留下任何副作用");
    }

    // ==================== 生命周期（票 04：kill / 退出事件 / 停用回收） ====================

    /// 漂移锁：宿主发布的事件名与 topic 形状必须与 SDK 常量/助手逐字一致
    ///
    /// 插件按 SDK 的 `pty_event_topic(PTY_EXIT, id)` 订阅，宿主按
    /// `{EVENT_EXIT}.{owner}` 发布——两处各写一份就会「订阅成功但永远收不到」，
    /// 且这类错配在单侧测试里不可见。
    #[test]
    fn exit_event_name_matches_sdk_subscription_helper() {
        use bedcode_plugin_api::host as sdk;
        assert_eq!(EVENT_EXIT, sdk::PTY_EXIT, "宿主事件名与 SDK 常量漂移");
        assert_eq!(
            format!("{EVENT_EXIT}.com.example.plugin"),
            sdk::pty_event_topic(sdk::PTY_EXIT, "com.example.plugin"),
            "宿主 topic 形状与 SDK 助手漂移"
        );
    }

    /// 记录总线投递的 payload（含 topic 与 sender，供定向投递与恰好一次断言）
    struct ExitSink {
        tx: std::sync::mpsc::Sender<serde_json::Value>,
    }

    impl crate::plugin::bus::BusMessageHandler for ExitSink {
        fn on_message(&self, msg: &bedcode_plugin_api::BusMessage) -> anyhow::Result<()> {
            let _ = self.tx.send(serde_json::json!({
                "topic": msg.topic,
                "sender": msg.sender,
                "payload": msg.payload,
            }));
            Ok(())
        }
    }

    /// 在宿主总线上静态订阅一个 topic（等价插件 activate 期的 `bus_subscribe`）
    async fn subscribe(
        bus: &Arc<MessageBus>,
        sub_id: &str,
        topic: &str,
    ) -> std::sync::mpsc::Receiver<serde_json::Value> {
        let (tx, rx) = std::sync::mpsc::channel();
        bus.subscribe_static(sub_id, topic, Box::new(ExitSink { tx })).await;
        rx
    }

    /// 等待一条投递（消费任务异步，超时返回 None）
    async fn wait_event(rx: &std::sync::mpsc::Receiver<serde_json::Value>) -> Option<serde_json::Value> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(event) = rx.try_recv() {
                return Some(event);
            }
            if Instant::now() >= deadline {
                return None;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// 轮询至句柄被监听任务摘除（`kill` 只发起终止，摘除在终态齐备时发生）
    async fn wait_handle_retired(ctx: &Arc<WasmHostContext>, plugin_id: &str, pty_id: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match pty_is_running(ctx, plugin_id, pty_id) {
                Err(e) if e.contains("not found") => return e,
                _ => {
                    if Instant::now() >= deadline {
                        panic!("句柄必须在超时前被摘除，当前仍可寻址: {pty_id}");
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }
        }
    }

    /// 正例：属主 kill → 进程终止 + 句柄摘除 + 属主收到 reason=killed 的退出事件
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn kill_terminates_handle_and_publishes_killed_event() {
        let owner = "com.bedcode.kill";
        let ctx = build_host_ctx();
        grant_permissions(&ctx, owner, &[PERMISSION_PTY_SPAWN, PERMISSION_PTY_IO]);
        let pty_id = pty_spawn(&ctx, owner, ALIVE).expect("spawn");
        let events = subscribe(&ctx.message_bus, "sub-kill", &format!("{EVENT_EXIT}.{owner}")).await;

        pty_kill(&ctx, owner, &pty_id).expect("kill 应成功");
        let event = wait_event(&events).await.expect("属主必须收到 pty:exit");
        assert_eq!(
            event["payload"]["ptyId"].as_str(),
            Some(pty_id.as_str()),
            "事件必须寻址到被杀的那条 PTY: {event}"
        );
        assert_eq!(event["payload"]["reason"], "killed", "kill 路径 reason 固定: {event}");
        assert_eq!(event["topic"], format!("{EVENT_EXIT}.{owner}"), "topic 内嵌属主");
        assert_eq!(event["sender"], "host", "事件由宿主发布");

        // 摘除即不可寻址（句柄与环同时释放）
        let err = wait_handle_retired(&ctx, owner, &pty_id).await;
        assert!(err.contains("not found"), "got: {err}");
        assert_eq!(registered_count_for(owner), 0, "kill 后注册表不得留残项");
    }

    /// 正例：进程自然退出 → 属主收到 reason=stopped + 真实退出码（票 01 的 exitCode 能力）
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn natural_exit_publishes_stopped_event_with_exit_code() {
        let owner = "com.bedcode.natural-exit";
        let ctx = build_host_ctx();
        grant_permissions(&ctx, owner, &[PERMISSION_PTY_SPAWN, PERMISSION_PTY_IO]);
        // 订阅先于 spawn：宿主不缓冲不重放，短命命令可能在订阅前就终态
        let events = subscribe(&ctx.message_bus, "sub-exit", &format!("{EVENT_EXIT}.{owner}")).await;

        pty_spawn(&ctx, owner, r#"{"command":"/bin/sh","args":["-c","exit 42"]}"#).expect("spawn");
        let event = wait_event(&events).await.expect("自然退出必须发出退出事件");
        assert_eq!(event["payload"]["reason"], "stopped", "非 kill 的退出: {event}");
        assert_eq!(
            event["payload"]["exitCode"], 42,
            "退出码必须如实带出（引擎侧回收所得）: {event}"
        );
    }

    /// 边界：未显式 exit 的进程退出码为 0，且**不得**与「取不到退出码」混淆为缺字段
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn zero_exit_code_is_reported_as_value_not_absent_field() {
        let owner = "com.bedcode.exit-zero";
        let ctx = build_host_ctx();
        grant_permissions(&ctx, owner, &[PERMISSION_PTY_SPAWN, PERMISSION_PTY_IO]);
        let events = subscribe(&ctx.message_bus, "sub-zero", &format!("{EVENT_EXIT}.{owner}")).await;

        pty_spawn(&ctx, owner, r#"{"command":"/bin/true"}"#).expect("spawn");
        let event = wait_event(&events).await.expect("须有退出事件");
        assert_eq!(
            event["payload"].get("exitCode"),
            Some(&serde_json::json!(0)),
            "退出码 0 与「回收失败无退出码」必须可区分: {event}"
        );
    }

    /// 事件定向：非属主 topic 零投递，属主 topic 恰好一条（不重放、不双发）
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn exit_events_are_owner_scoped_and_exactly_once() {
        let owner = "com.bedcode.exit-owner";
        let other = "com.bedcode.exit-other";
        let ctx = build_host_ctx();
        grant_permissions(&ctx, owner, &[PERMISSION_PTY_SPAWN, PERMISSION_PTY_IO]);
        grant_permissions(&ctx, other, &[PERMISSION_PTY_SPAWN, PERMISSION_PTY_IO]);

        let owner_events = subscribe(&ctx.message_bus, "sub-owner", &format!("{EVENT_EXIT}.{owner}")).await;
        let other_events = subscribe(&ctx.message_bus, "sub-other", &format!("{EVENT_EXIT}.{other}")).await;

        let mine = pty_spawn(&ctx, owner, ALIVE).expect("spawn owner");
        let theirs = pty_spawn(&ctx, other, ALIVE).expect("spawn other");

        pty_kill(&ctx, owner, &mine).expect("kill");
        let event = wait_event(&owner_events).await.expect("属主收到自己的事件");
        assert_eq!(event["payload"]["ptyId"], mine);
        assert!(
            other_events.try_recv().is_err(),
            "非属主 topic 物理上收不到他人事件（topic 内嵌属主）"
        );
        // 恰好一次：监听任务与停用回收之外不再有第二个发布者
        assert!(
            wait_event_timeout(&owner_events, Duration::from_secs(1))
                .await
                .is_none(),
            "一条 PTY 只能有一条退出事件（不重放、不双发）"
        );
        // 它插件的句柄不受本次 kill 影响
        assert!(
            pty_is_running(&ctx, other, &theirs).expect("他人句柄应仍可查询"),
            "kill 只作用于属主自己的进程"
        );
    }

    /// 正例：停用回收 kill 并摘除本人全部 PTY，逐条补发事件，且只碰本人
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn purge_for_plugin_retires_all_owned_handles_and_touches_nobody_else() {
        let owner = "com.bedcode.purge-owner";
        let other = "com.bedcode.purge-other";
        let ctx = build_host_ctx();
        grant_permissions(&ctx, owner, &[PERMISSION_PTY_SPAWN, PERMISSION_PTY_IO]);
        grant_permissions(&ctx, other, &[PERMISSION_PTY_SPAWN, PERMISSION_PTY_IO]);

        let first = pty_spawn(&ctx, owner, ALIVE).expect("spawn 1");
        let second = pty_spawn(&ctx, owner, r#"{"command":"/bin/cat"}"#).expect("spawn 2");
        let theirs = pty_spawn(&ctx, other, ALIVE).expect("spawn peer");
        let owner_events = subscribe(&ctx.message_bus, "sub-purge", &format!("{EVENT_EXIT}.{owner}")).await;
        let other_events = subscribe(&ctx.message_bus, "sub-purge-peer", &format!("{EVENT_EXIT}.{other}")).await;

        assert_eq!(
            purge_for_plugin(owner, &ctx.message_bus),
            2,
            "回收数应为本人全部在册 PTY"
        );

        // 本人：注册表清空、进程终止、逐条补发 reason=killed
        // （事件顺序按注册表遍历，HashMap 不承诺——故断言**集合**而非序列）
        assert_eq!(registered_count_for(owner), 0, "停用回收不得留残项");
        let mut received: Vec<String> = Vec::new();
        for _ in 0..2 {
            let event = wait_event(&owner_events).await.expect("每条 PTY 各一条补发事件");
            assert_eq!(event["payload"]["reason"], "killed", "停用即宿主代为终止: {event}");
            received.push(event["payload"]["ptyId"].as_str().unwrap_or_default().to_string());
        }
        received.sort();
        let mut want = vec![first.clone(), second.clone()];
        want.sort();
        assert_eq!(received, want, "回收必须逐条寻址到本人每一条 PTY");
        assert!(
            wait_event_timeout(&owner_events, Duration::from_secs(1))
                .await
                .is_none(),
            "恰好一次：停用回收与退出监听不得对同一 PTY 各发一条"
        );

        // 它插件：句柄在册、可用，且收不到任何补发事件
        assert_eq!(registered_count_for(other), 1, "回收越界碰了他插件的句柄");
        assert!(
            pty_is_running(&ctx, other, &theirs).expect("他人句柄仍可查"),
            "他人进程必须未被 kill"
        );
        assert!(other_events.try_recv().is_err(), "非属主收不到他人的回收事件");

        purge_for_plugin(other, &ctx.message_bus);
    }

    /// 反例：spawn 失败路径零事件、零句柄（无句柄可寻址，spec D5）
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn failed_spawn_publishes_no_event_and_registers_nothing() {
        let owner = "com.bedcode.spawn-fail";
        let ctx = build_host_ctx();
        grant_permissions(&ctx, owner, &[PERMISSION_PTY_SPAWN, PERMISSION_PTY_IO]);
        let events = subscribe(&ctx.message_bus, "sub-fail", &format!("{EVENT_EXIT}.{owner}")).await;

        let err = pty_spawn(&ctx, owner, r#"{"command":"/nonexistent/bedcode-pty-command"}"#).unwrap_err();
        assert!(
            err.contains("启动子进程失败") || err.contains("打开伪终端失败"),
            "spawn 失败必须带操作上下文: {err}"
        );
        assert_eq!(registered_count_for(owner), 0, "失败不得留句柄");
        assert!(
            wait_event_timeout(&events, Duration::from_millis(300)).await.is_none(),
            "失败路径不得发布任何事件（回归票 02 契约）"
        );
    }

    /// 短窗口等待（负向断言用：等满即确认「没有投递」）
    async fn wait_event_timeout(
        rx: &std::sync::mpsc::Receiver<serde_json::Value>,
        wait: Duration,
    ) -> Option<serde_json::Value> {
        tokio::time::timeout(wait, async {
            loop {
                if let Ok(event) = rx.try_recv() {
                    return Some(event);
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_default()
    }

    // ==================== 数据面（票 03：write / resize / is-running） ====================

    /// 反例：只授创建域（pty:spawn）→ 数据面三函数一律拒绝（两域独立）
    #[cfg(target_os = "linux")]
    #[test]
    fn io_apis_without_io_permission_are_denied() {
        let ctx = build_host_ctx();
        grant_permissions(&ctx, "com.bedcode.spawn-only", &[PERMISSION_PTY_SPAWN]);
        let pty_id = pty_spawn(&ctx, "com.bedcode.spawn-only", ALIVE).expect("spawn");

        assert_eq!(
            pty_write(&ctx, "com.bedcode.spawn-only", &pty_id, b"x").unwrap_err(),
            "permission denied: pty:io"
        );
        assert_eq!(
            pty_resize(&ctx, "com.bedcode.spawn-only", &pty_id, 80, 24).unwrap_err(),
            "permission denied: pty:io"
        );
        assert_eq!(
            pty_is_running(&ctx, "com.bedcode.spawn-only", &pty_id).unwrap_err(),
            "permission denied: pty:io"
        );
    }

    /// 正例：write 的字节真的进了进程并被进程变换后回读（`sed` 前缀证明非 tty 回显）
    #[cfg(target_os = "linux")]
    #[test]
    fn write_response_comes_from_process_not_tty_echo() {
        let marker = unique_tag("SED");
        let ctx = ctx_with_pty("com.bedcode.sed", r#"{"command":"/bin/sed","args":["s/^/OUT:/"]}"#);
        let pty_id = find_handle_of("com.bedcode.sed");

        pty_write(&ctx, "com.bedcode.sed", &pty_id, format!("body-{marker}\n").as_bytes()).expect("write");
        let output = wait_for_output(&ring_of(&pty_id), &format!("OUT:body-{marker}"));
        assert!(
            output.contains(&format!("OUT:body-{marker}")),
            "OUT: 前缀只可能由进程加上（tty 回显不带前缀）: {output}"
        );
    }

    /// 边界：恰好等于上限放行（`>` 的另一侧），且整块字节确实落到进程
    #[cfg(target_os = "linux")]
    #[test]
    fn write_at_limit_is_accepted_and_delivered_whole() {
        let ctx = ctx_with_pty("com.bedcode.at-limit", r#"{"command":"/bin/cat"}"#);
        let pty_id = find_handle_of("com.bedcode.at-limit");

        // 载荷全部由短行组成：canonical 模式的内核输入队列按行放行（MAX_CANON ~4 KiB），
        // 64 KiB 无换行的整块写入会卡在读端，测不到准入判定本身
        let mut payload: Vec<u8> = Vec::with_capacity(PLUGIN_PTY_MAX_WRITE_BYTES);
        while payload.len() < PLUGIN_PTY_MAX_WRITE_BYTES {
            let fill = (PLUGIN_PTY_MAX_WRITE_BYTES - payload.len() - 1).min(59);
            payload.extend(std::iter::repeat_n(b'z', fill));
            payload.push(b'\n');
        }
        assert_eq!(payload.len(), PLUGIN_PTY_MAX_WRITE_BYTES, "边界载荷必须恰好等于上限");

        pty_write(&ctx, "com.bedcode.at-limit", &pty_id, &payload).expect("等于上限必须放行");

        // cat 原样回吐：环内驻留字节达到写入量即证明「没被截断成半块」
        let ring = ring_of(&pty_id);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let resident = ring.lock().unwrap_or_else(|e| e.into_inner()).resident_bytes();
            if resident >= PLUGIN_PTY_MAX_WRITE_BYTES as u64 || Instant::now() >= deadline {
                assert!(
                    resident >= PLUGIN_PTY_MAX_WRITE_BYTES as u64,
                    "等于上限的写入应整块送达，实际环内驻留 {resident} 字节"
                );
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// 反例：write 超单次上限 → 明确错误且不写入（不静默截断）
    #[cfg(target_os = "linux")]
    #[test]
    fn write_over_limit_is_rejected_without_partial_input() {
        let ctx = ctx_with_pty(
            "com.bedcode.limit",
            // 收到一行才回显标记：据此判定「前一次超限写入没有半个字节漏进去」；
            // 末尾再阻塞一次，保证断言窗口内句柄不被退出监听摘走（票 04）
            r#"{"command":"/bin/sh","args":["-c","read line; echo PASSED; read go"]}"#,
        );
        let pty_id = find_handle_of("com.bedcode.limit");
        let oversized = vec![b'x'; PLUGIN_PTY_MAX_WRITE_BYTES + 1];

        let err = pty_write(&ctx, "com.bedcode.limit", &pty_id, &oversized).unwrap_err();
        assert!(err.contains("too large") && err.contains("limit"), "got: {err}");
        assert!(
            err.contains(&PLUGIN_PTY_MAX_WRITE_BYTES.to_string()),
            "错误必须带上限常量（不静默截断）: {err}"
        );

        // 被拒的负载不得有任何字节进入进程 stdin：随后一行合法写入应能被 `read` 取到
        // （若超限负载被部分写入，`read line` 会先消费残渣，PASSED 就永远不来）
        pty_write(&ctx, "com.bedcode.limit", &pty_id, b"go\n").expect("等于/低于上限应放行");
        let output = wait_for_output(&ring_of(&pty_id), "PASSED");
        assert!(output.contains("PASSED"), "超限拒绝必须零副作用: {output}");
    }

    /// 正例：resize 改变内核终端尺寸（进程侧 `stty size` 反查 30x100 → 24x80）
    #[cfg(target_os = "linux")]
    #[test]
    fn resize_changes_kernel_winsize_observed_by_process() {
        let ctx = ctx_with_pty(
            "com.bedcode.resize",
            // 插件自己要求 shell 包装（业务性包装归插件层，宿主不做）：
            // 先报一次尺寸，然后阻塞在 read——第二次报尺寸由本用例 write 解锁，
            // 因此「resize 已生效」与「第二次读取」之间无竞态
            r#"{"command":"/bin/sh","args":["-c","stty size; read go; stty size"],"cols":100,"rows":30}"#,
        );
        let pty_id = find_handle_of("com.bedcode.resize");
        let before = wait_for_output(&ring_of(&pty_id), "30 100");
        assert!(before.contains("30 100"), "spawn 尺寸应为 30x100: {before}");

        pty_resize(&ctx, "com.bedcode.resize", &pty_id, 80, 24).expect("resize");
        pty_write(&ctx, "com.bedcode.resize", &pty_id, b"go\n").expect("write 解锁第二次读取");
        let after = wait_for_output(&ring_of(&pty_id), "24 80");
        assert!(after.contains("24 80"), "resize 后进程应观察到 24x80: {after}");
    }

    /// 契约：`is-running` 判据四格真值表（票 03 C-②的确定性锁）
    ///
    /// 「进程自然退出但句柄尚未摘除」这一格在端到端上只有毫秒窗口（EOF 后退出监听
    /// 立即摘环，票 04），故把组合判定做成纯函数逐格断言：去掉 `!output_terminated`
    /// 的变异必须在此翻红，否则该判据就退化成「只信 `running` 标志」——那会让插件
    /// 在丢事件时永远看到一个死进程是活的。
    #[test]
    fn is_running_verdict_covers_all_four_states() {
        assert!(running_verdict(true, false), "活着且输出未终结 → true");
        assert!(!running_verdict(false, false), "已被要求终止 → false");
        assert!(
            !running_verdict(true, true),
            "running 未翻但读线程已 EOF（自然退出的那一格）→ 必须 false"
        );
        assert!(!running_verdict(false, true), "两路都终结 → false");
    }

    /// 正例 + 状态迁移：进程存活期 is-running=true，自然退出后句柄被摘除且不再可寻址
    ///
    /// 判据组合本身见 [`is_running_verdict_covers_all_four_states`]；本用例验端到端
    /// 的两端确定态：alive=true，退出后退出监听摘除句柄（票 04「exit 即摘除」）⇒
    /// `Err(not found)`。
    #[cfg(target_os = "linux")]
    #[test]
    fn is_running_true_while_alive_and_handle_retired_after_natural_exit() {
        let owner = "com.bedcode.running";
        let ctx = ctx_with_pty(owner, ALIVE);
        let pty_id = find_handle_of(owner);
        assert!(
            pty_is_running(&ctx, owner, &pty_id).expect("is-running"),
            "阻塞在 read 的进程应为 running"
        );

        // 解锁 → shell 自然退出 → 终态事件 → 句柄摘除
        pty_write(&ctx, owner, &pty_id, b"go\n").expect("write");
        let deadline = Instant::now() + Duration::from_secs(5);
        while is_registered(&pty_id) {
            if Instant::now() >= deadline {
                panic!("自然退出后句柄必须被退出监听摘除，不得永久在册");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let err = pty_is_running(&ctx, owner, &pty_id).unwrap_err();
        assert!(err.contains("not found"), "摘除后必须不可寻址，got: {err}");
    }

    // ==================== 限额与背压（票 05） ====================

    /// 持续产出的流式载体（`stty -echo` 关掉 tty 回显，环内内容即进程产出）
    #[cfg(target_os = "linux")]
    fn streaming_config(ring_bytes: u64) -> String {
        format!(
            r#"{{"command":"/bin/sh","args":["-c","stty -echo; while true; do echo line; sleep 0.001; done"],"ringBytes":{ring_bytes}}}"#
        )
    }

    /// 回显进程载体的就绪标记（`stty` 与 `cat` 之间的同步点，见 `quiet_cat_config`）
    #[cfg(target_os = "linux")]
    const STTY_READY: &str = "STTY_READY";

    /// 回显进程载体（写入什么就产出什么，可逐字节比对）
    ///
    /// 必须等 `stty` 真的生效后再灌载荷：`stty -echo -onlcr` 与 `cat` 是 fork 后异步
    /// 执行的，若在它生效前写入，前半段会被 tty 驱动回显一遍并做 `\n → \r\n` 改写，
    /// 字节级比对就会挂。故以 `echo STTY_READY` 作为同步点（关掉回显后它只出现一次）。
    #[cfg(target_os = "linux")]
    fn quiet_cat_config() -> &'static str {
        r#"{"command":"/bin/sh","args":["-c","stty -echo -onlcr; echo STTY_READY; cat"]}"#
    }

    /// 由短行拼装的 N 字节载荷（canonical 终端下行块才不会卡在内核输入队列，
    /// 与票 03 的边界用例同一约束）
    #[cfg(target_os = "linux")]
    fn line_payload(len: usize) -> Vec<u8> {
        let mut payload: Vec<u8> = Vec::with_capacity(len);
        let mut i = 0usize;
        while payload.len() < len {
            let fill = (len - payload.len() - 1).min(50);
            for _ in 0..fill {
                payload.push(b'a' + (i % 26) as u8);
                i += 1;
            }
            payload.push(b'\n');
        }
        payload
    }

    /// 轮询环直至谓词命中（真 PTY 产出异步：断言最终事实，不断言时序）
    #[cfg(target_os = "linux")]
    fn wait_until(ring: &Arc<Mutex<PtyRing>>, pred: impl Fn(&PtyRing) -> bool, why: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            {
                let guard = ring.lock().unwrap_or_else(|e| e.into_inner());
                if pred(&guard) {
                    return;
                }
            }
            assert!(Instant::now() < deadline, "{why}（5s 内未达成）");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// 反例：每插件在册数达上限 → `Err`（fail-visible：不排队、不静默淘汰旧句柄），
    /// 配额按属主隔离，且句柄回收后额度归还
    #[cfg(target_os = "linux")]
    #[test]
    fn pty_quota_is_per_plugin_and_rejects_overflow_without_side_effects() {
        let owner = "com.bedcode.quota";
        let peer = "com.bedcode.quota-peer";
        let ctx = build_host_ctx();
        grant_permissions(&ctx, owner, &[PERMISSION_PTY_SPAWN, PERMISSION_PTY_IO]);
        grant_permissions(&ctx, peer, &[PERMISSION_PTY_SPAWN, PERMISSION_PTY_IO]);

        for _ in 0..PLUGIN_PTY_MAX_SESSIONS_PER_PLUGIN {
            pty_spawn(&ctx, owner, ALIVE).expect("上限之内必须放行");
        }
        assert_eq!(registered_count_for(owner), PLUGIN_PTY_MAX_SESSIONS_PER_PLUGIN);

        let err = pty_spawn(&ctx, owner, ALIVE).unwrap_err();
        assert!(err.contains("too many ptys"), "超限必须明确报错，got: {err}");
        assert!(
            err.contains(&PLUGIN_PTY_MAX_SESSIONS_PER_PLUGIN.to_string()),
            "错误必须带上限常量: {err}"
        );
        assert_eq!(
            registered_count_for(owner),
            PLUGIN_PTY_MAX_SESSIONS_PER_PLUGIN,
            "超限拒绝不得留下第 9 条，也不得动既有句柄"
        );

        // 配额按属主计：同宿主另一插件此刻仍可创建并正常查询
        let theirs = pty_spawn(&ctx, peer, ALIVE).expect("他插件不受该配额影响");
        assert!(pty_is_running(&ctx, peer, &theirs).expect("他插件句柄可用"));

        // 回收即归还额度
        let victim = find_handle_of(owner);
        pty_kill(&ctx, owner, &victim).expect("kill 腾出额度");
        let deadline = Instant::now() + Duration::from_secs(5);
        while registered_count_for(owner) >= PLUGIN_PTY_MAX_SESSIONS_PER_PLUGIN && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        pty_spawn(&ctx, owner, ALIVE).expect("腾出额度后同一属主必须可再建");

        purge_for_plugin(owner, &ctx.message_bus);
        purge_for_plugin(peer, &ctx.message_bus);
    }

    /// 反例：`ringBytes` 为 0 或超宿主上限 → `Err`（不夹取、不降级），零副作用
    /// （配额类判定在开 fd / 起进程之前完成）
    #[test]
    fn declared_ring_bytes_out_of_range_is_rejected_without_side_effects() {
        let owner = "com.bedcode.range";
        let ctx = build_host_ctx();
        grant_permissions(&ctx, owner, &[PERMISSION_PTY_SPAWN, PERMISSION_PTY_IO]);

        for bad in [0u64, PLUGIN_PTY_RING_MAX_BYTES + 1] {
            let err = pty_spawn(&ctx, owner, &format!(r#"{{"command":"/bin/true","ringBytes":{bad}}}"#)).unwrap_err();
            assert!(err.contains("ringBytes"), "错误必须点名被拒的参数，got: {err}");
            assert!(
                err.contains(&bad.to_string()) || err.contains("greater than 0"),
                "错误必须带上被拒的值（不静默夹取）: {err}"
            );
        }
        assert_eq!(registered_count_for(owner), 0, "容量非法不得留下任何句柄");

        // 恰等于上限的声明必须放行（off-by-one 的另一侧）
        pty_spawn(
            &ctx,
            owner,
            &format!(r#"{{"command":"/bin/sh","args":["-c","read go"],"ringBytes":{PLUGIN_PTY_RING_MAX_BYTES}}}"#),
        )
        .expect("等于上限必须放行");
        purge_for_plugin(owner, &ctx.message_bus);
    }

    /// 背压契约：小环 + 从不拉取的消费者 → 产出持续推进，落后游标得到 truncated 并可续拉
    ///
    /// 「源侧零等待」在环这一维的可观测判据是**产出偏移持续增长**（读线程从不因消费者
    /// 停摆；无丢帧属引擎侧，票 01 的 `pty_reader` 用例覆盖）。
    #[cfg(target_os = "linux")]
    #[test]
    fn small_declared_ring_evicts_for_a_never_fetching_consumer_without_stalling_output() {
        let owner = "com.bedcode.backpressure";
        let ctx = build_host_ctx();
        grant_permissions(&ctx, owner, &[PERMISSION_PTY_SPAWN, PERMISSION_PTY_IO]);
        // 环只 512 字节，进程每 ~1ms 产出一行 → 必然远超容量
        let pty_id = pty_spawn(&ctx, owner, &streaming_config(512)).expect("spawn");
        let ring = ring_of(&pty_id);

        // 消费者全程不拉取，直到产出远超环容量
        wait_until(
            &ring,
            |r| r.watermarks().1 > 8 * 512,
            "产出必须持续增长（源侧未被拖住）",
        );

        // 落后于环起点的游标：得到现存最早段 + truncated + 可续拉游标
        let stale = pty_ring_fetch(&ctx, owner, &pty_id, 0, 4096)
            .expect("ring-fetch")
            .expect("驻留非空");
        assert!(stale.truncated, "产出远超容量，游标 0 必然落后于驻留起点");
        assert!(
            stale.next_offset >= stale.data.len() as u64,
            "载荷与续拉游标自洽: {stale:?}"
        );
        assert!(
            stale.next_offset - stale.data.len() as u64 > 0,
            "返回段必须从产出中段起（此前的字节已淘汰）: {stale:?}"
        );
        assert!(
            stale.data.len() <= PLUGIN_PTY_RING_FETCH_MAX_BYTES as usize,
            "单次返回不得越界拷贝: {}",
            stale.data.len()
        );
        assert!(
            String::from_utf8_lossy(&stale.data).contains("line"),
            "返回的必须是真实输出段"
        );

        // 续拉：游标已在驻留区间内，不再报缺口且必须单调前进
        if let Some(more) = pty_ring_fetch(&ctx, owner, &pty_id, stale.next_offset, 4096).expect("续拉") {
            assert!(!more.truncated, "从 next-offset 起续拉不得再报缺口: {more:?}");
            assert!(more.next_offset > stale.next_offset, "游标必须前进: {more:?}");
        }

        pty_kill(&ctx, owner, &pty_id).expect("kill 清场");
    }

    /// 单次读上限：`max-bytes` 超宿主值即截断（数据面不报错），按游标可拉全量
    #[cfg(target_os = "linux")]
    #[test]
    fn ring_fetch_is_capped_per_call_and_resumes_to_the_end() {
        let owner = "com.bedcode.fetch-cap";
        let ctx = build_host_ctx();
        grant_permissions(&ctx, owner, &[PERMISSION_PTY_SPAWN, PERMISSION_PTY_IO]);
        let pty_id = pty_spawn(&ctx, owner, quiet_cat_config()).expect("spawn");
        let ring = ring_of(&pty_id);
        // 同步点：`stty` 生效后才会打印就绪标记（在此之前写入的载荷会被 tty 驱动回显
        // 一遍并做 `\n → \r\n` 改写，字节级比对就不成立）
        wait_for_output(&ring, STTY_READY);
        let prefix = resident_text(&ring);
        let produced = {
            let guard = ring.lock().unwrap_or_else(|e| e.into_inner());
            guard.watermarks().1
        };
        assert_eq!(prefix.len() as u64, produced, "此刻驻留必须等于全部产出（尚未淘汰）");

        // 40 KiB 一次性写入（低于 64 KiB 准入上限；默认环 256 KiB 容得下，不触发淘汰）
        let payload = line_payload(40 * 1024);
        pty_write(&ctx, owner, &pty_id, &payload).expect("write");
        let expected: Vec<u8> = [prefix.as_bytes(), &payload].concat();
        wait_until(
            &ring,
            |r| r.watermarks().1 >= produced + payload.len() as u64,
            "全部产出必须已入环",
        );

        let mut cursor = 0u64;
        let mut reassembled: Vec<u8> = Vec::new();
        let mut calls = 0usize;
        // `max_bytes: u32::MAX` 即「取宿主允许的一批」，用于验截断常量本身
        while let Some(fetched) = pty_ring_fetch(&ctx, owner, &pty_id, cursor, u32::MAX).expect("ring-fetch") {
            calls += 1;
            assert!(!fetched.truncated, "环容量足够，全程不该有缺口（calls={calls}）");
            assert!(
                fetched.data.len() <= PLUGIN_PTY_RING_FETCH_MAX_BYTES as usize,
                "单次拷贝必须被截到上限，got {}",
                fetched.data.len()
            );
            if calls == 1 {
                assert_eq!(
                    fetched.data.len(),
                    PLUGIN_PTY_RING_FETCH_MAX_BYTES as usize,
                    "首批必须**正好**被截到上限（截断生效，而不是整块返回或报错）"
                );
            }
            assert!(fetched.next_offset > cursor, "游标必须前进，否则续拉死循环");
            reassembled.extend_from_slice(&fetched.data);
            cursor = fetched.next_offset;
            assert!(calls < 64, "拉取轮次异常，疑似游标不前进");
        }

        assert_eq!(
            calls,
            expected.len().div_ceil(PLUGIN_PTY_RING_FETCH_MAX_BYTES as usize),
            "截断轮次必须等于「总量 / 单次上限」（向上取整）"
        );
        assert_eq!(
            reassembled, expected,
            "逐字节重组必须等于进程全部产出（截断不丢不改序）"
        );

        pty_kill(&ctx, owner, &pty_id).expect("kill 清场");
    }

    // ==================== spawn → ring-fetch 主干（真 PTY） ====================

    /// 正例：spawn 真实命令 → 输出经环形缓冲被 `ring-fetch` 拉到，游标可续
    #[cfg(target_os = "linux")]
    #[test]
    fn spawn_runs_real_command_and_output_reaches_ring_fetch() {
        let marker = unique_tag("RING");
        let pty_id = spawn_ok("com.bedcode.ring", &alive_with_output(&marker));
        assert!(pty_id.starts_with("pty-"), "句柄形状应为 pty-<uuid>，got: {pty_id}");
        let output = wait_for_output(&ring_of(&pty_id), &marker);
        assert!(output.contains(&marker), "真 PTY 输出必须落入环: {output}");

        let ctx = build_host_ctx();
        grant_permissions(&ctx, "com.bedcode.ring", &[PERMISSION_PTY_SPAWN, PERMISSION_PTY_IO]);
        let fetched = pty_ring_fetch(&ctx, "com.bedcode.ring", &pty_id, 0, 4096)
            .expect("ring-fetch")
            .expect("有产出时不得返回 None");
        assert!(
            String::from_utf8_lossy(&fetched.data).contains(&marker),
            "宿主 ring-fetch 应答里必须有输出: {:?}",
            String::from_utf8_lossy(&fetched.data)
        );
        assert!(!fetched.truncated, "首次全量拉取不存在缺口");
        assert_eq!(fetched.next_offset, fetched.data.len() as u64);
    }

    /// 正例：二次按 `next_offset` 续拉不重复已消费字节（游标契约）
    #[cfg(target_os = "linux")]
    #[test]
    fn second_fetch_from_next_offset_returns_nothing_new() {
        let marker = unique_tag("CONT");
        let ctx = ctx_with_pty("com.bedcode.cursor", &alive_with_output(&marker));
        let pty_id = find_handle_of("com.bedcode.cursor");
        wait_for_output(&ring_of(&pty_id), &marker);

        let first = pty_ring_fetch(&ctx, "com.bedcode.cursor", &pty_id, 0, 4096)
            .unwrap()
            .expect("首批");
        let second = pty_ring_fetch(&ctx, "com.bedcode.cursor", &pty_id, first.next_offset, 4096).unwrap();
        assert!(
            second.map(|f| f.data).unwrap_or_default().is_empty(),
            "游标已追平时不得重复投递已消费字节"
        );
    }

    /// 裁剪线证据：参数数组原样 exec，宿主不做 shell 解释（分号不构成第二条命令）
    ///
    /// 载体用常驻 `sed`（票 04 后短命命令会立刻被摘环）：宿主若做 shell 包装
    /// （`sh -c "sed s/^/literal; echo PWNED"`），sed 的参数会被截断为 `s/^/literal;`、
    /// `echo PWNED` 另行执行，连续字面量 `literal; echo PWNED` 就不会出现在输出里。
    #[cfg(target_os = "linux")]
    #[test]
    fn spawn_executes_argv_verbatim_without_shell_interpretation() {
        let ctx = ctx_with_pty(
            "com.bedcode.argv",
            r#"{"command":"/bin/sed","args":["s/^/literal; echo PWNED/"]}"#,
        );
        let pty_id = find_handle_of("com.bedcode.argv");
        pty_write(&ctx, "com.bedcode.argv", &pty_id, b"body\n").expect("write");

        let output = wait_for_output(&ring_of(&pty_id), "literal");
        assert!(
            output.contains("literal; echo PWNEDbody"),
            "argv 必须原样 exec: {output}"
        );
        assert!(
            !output.contains("\nPWNED\r\n") && !output.contains("PWNEDbody\nPWNED"),
            "宿主不得做 shell 解析（分号后的 echo 不应被执行）: {output}"
        );
    }

    /// 引擎参数证据：声明的 env 生效、业务 `BEDCODE_SESSION_ID` 不注入
    #[cfg(target_os = "linux")]
    #[test]
    fn spawn_applies_declared_env_without_business_identity() {
        let owner = "com.bedcode.env";
        let marker = unique_tag("ENV");
        let ctx = ctx_with_pty(
            owner,
            &format!(r#"{{"command":"/bin/sh","args":["-c","env; read go"],"env":{{"BEDCODE_PTY_TEST":"{marker}"}}}}"#),
        );
        let pty_id = find_handle_of(owner);
        let env_output = wait_for_output(&ring_of(&pty_id), &marker);
        assert!(
            env_output.contains(&format!("BEDCODE_PTY_TEST={marker}")),
            "声明的 env 必须进子进程: {env_output}"
        );
        assert!(
            !env_output.contains("BEDCODE_SESSION_ID"),
            "插件私有 PTY 不得带业务会话身份: {env_output}"
        );
        assert!(
            pty_is_running(&ctx, owner, &pty_id).expect("is-running"),
            "载体进程应仍存活（env 输出后阻塞在 read）"
        );
    }

    /// 引擎参数证据：cols/rows 真的作用到终端尺寸（子进程 `stty size` 反查）
    #[cfg(target_os = "linux")]
    #[test]
    fn spawn_applies_requested_terminal_size() {
        let pty_id = spawn_ok(
            "com.bedcode.size",
            r#"{"command":"/bin/sh","args":["-c","stty size; read go"],"cols":100,"rows":30}"#,
        );
        let output = wait_for_output(&ring_of(&pty_id), "30 100");
        assert!(
            output.contains("30 100"),
            "stty 报告尺寸应为 rows=30 cols=100，got: {output}"
        );
    }

    // ==================== 业务线零感知 ====================

    /// 正例：插件 PTY 既不进业务输出注册表，也不进业务会话表（业务线零感知）
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn spawned_pty_is_absent_from_business_session_lines() {
        use crate::session::GlobalOutputManager;

        let marker = unique_tag("ISOLATE");
        let ctx = ctx_with_pty("com.bedcode.isolate", &alive_with_output(&marker));
        let pty_id = find_handle_of("com.bedcode.isolate");
        wait_for_output(&ring_of(&pty_id), &marker);

        assert!(
            !GlobalOutputManager::global().has_session(&pty_id).await,
            "插件私有 PTY 不得注册进业务输出总线"
        );
        assert!(
            ctx.session_manager.get_session(&pty_id).await.is_none(),
            "插件私有 PTY 不得出现在业务会话表"
        );
        assert!(
            ctx.session_manager.list_sessions().await.is_empty(),
            "业务会话列表必须零感知"
        );
    }

    /// 句柄是否仍在册（测试断言副作用用）
    #[cfg(target_os = "linux")]
    fn is_registered(pty_id: &str) -> bool {
        PTYS.lock().unwrap_or_else(|e| e.into_inner()).contains_key(pty_id)
    }

    /// 在册句柄归属查询（测试断言副作用用）
    fn find_handle_of(plugin_id: &str) -> String {
        PTYS.lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|(_, entry)| entry.owner == plugin_id)
            .map(|(pty_id, _)| pty_id.clone())
            .expect("该插件应有在册 PTY")
    }
}
