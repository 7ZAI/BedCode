//! 任务域（票 15-16）：Agent 集成与会话状态 + 队列/定时后端
//!
//! 自 `com.bedcode.auto-task` 插件搬迁（spec D8-P4「任务域并入」），按域重组为
//! 本模块目录，不做逐文件平移（D1）。本插件后端只经宿主 `host-*` 基础服务取能力
//! （D2 红线）：文件读写（`host-fs`）、插件私有库（`host-plugin-database`）、事件
//! 广播（`host-events` / `host-bus`）、配置读取（`host-config`）、定时器
//! （`host-timer`）——不新增任何宿主命令。
//!
//! 模块构成（对应 auto-task 六模块，按依赖序）：
//! - [`agent`]：Agent 能力 registry（命令过滤 / agent 识别 / 会话集成形态）
//! - [`hooks`]：Agent 集成（多 Agent hooks 安装 / 清理 / 模板版本触发重写）
//! - [`state`]：会话状态与会话映射 + 任务状态记录/广播 + 自动授权模式
//! - [`queue`]：任务队列调度与广播（随 state 一起搬迁——state↔queue 循环依赖，
//!   无法分开编译；票据 16 完成定时驱动部分与 HTTP 面）
//! - [`preset`]：预设任务（一次性消耗，入队后删除）
//!
//! **编排反转（票 15 硬性验收）**：本插件是会话编排方——会话创建由本插件经
//! `host-session.create-with-spec` 发起（launch 域），集成注入（写 hooks）由本插件
//! 自己驱动：activate 注册生命周期监听器，`on-session-lifecycle(Creating)` 即调用
//! [`hooks::ensure_agent_integration`]（resource_dir 由宿主在事件 payload 注入）。
//! 反向回调兼容面（`terminal-hooks` / `on-session-lifecycle` / `on-input-submitted`）
//! 保留在 SDK，但本插件不再依赖它驱动主流程——创建编排在插件侧，回调只是资源
//! 目录与其它创建路径（移动端 HTTP）的传输载体。
//!
//! **async 化（票 15 硬性验收）**：wasip3 async store 内整批文件遍历与长循环必须
//! 周期让出（调用宿主原语即 await 点），禁止无让出的阻塞循环拖垮同实例配对回调
//! （D7）。长循环统一经 [`yield_guard`] 计数让出。
//!
//! **错误隔离（票 15 硬性验收）**：三域（配对/信任、会话/配置、任务）各自 `Result`
//! 边界、禁止跨域持锁；定时器回调按命令名分域计数失败，失败只降级本域。

pub mod agent;
pub mod hooks;
pub mod preset;
pub mod queue;
pub mod scheduled;
pub mod state;

use bedcode_plugin_api::host::{ConfigKey, HostConfig, HostLog, HostPluginDatabase};
use bedcode_plugin_api::wasm_host::WasmHost;
use serde::Serialize;

/// 长循环让出护栏（WASI 0.3 async 语义）
///
/// wasmtime async store 中宿主 import 调用是唯一的 await 点：纯 guest 计算循环
/// 不触发让出，长时间无宿主调用会阻塞同一 store 上的配对回调（D7 错误隔离——
/// 配对不得阻塞任务 tick）。本护栏每 `TASK_YIELD_INTERVAL` 次调用一次廉价宿主
/// 原语（`config_get(CurrentTimeMs)`，白名单键、无副作用）制造 await 点。
///
/// 用法：`let mut counter = 0u64; for ... { ...; yield_guard(host, &mut counter); }`
pub fn yield_guard(host: &WasmHost, counter: &mut u64) {
    *counter += 1;
    if *counter % TASK_YIELD_INTERVAL == 0 {
        // 仅需让出：忽略返回值（读宿主时钟无副作用，失败也不影响任务逻辑）
        let _ = host.config_get(ConfigKey::CurrentTimeMs);
    }
}

/// 让出频率：每 64 次迭代让出一次（host 调用本身廉价；过高频会放大宿主往返）
pub const TASK_YIELD_INTERVAL: u64 = 64;

/// 任务历史表建表 SQL（按语句拆分，宿主 plugin_db_execute 单语句执行）
///
/// 与 auto-task 原 schema 逐字一致（对照测试断言同一输入 → 同一写入结果，
/// 表结构漂移会破坏等价性）。
pub const TASK_HISTORY_SCHEMA: &[&str] = &[
    r#"
CREATE TABLE IF NOT EXISTS task_history (
    id              TEXT PRIMARY KEY,
    description     TEXT,
    status          TEXT NOT NULL DEFAULT 'pending',
    agent           TEXT,
    source          TEXT,
    session_id      TEXT,
    claude_sid      TEXT,
    working_dir     TEXT,
    exit_reason     TEXT,
    questions       TEXT,
    auto_approve    INTEGER DEFAULT 0,
    event_time      TEXT,
    input_tokens    INTEGER,
    output_tokens   INTEGER,
    created_at      TEXT NOT NULL,
    started_at      TEXT,
    completed_at    TEXT,
    updated_at      TEXT NOT NULL
)"#,
    "CREATE INDEX IF NOT EXISTS idx_task_history_status ON task_history(status)",
    "CREATE INDEX IF NOT EXISTS idx_task_history_session_id ON task_history(session_id)",
    "CREATE INDEX IF NOT EXISTS idx_task_history_created_at ON task_history(created_at)",
];

/// Claude Code session ↔ BedCode PTY session 映射表建表 SQL（按语句拆分）
///
/// 表名统一为 `task_session_mapping`（票 16 / D5）：它是**任务域**的记账（Agent 会话
/// ↔ 床码会话），不是会话引擎的表；旧名由 `crate::schema` 的幂等重命名迁移改过来。
pub const SESSION_MAPPING_SCHEMA: &[&str] = &[
    r#"
CREATE TABLE IF NOT EXISTS task_session_mapping (
    claude_sid  TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL,
    created_at  TEXT NOT NULL
)"#,
    "CREATE INDEX IF NOT EXISTS idx_task_session_mapping_session ON task_session_mapping(session_id)",
];

/// 会话级开关表建表 SQL（按语句拆分）
///
/// auto_execute（自动执行）/ auto_answer（自动应答）两个独立开关。表名带 `task_`
/// 前缀的理由同 [`SESSION_MAPPING_SCHEMA`]——HTTP 端点 `session-settings` 是**线协议**，
/// 一个字符都不改（票 16），与私有库表名无关。
pub const SESSION_SETTINGS_SCHEMA: &[&str] = &[r#"
CREATE TABLE IF NOT EXISTS task_session_settings (
    session_id   TEXT PRIMARY KEY,
    auto_execute INTEGER NOT NULL DEFAULT 0,
    auto_answer  INTEGER NOT NULL DEFAULT 0,
    updated_at   TEXT NOT NULL
)"#];

/// 定时自动任务表建表 SQL（按语句拆分）
///
/// 状态机：pending（待触发）→ creating（会话已创建，等 Created 事件入队）
/// → executed（prompts 已入队）；failed（会话创建失败）/ missed（错过不补跑）。
/// session_id 列关联触发时创建的会话，是 Created 事件的匹配键。
pub const SCHEDULED_JOBS_SCHEMA: &[&str] = &[
    r#"
CREATE TABLE IF NOT EXISTS task_scheduled (
    id          TEXT PRIMARY KEY,
    name        TEXT,
    config_id   TEXT NOT NULL,
    trigger_at  TEXT NOT NULL,
    prompts     TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'pending',
    session_id  TEXT,
    created_at  TEXT NOT NULL,
    executed_at TEXT,
    error       TEXT
)"#,
    "CREATE INDEX IF NOT EXISTS idx_task_scheduled_status ON task_scheduled(status, trigger_at)",
];

/// 初始化任务域全部表（幂等：逐条 CREATE TABLE IF NOT EXISTS）
///
/// **必须在 `crate::schema::migrate_via_host()` 之后调用**：旧库若先被这里建出统一名
/// 的空表，重命名迁移会因「新旧名同时存在」跳过该条，旧表里的真实数据留在无人读的
/// 名字下（现象是升级后列表变空而非报错）。
pub fn ensure_schema_via_host(host: &WasmHost) -> Result<(), String> {
    for (name, schema) in [
        ("task_history", TASK_HISTORY_SCHEMA),
        ("task_session_mapping", SESSION_MAPPING_SCHEMA),
        ("task_session_settings", SESSION_SETTINGS_SCHEMA),
        ("task_queue", queue::TASK_QUEUE_SCHEMA),
        ("task_preset", preset::PRESET_TASKS_SCHEMA),
        ("task_scheduled", SCHEDULED_JOBS_SCHEMA),
    ] {
        for stmt in schema {
            host.plugin_db_execute(stmt)
                .map_err(|e| format!("task schema init failed on {}: {}", name, e))?;
        }
    }
    Ok(())
}

// ==================== 定时器分域回调（D7 错误隔离） ====================

/// 定时器回调命令名：宿主到点调用本插件该 command（ADR 0003）
pub const SCHEDULER_TICK_COMMAND: &str = "session.task.scheduler-tick";

/// 定时器间隔（秒）——与旧插件并行期保持 1s：延迟 clear 的精度是可感知行为，
/// 放宽间隔会让「上一个任务输出被立刻清屏」
pub const SCHEDULER_INTERVAL_SECS: u64 = 1;

/// tick 分发表里的队列域步骤名（按名计数失败）
pub const DOMAIN_QUEUE_DELAY_CLEAR: &str = "queue-delay-clear";

/// tick 分发表里的静默超时域步骤名
pub const DOMAIN_QUEUE_SILENCE: &str = "queue-silence-check";

/// tick 分发表里的定时任务域步骤名（票 16）
///
/// 旧 auto-task 把「定时任务四步」和「静默看门狗」串在同一个 `handle_scheduler_tick`
/// 里（看门狗是它的第 4 步），合并后拆成两个独立域：定时域内某一步失败不再连带把
/// 看门狗一起跳过，反之亦然（D7 单域降级）。一轮 tick 内各步仍恰好执行一次。
pub const DOMAIN_SCHEDULED_TRIGGER: &str = "scheduled-trigger";

/// 单域失败明细（`(域名, 原因)`）
#[derive(Debug, Clone, Serialize)]
pub struct TickFailure {
    pub domain: String,
    pub reason: String,
}

/// tick 执行报告（外部可见：宿主定时器回调的返回值）
#[derive(Debug, Clone, Default, Serialize)]
pub struct TickReport {
    /// 本轮成功执行的域
    pub executed: Vec<String>,
    /// 本轮失败但**未中断其余域**的域（D7：失败只降级本域）
    pub failed: Vec<TickFailure>,
}

/// 按域依次执行 tick 步骤，单域失败只登记该域而不中断其余域
///
/// **错误隔离契约（票 15 硬性验收第 6 项）**：三域同实例（配对 / 会话 / 任务），
/// 任一步失败必须只降级本域；这里是该契约的 executable 形态——只要还想让后续的
/// 定时任务域（票 16）在被前一个域拖累时仍能跑起来，就必须顺序独立执行。
pub fn run_tick_domains(
    steps: Vec<(&'static str, Box<dyn FnOnce() -> Result<(), String>>)>,
) -> TickReport {
    let mut report = TickReport::default();
    for (domain, step) in steps {
        match step() {
            Ok(()) => report.executed.push(domain.to_string()),
            Err(reason) => report.failed.push(TickFailure {
                domain: domain.to_string(),
                reason,
            }),
        }
    }
    report
}

/// 任务域 tick：宿主定时器到点调用
///
/// 三个域依次独立执行（票 15 两域 + 票 16 定时任务域）：
/// - [`DOMAIN_QUEUE_DELAY_CLEAR`]：waiting 态项登记的延迟 clear 到点下发
/// - [`DOMAIN_QUEUE_SILENCE`]：executing 态静默超时 → 复用会话结束兜底收敛
/// - [`DOMAIN_SCHEDULED_TRIGGER`]：定时任务宽限兑底 / 错过判定 / 到期触发 /
///   定时会话首轮下发兜底（见 [`scheduled::handle_scheduler_tick`]）
///
/// 单域失败只登记该域，不短路其余域（D7）。
pub fn tick_via_host(host: &WasmHost, now_utc: &str) -> serde_json::Value {
    // 闭包箱必须是 'static：`WasmHost` 是 Copy 的 unit struct，`now_utc` 转所有权字符串
    let host = *host;
    let now = now_utc.to_string();
    let steps: Vec<(&'static str, Box<dyn FnOnce() -> Result<(), String>>)> = vec![
        (
            DOMAIN_QUEUE_DELAY_CLEAR,
            Box::new({
                let now = now.clone();
                move || queue::send_due_clears(&host, &now)
            }),
        ),
        (
            DOMAIN_QUEUE_SILENCE,
            Box::new({
                let now = now.clone();
                move || queue::check_executing_silence(&host, &now)
            }),
        ),
        (
            DOMAIN_SCHEDULED_TRIGGER,
            Box::new(move || scheduled::handle_scheduler_tick(&host, &now)),
        ),
    ];
    let report = run_tick_domains(steps);
    for failure in &report.failed {
        host.log_warn(&format!(
            "task tick: domain '{}' failed, degraded but others unaffected: {}",
            failure.domain, failure.reason
        ));
    }
    serde_json::to_value(&report).unwrap_or_else(|e| {
        serde_json::json!({ "executed": [], "failed": [{ "domain": "report-serialize", "reason": e.to_string() }] })
    })
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// tick 分发表的错误隔离契约：单域失败**只登记该域**，不得中断其余域
    ///
    /// 三域同实例（D7）：若第一个域的失败会短路后续域，定时任务域（票 16 并入同一
    /// tick）就会在队列域出问题的时间窗内彻底停摆——本用例把「只降级本域」从事后
    /// 解释变成可执行断言。
    #[test]
    fn tick_domains_isolate_failures_and_keep_running_others() {
        // 闭包箱要求 'static：用共享单元格记录执行顺序（不借用局部变量）
        let ran = std::rc::Rc::new(std::cell::RefCell::new(Vec::<&'static str>::new()));
        let first_trace = ran.clone();
        let second_trace = ran.clone();
        let report = run_tick_domains(vec![
            (
                DOMAIN_QUEUE_DELAY_CLEAR,
                Box::new(move || {
                    first_trace.borrow_mut().push(DOMAIN_QUEUE_DELAY_CLEAR);
                    Err("boom".to_string())
                }),
            ),
            (
                DOMAIN_QUEUE_SILENCE,
                Box::new(move || {
                    second_trace.borrow_mut().push(DOMAIN_QUEUE_SILENCE);
                    Ok(())
                }),
            ),
        ]);

        assert_eq!(
            *ran.borrow(),
            vec![DOMAIN_QUEUE_DELAY_CLEAR, DOMAIN_QUEUE_SILENCE],
            "失败域之后的域必须继续执行（不得短路）"
        );
        assert_eq!(report.executed, vec![DOMAIN_QUEUE_SILENCE.to_string()]);
        assert_eq!(report.failed.len(), 1, "只登记真正失败的域, got: {:?}", report.failed);
        assert_eq!(report.failed[0].domain, DOMAIN_QUEUE_DELAY_CLEAR);
        assert_eq!(report.failed[0].reason, "boom");
    }

    /// 让出护栏：计数递增，且只在到达 `TASK_YIELD_INTERVAL` 时触发一次宿主调用
    ///
    /// 断言的是「长循环有 await 点」这一 D7 约束的形状（按频率让出，而非每次
    /// 迭代都打宿主——那会把循环变成 O(n) 次往返）。宿主时钟读取本身无副作用，
    /// 故此处验证计数语义而非返回值。
    #[test]
    fn yield_guard_counts_and_fires_on_interval() {
        let mut counter = 0u64;
        for _ in 0..(TASK_YIELD_INTERVAL - 1) {
            yield_guard(&WasmHost, &mut counter);
        }
        assert_eq!(counter, TASK_YIELD_INTERVAL - 1, "每次迭代只累加计数");
        assert_ne!(counter % TASK_YIELD_INTERVAL, 0, "未到阈值不得触发让出");
    }

    /// 跨域持锁禁令（D7）：任务域源码不得引用配对 / 信任 / 同意域的进程级状态
    ///
    /// 那三个域各自持有 `static Mutex`（配对码 / QR 管理器等），任务域一旦触碰就
    /// 意味着任务 tick 与配对调用可能互相阻塞——这是三域合并后**唯一可能**把故障
    /// 半径从一个域扩散到整个产品面的路径。源码扫描是这道边界可单测的形态
    /// （不扫描本文件：护栏自身的字面量会自匹配）。
    #[test]
    fn task_modules_reference_no_other_domain_state() {
        for (file, src) in task_domain_sources() {
            for forbidden in [
                "crate::pairing",
                "crate::trust",
                "crate::consent",
                "CURRENT_CODE",
                "QR_MANAGER",
            ] {
                assert!(
                    !src.contains(forbidden),
                    "任务域不得跨域引用 {forbidden}（{file}）：D7 禁止跨域持锁"
                );
            }
        }
    }

    /// 任务域全部模块源码（`(文件名, 内容)`）——源码扫描类用例共用
    ///
    /// 不含本文件：护栏自身的字面量（旧表名等）会自匹配。
    fn task_domain_sources() -> Vec<(&'static str, &'static str)> {
        vec![
            ("agent.rs", include_str!("agent.rs")),
            ("hooks.rs", include_str!("hooks.rs")),
            ("state.rs", include_str!("state.rs")),
            ("queue.rs", include_str!("queue.rs")),
            ("preset.rs", include_str!("preset.rs")),
            ("scheduled.rs", include_str!("scheduled.rs")),
        ]
    }

    /// 票 16（D5）：私有库表名统一后，任务域源码里不得再出现旧表名的 **SQL 用法**
    ///
    /// 只扫 SQL 关键字紧邻的位置：`upsert_session_mapping` 之类的函数名与
    /// `session-settings` 这类线协议路径都合法保留（后者一个字符都不能改）。
    /// 漏改一处 SQL 的后果是运行时「no such table」，而编译与大部分单测发现不了
    /// ——native 用例不连真库，只有这条静态护栏能挡。
    #[test]
    fn task_sources_only_use_prefixed_table_names() {
        let keywords = ["FROM ", "INTO ", "UPDATE ", "JOIN ", "EXISTS ", "TABLE ", "ON "];
        for (file, src) in task_domain_sources() {
            for legacy in crate::schema::TABLE_RENAMES {
                for kw in keywords {
                    let needle = format!("{}{}", kw, legacy.legacy);
                    assert!(
                        !src.contains(&needle),
                        "{file} 仍按旧名 {legacy:?} 写 SQL（找到 `{needle}`）：表名已统一到域前缀"
                    );
                }
            }
        }
    }

    /// 统一后的新表名必须在任务域建表语句里出现（防「只改读不改写」的半迁移）
    #[test]
    fn task_schema_creates_prefixed_tables() {
        let schema = format!(
            "{:?}",
            (
                TASK_HISTORY_SCHEMA,
                SESSION_MAPPING_SCHEMA,
                SESSION_SETTINGS_SCHEMA,
                queue::TASK_QUEUE_SCHEMA,
                preset::PRESET_TASKS_SCHEMA,
                SCHEDULED_JOBS_SCHEMA,
            )
        );
        for name in crate::schema::TABLE_RENAMES {
            assert!(
                schema.contains(name.prefixed),
                "建表语句缺统一表名 {}（新库将建不出该表）",
                name.prefixed
            );
        }
    }

    /// 票 16：HTTP 端点清单与 plugin.json 的 `contributes.httpEndpoints` 逐字一致
    ///
    /// 宿主对已声明插件走精确匹配：清单少一项 → 该端点被宿主 404（移动端与 hook
    /// 脚本静默失效）；多一项 → 未实现的路径被放行到插件里才 404（审计歧义）。
    /// 两向都比对，顺序不敏感（宿主按集合匹配）。
    #[test]
    fn http_endpoints_manifest_matches_dispatch_list() {
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../../plugin.json")).expect("plugin.json 合法");
        let declared = manifest["contributes"]["httpEndpoints"]
            .as_array()
            .unwrap_or_else(|| panic!("manifest 未声明 contributes.httpEndpoints: {}", &manifest["contributes"]))
            .iter()
            .map(|v| v.as_str().expect("端点条目必须是字符串"))
            .collect::<Vec<_>>();

        let mut expected = HTTP_ENDPOINTS.to_vec();
        let mut actual = declared.clone();
        expected.sort();
        actual.sort();
        assert_eq!(
            actual, expected,
            "contributes.httpEndpoints 必须与 task::HTTP_ENDPOINTS 逐项一致"
        );
        assert_eq!(HTTP_ENDPOINTS.len(), 17);
    }

    /// 票 16（对照）：path 段与旧 auto-task 的分派集合完全相同
    ///
    /// golden 清单逐字取自 `plugins/auto-task/rust/src/lib.rs` 的 `_http_endpoint`
    /// 分派（`task-queue/` + `scheduled-jobs/` + state 三路）。搬迁的硬约束是
    /// 「基址随插件 id 改、path 段一个不改」（D1），这张清单就是那条约束的断言形态。
    #[test]
    fn http_paths_are_the_same_set_the_old_plugin_served() {
        const GOLDEN: &[&str] = &[
            "task-status",
            "session-mode",
            "session-settings",
            "task-history/current",
            "task-history/list",
            "supported-agents",
            "task-queue/add",
            "task-queue/remove",
            "task-queue/list",
            "task-queue/clear",
            "task-queue/update",
            "task-queue/reorder",
            "task-queue/cancel",
            "scheduled-jobs/create",
            "scheduled-jobs/list",
            "scheduled-jobs/remove",
            "scheduled-jobs/reset",
        ];
        let mut ours = HTTP_ENDPOINTS.to_vec();
        let mut golden = GOLDEN.to_vec();
        ours.sort();
        golden.sort();
        assert_eq!(ours, golden, "HTTP path 段不得随搬迁变化");
    }

    /// 票 16：三域路径分派（与旧实现同序：先队列、再定时、其余归状态域）
    #[test]
    fn http_path_classification_matches_old_plugin_order() {
        assert_eq!(classify_http_path("task-queue/add"), HttpRoute::Queue("add"));
        assert_eq!(
            classify_http_path("scheduled-jobs/create"),
            HttpRoute::Scheduled("create")
        );
        assert_eq!(
            classify_http_path("task-history/current"),
            HttpRoute::State("task-history/current")
        );
        // 未知路径落状态域并由其自答 404（宿主未声明时才走到这一步）
        assert_eq!(classify_http_path("ghost"), HttpRoute::State("ghost"));
        // 前缀本身（尾部斜杠）归对应域，由其 match 臂落 404——不回落状态域
        assert_eq!(classify_http_path("task-queue/"), HttpRoute::Queue(""));
        // 不带斜杠的同名裸路径不归队列域（旧实现即如此：strip_prefix 要求斜杠）
        assert_eq!(classify_http_path("task-queue"), HttpRoute::State("task-queue"));
        // 清单里每一项都必须能分派到与之匹配的域
        for path in HTTP_ENDPOINTS {
            let route = classify_http_path(path);
            if path.starts_with("task-queue/") {
                assert!(matches!(route, HttpRoute::Queue(_)), "{path} 应归队列域");
            } else if path.starts_with("scheduled-jobs/") {
                assert!(matches!(route, HttpRoute::Scheduled(_)), "{path} 应归定时域");
            } else {
                assert!(matches!(route, HttpRoute::State(_)), "{path} 应归状态域");
            }
        }
    }

    /// tick 分发表覆盖三个域（票 15 两域 + 票 16 定时域），且定时域不重复注册
    #[test]
    fn tick_domains_cover_queue_and_scheduled_without_duplicates() {
        let ran = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let steps: Vec<(&'static str, Box<dyn FnOnce() -> Result<(), String>>)> = [
            DOMAIN_QUEUE_DELAY_CLEAR,
            DOMAIN_QUEUE_SILENCE,
            DOMAIN_SCHEDULED_TRIGGER,
        ]
        .into_iter()
        .map(|name| {
            let trace = ran.clone();
            (
                name,
                Box::new(move || {
                    trace.borrow_mut().push(name.to_string());
                    Ok(())
                }) as Box<dyn FnOnce() -> Result<(), String>>,
            )
        })
        .collect();
        let report = run_tick_domains(steps);

        assert_eq!(
            *ran.borrow(),
            vec![
                DOMAIN_QUEUE_DELAY_CLEAR.to_string(),
                DOMAIN_QUEUE_SILENCE.to_string(),
                DOMAIN_SCHEDULED_TRIGGER.to_string(),
            ],
            "一轮 tick 三域各执行一次"
        );
        assert_eq!(report.executed.len(), 3);
        assert!(report.failed.is_empty());
    }

    /// 票 17（原票 16 的「双轨对照」用例退役）：四类广播的触发点数钉死
    ///
    /// 票 16 那条 `broadcast_payloads_match_the_pre_migration_implementation` 靠
    /// `include_str!` 读 auto-task 四个模块源码做逐构造比对，旧插件目录一删它就没有
    /// 对照基准。删它不丢保护，因为**载荷形状已有更强的结构性闸门**：宿主
    /// `host_impl/events.rs::broadcast_sync` 把载荷反序列化成 SDK 的
    /// `bedcode_plugin_api::events::SyncEvent`（与插件同一类型，serde 表示即线协议），
    /// 再经**穷尽 `From`** 转成 `DesktopSyncEvent`——改字段名、漏字段、写成 camelCase
    /// 要么在此撞成显性 broadcast error，要么编译期就过不去。
    ///
    /// 闸门盖不住的是「触发时机」：某条推进路径被删掉时载荷类型仍然自洽。故此处按
    /// 点数钉死（无头 harness 里 AppContext 未 init，broadcast 只能显性报错，
    /// S1 闭环观测不到同步通道出口，只能静态计点）。点数写死=回归护栏，
    /// 少一个点即红，多一个点也必须来交代为什么。
    #[test]
    fn broadcast_trigger_points_are_pinned() {
        let sources = [
            ("state.rs", include_str!("state.rs")),
            ("queue.rs", include_str!("queue.rs")),
            ("scheduled.rs", include_str!("scheduled.rs")),
            ("preset.rs", include_str!("preset.rs")),
        ];
        let count = |marker: &str| -> usize {
            sources
                .iter()
                .map(|(file, src)| {
                    // 只数真实发布点：跳过文档注释里的同名提及
                    src.lines()
                        .filter(|l| l.contains(marker) && !l.trim_start().starts_with("///"))
                        .count()
                })
                .sum()
        };
        let expected: [(&str, usize); 6] = [
            // 任务状态推进四点：调度建行 / 输入建行 / hook 推送 / 会话结束兜底
            ("SyncEvent::TaskStatusChanged", 4),
            // 模式变更两点：状态域写入 + 定时任务入队时开启自动执行
            ("SyncEvent::SessionModeChanged", 2),
            ("SyncEvent::TaskQueueChanged", 1),
            ("SyncEvent::TaskScheduledChanged", 1),
            // 前端两通道成对发布（每个广播点既发消息总线、又发插件事件），
            // 故两者数量必须相等且等于四类广播的发布点总数
            ("bus_publish(", 9),
            ("emit_event(", 9),
        ];
        for (marker, want) in expected {
            let got = count(marker);
            assert_eq!(got, want, "`{marker}` 发布点数应为 {want}，实际 {got}");
        }
        assert_eq!(
            count("bus_publish("),
            count("emit_event("),
            "两通道发布点必须成对（漏一侧 = 总线或前端事件单边失声）"
        );
    }

    /// 票 16/17：随包 hook 脚本的接线与模板版本对账
    ///
    /// 脚本内容里的 `PLUGIN_ID` 决定了部署到用户项目后的回调地址；这里漏改一位，
    /// agent 的状态推送就会打到旧前缀上（旧插件退役后是静默 404，任务链整条断掉）。
    /// 文件名与本模块从 SDK 取到的脚本名常量对账：改了常量而没改随包文件（或反之）
    /// 都会在这里撞红。
    ///
    /// 票 16 刻意把模板版本 bump 压到 auto-task 后端退役之后（双轨期两侧各比对自己
    /// 那份常量，先 bump 会来回覆盖）；票 17 退役后一次性 bump 四个，把已写入用户
    /// 项目的集成副本改指新前缀。脚本首行 `@bedcode-template-version` 与 `hooks.rs`
    /// 对应常量**必须同值**——只改一侧的现象是「部署副本被判定为最新而永不重写」，
    /// 或反过来每轮都重写，都静默且难查，故在此逐脚本钉死。
    #[test]
    fn shipped_hook_scripts_point_at_this_plugin() {
        use bedcode_plugin_api::constants::{
            CODEX_HOOK_SCRIPT_NAME, HOOK_SCRIPT_NAME, OPENCODE_HOOK_SCRIPT_NAME,
            PI_HOOK_SCRIPT_NAME,
        };
        use super::hooks::{
            CLAUDE_HOOK_TEMPLATE_VERSION, CODEX_HOOK_TEMPLATE_VERSION,
            OPENCODE_PLUGIN_TEMPLATE_VERSION, PI_EXTENSION_TEMPLATE_VERSION,
        };
        let shipped: [(&str, &str, &str, &str); 4] = [
            (
                HOOK_SCRIPT_NAME,
                "auto_task_hook.py",
                include_str!("../../../scripts/auto_task_hook.py"),
                CLAUDE_HOOK_TEMPLATE_VERSION,
            ),
            (
                CODEX_HOOK_SCRIPT_NAME,
                "codex_task_hook.py",
                include_str!("../../../scripts/codex_task_hook.py"),
                CODEX_HOOK_TEMPLATE_VERSION,
            ),
            (
                PI_HOOK_SCRIPT_NAME,
                "pi_task_hook.ts",
                include_str!("../../../scripts/pi_task_hook.ts"),
                PI_EXTENSION_TEMPLATE_VERSION,
            ),
            (
                OPENCODE_HOOK_SCRIPT_NAME,
                "opencode_task_hook.ts",
                include_str!("../../../scripts/opencode_task_hook.ts"),
                OPENCODE_PLUGIN_TEMPLATE_VERSION,
            ),
        ];
        for (constant, file, content, template_version) in shipped {
            assert_eq!(constant, file, "SDK 脚本名常量与随包文件名漂移: {constant} ≠ {file}");
            assert!(
                content.contains("com.bedcode.session"),
                "{file} 未指向合并插件 id"
            );
            assert!(
                !content.contains("com.bedcode.auto-task"),
                "{file} 仍含旧插件 id（部署后会打到旧前缀）"
            );
            // 路径段必须仍是本票声明的那批（基址改、段不改）
            assert!(
                content.contains("/api/plugin/") && content.contains("task-status"),
                "{file} 的端点拼接形状漂移"
            );
            let marker = format!("@bedcode-template-version {template_version}");
            assert!(
                content.contains(&marker),
                "{file} 的模板版本标记与 hooks.rs 常量不同值（应为 {marker}）"
            );
        }
    }
}

// ==================== HTTP 端点面（票 16） ====================

/// 本域对外 HTTP 端点清单（**manifest `contributes.httpEndpoints` 的单一事实源**）
///
/// 条目是去掉 `/api/plugin/<插件 id>/` 前缀后的相对路径段，与宿主传给
/// `_http_endpoint` 的 `path` 字段逐字一致。基址随插件 id 改（D1），**path 段一个
/// 都不改**——移动端与已部署在项目里的 hook 脚本按原路径片段拼接请求。
///
/// 宿主侧对已声明插件做完整路径精确匹配（未声明路径 404，请求不到达插件），
/// 因此本清单即「审计视图 = 实际可达面」；契约用例锁死它与 plugin.json 的一致性。
pub const HTTP_ENDPOINTS: &[&str] = &[
    // 状态与模式（[`state::handle_http_endpoint`]）
    "task-status",
    "session-mode",
    "session-settings",
    "task-history/current",
    "task-history/list",
    "supported-agents",
    // 队列（[`queue::handle_queue_http`]，前缀 `task-queue/`）
    "task-queue/add",
    "task-queue/remove",
    "task-queue/list",
    "task-queue/clear",
    "task-queue/update",
    "task-queue/reorder",
    "task-queue/cancel",
    // 定时任务（[`scheduled::handle_scheduled_http`]，前缀 `scheduled-jobs/`）
    "scheduled-jobs/create",
    "scheduled-jobs/list",
    "scheduled-jobs/remove",
    "scheduled-jobs/reset",
];

/// HTTP 路径归属域（纯判定，不触碰宿主 → native 可测）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpRoute<'a> {
    /// `task-queue/` 前缀，携带去掉前缀的子路径
    Queue(&'a str),
    /// `scheduled-jobs/` 前缀
    Scheduled(&'a str),
    /// 其余全部由状态域应答（含未知路径 → 该域自己返回 404）
    State(&'a str),
}

/// 路径分派判定（与旧 auto-task 的 `strip_prefix` 顺序逐字一致）
pub fn classify_http_path(path: &str) -> HttpRoute<'_> {
    if let Some(sub) = path.strip_prefix("task-queue/") {
        HttpRoute::Queue(sub)
    } else if let Some(sub) = path.strip_prefix("scheduled-jobs/") {
        HttpRoute::Scheduled(sub)
    } else {
        HttpRoute::State(path)
    }
}

/// HTTP 端点入口（宿主 `_http_endpoint` command 调用）
///
/// 返回体形状固定为 `{status, body, contentType?}`——由三域各自的 handler 产出
/// （`http_response::{ok, ok_with_data, error}`），宿主 `plugin_controller` 提取。
pub fn handle_http_via_host(
    host: &WasmHost,
    method: &str,
    path: &str,
    body: &serde_json::Value,
    query: &serde_json::Value,
) -> serde_json::Value {
    match classify_http_path(path) {
        HttpRoute::Queue(sub) => queue::handle_queue_http(host, method, sub, body, query),
        HttpRoute::Scheduled(sub) => {
            scheduled::handle_scheduled_http(host, method, sub, body, query)
        }
        HttpRoute::State(sub) => state::handle_http_endpoint(host, method, sub, body, query),
    }
}

/// 会话配置 + 任务域「是否支持自动化」标记（agent 能力 join）
///
/// 配置真源在本插件私有库（票 08），command 经 agent registry 判定是否为本域
/// 支持的 agent——消费方据此决定能否对该配置排队列任务。
pub fn list_configs_with_support_via_host() -> Vec<serde_json::Value> {
    let host = WasmHost;
    let configs = crate::config::list_via_host()
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    let mut counter = 0u64;
    configs
        .into_iter()
        .map(|mut c| {
            if let Some(cmd) = c.get("command").and_then(|v| v.as_str()) {
                let agent = agent::detect_agent(cmd);
                c["is_supported"] = serde_json::Value::Bool(agent::is_supported(agent));
            }
            yield_guard(&host, &mut counter);
            c
        })
        .collect()
}
