//! 宿主能力：插件私有伪终端（WIT `host-pty`，ABI v16）
//!
//! 裸 PTY 引擎原语（ADR 0022 裁剪线）：插件创建**只属于自己**的交互式伪终端，
//! 跑真实命令 / TUI 程序并以字节流读写。与宿主业务会话线的分工写死在契约里：
//!
//! | 能力 | 服务对象 |
//! | --- | --- |
//! | [`HostPty`]（本模块） | 插件私有 PTY（不进业务会话注册表、不注册业务输出总线） |
//! | `host-terminal` / `host-session` / `terminal-hooks` | 宿主业务终端会话（配置、生命周期、前端 UI） |
//! | `host-process`（非交互） | 一次性命令执行，无 TTY 行为 |
//!
//! # 权限两域
//!
//! - `pty:spawn`：`pty_spawn` / `pty_kill`（在宿主机执行任意命令的高风险面）
//! - `pty:io`：`pty_write` / `pty_resize` / `pty_ring_fetch` / `pty_is_running`（数据面）
//!
//! 属主隔离：句柄 `pty-<uuid>` 仅创建者插件可用，他人调用返回
//! `not owner of pty handle`；插件停用时宿主回收其全部 PTY。
//!
//! # 输出面：环形缓冲 + 游标拉取（无 push 回调）
//!
//! 宿主为每个 PTY 维护有界环形缓冲，插件按自己的节奏调用 [`HostPty::pty_ring_fetch`]：
//!
//! - 首次传 `from_offset = 0`，此后一律传上次返回的 `next_offset`——**续拉不重复**；
//! - `Ok(None)` = 游标已追平产出端，无新字节（轮询节奏由插件决定，宿主不唤醒）；
//! - `truncated = true` = 你传来的游标落后于环驻留起点，中间有字节被淘汰，返回的是
//!   现存最早段——**必须按 resync 语义重建上下文**（清掉本地已渲染状态再续）；
//! - 环满只淘汰自己的历史，绝不把背压踢回 PTY 读取端（慢插件不影响进程产出）。
//!
//! 环容量是**插件声明面**：spawn 时用 [`PtySpawnConfig::ring_bytes`] 给出，省略取宿主
//! 默认（256 KiB），为 0 或超宿主上限（4 MiB）直接报错——宿主不静默夹取，也不因新句柄
//! 淘汰你已有的句柄（每插件在册条数另有上限）。单次拉取字节数另受宿主上限截断
//! （`max_bytes` 传大值即「取到宿主允许的一批」），余下按 `next_offset` 续拉。
//!
//! # 退出事件与订阅时序（硬约束）
//!
//! 进程退出（任意原因）→ 宿主发布 owner 作用域 topic [`PTY_EXIT`]，payload
//! `{ ptyId, reason, exitCode? }`（camelCase），`reason = "stopped" | "killed" | "error"`。
//!
//! **必须在 `activate` 期完成 `bus_subscribe`**（用 [`pty_event_topic`] 生成，勿手拼）：
//! 宿主不缓冲、不重放，晚订阅期间的事件永久丢失且不报错；丢失后的自愈入口是
//! [`HostPty::pty_is_running`] 快照。事件发出时宿主已摘除该句柄与环，因此
//! **先把输出消费完再等退出事件**（判据：`ring-fetch` 的 `next_offset` 不再前进）——
//! 退出事件之后不再有可读字节。

use super::HostError;

// ==================== 退出事件 topic（owner 作用域，勿手拼） ====================

/// PTY 进程退出（唯一的生命周期事件）
pub const PTY_EXIT: &str = "pty:exit";

/// 生成属主作用域事件 topic：`pty:<event>.<owner>`
///
/// `owner` 必须传本插件 ID（topic 内嵌属主，他人订阅物理上收不到）。
/// `event` 用 [`PTY_EXIT`] 常量，避免手拼拼错导致「订阅了却永远收不到」。
pub fn pty_event_topic(event: &str, plugin_id: &str) -> String {
    format!("{event}.{plugin_id}")
}

// ==================== 拉取结果与 spawn 参数 ====================

/// 一次 `ring-fetch` 的返回（对应 WIT `ring-fetch-result`）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PtyRingFetch {
    /// `[实际起点, next_offset)` 区间的原始字节（未解码，可能非 UTF-8）
    pub data: Vec<u8>,
    /// 下一次拉取应传的游标
    pub next_offset: u64,
    /// 传入游标落后于环驻留起点（有字节被淘汰）→ 需 resync
    pub truncated: bool,
}

/// `pty_spawn` 的 config-json 组装器（camelCase；未设置字段省略）
///
/// 宿主只收**裸引擎参数**：不做 shell 包装（要 `bash -lic` 请自己放进
/// `command`/`args`）、不做 WSL 路径转换、不做默认 shell 探测（ADR 0022）。
#[derive(Debug, Clone)]
pub struct PtySpawnConfig {
    command: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
    working_dir: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
    ring_bytes: Option<u64>,
}

impl PtySpawnConfig {
    /// `command` = 可执行文件（PATH 内名称或绝对路径）
    pub fn new(command: &str) -> Self {
        Self {
            command: command.to_string(),
            args: Vec::new(),
            env: Vec::new(),
            working_dir: None,
            cols: None,
            rows: None,
            ring_bytes: None,
        }
    }

    /// 参数数组（exec 直传，宿主不做 shell 解析，天然免注入）
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.args = args.into_iter().map(|a| a.as_ref().to_string()).collect();
        self
    }

    /// 环境变量（追加进子进程环境；不设置则继承宿主环境）
    pub fn env<K, V>(mut self, entries: K) -> Self
    where
        K: IntoIterator<Item = (V, V)>,
        V: AsRef<str>,
    {
        self.env = entries
            .into_iter()
            .map(|(k, v)| (k.as_ref().to_string(), v.as_ref().to_string()))
            .collect();
        self
    }

    /// 工作目录
    pub fn working_dir(mut self, dir: &str) -> Self {
        self.working_dir = Some(dir.to_string());
        self
    }

    /// 终端列数（缺省取宿主终端配置 `terminal.default_cols`）
    pub fn cols(mut self, cols: u16) -> Self {
        self.cols = Some(cols);
        self
    }

    /// 终端行数（缺省取宿主终端配置 `terminal.default_rows`）
    pub fn rows(mut self, rows: u16) -> Self {
        self.rows = Some(rows);
        self
    }

    /// 本条 PTY 的输出环容量（字节）：省略取宿主默认（256 KiB）
    ///
    /// 超过宿主上限时 `pty_spawn` **直接报错、不夹取**（静默降级会让插件按自己声明的
    /// 深度规划上下文）。环满只淘汰自己的历史并以 `truncated` 上报（见
    /// [`HostPty::pty_ring_fetch`]），绝不把背压踢回进程产出。需要「大段输出后离线
    /// 解析」的深回看场景按需调大；TUI 一帧全屏重绘约数十 KB，默认值够数十帧。
    pub fn ring_bytes(mut self, ring_bytes: u64) -> Self {
        self.ring_bytes = Some(ring_bytes);
        self
    }

    /// 序列化为 `pty_spawn` 的 config-json
    pub fn to_json(&self) -> String {
        let mut root = serde_json::Map::new();
        root.insert("command".to_string(), serde_json::json!(self.command));
        if !self.args.is_empty() {
            root.insert("args".to_string(), serde_json::json!(self.args));
        }
        if !self.env.is_empty() {
            let mut env = serde_json::Map::new();
            for (key, value) in &self.env {
                env.insert(key.clone(), serde_json::json!(value));
            }
            root.insert("env".to_string(), serde_json::Value::Object(env));
        }
        if let Some(working_dir) = &self.working_dir {
            root.insert("workingDir".to_string(), serde_json::json!(working_dir));
        }
        if let Some(cols) = self.cols {
            root.insert("cols".to_string(), serde_json::json!(cols));
        }
        if let Some(rows) = self.rows {
            root.insert("rows".to_string(), serde_json::json!(rows));
        }
        if let Some(ring_bytes) = self.ring_bytes {
            root.insert("ringBytes".to_string(), serde_json::json!(ring_bytes));
        }
        serde_json::Value::Object(root).to_string()
    }
}

// ==================== 能力 trait ====================

/// 插件私有伪终端（v16，host-pty）—— 签名与 WIT `host-pty` 一一对应
pub trait HostPty {
    /// 创建裸 PTY，成功返回句柄 `pty-<uuid>` 并登记属主
    ///
    /// 失败只回错误、**不发布任何事件**（无句柄可寻址）。参数见 [`PtySpawnConfig`]。
    /// 配额类失败一律可见（不排队、不静默降级、不淘汰自己已有的句柄）：
    /// 每插件在册条数达宿主上限、`ringBytes` 为 0 或超宿主上限，均直接返回错误。
    fn pty_spawn(&self, config_json: &str) -> Result<String, HostError>;

    /// 写入输入字节（`pty:io`）
    ///
    /// 宿主内建分块节奏（4000 字节分块 + 逐块让出，避免打满 PTY 内核缓冲）；单次调用
    /// 上限 64 KiB：超限直接返回错误、**一个字节都不写入**（静默截断会把半条命令喂进
    /// 交互进程，比失败更糟）。更大的输入由插件自行分批调用。
    fn pty_write(&self, pty_id: &str, data: &[u8]) -> Result<(), HostError>;

    /// 调整终端尺寸（`pty:io`，TUI 程序的全屏刷新依赖它）
    ///
    /// `Ok(())` 只表示尺寸已提交内核（winsize 变更以 SIGWINCH 通知前台进程组），
    /// **不承诺同步生效时序**——需要确认时让进程自报尺寸（如 `stty size`）再经
    /// [`HostPty::pty_ring_fetch`] 读回。
    fn pty_resize(&self, pty_id: &str, cols: u16, rows: u16) -> Result<(), HostError>;

    /// 终止并销毁（优雅 Ctrl-C → 兜底强杀），随后投递 [`PTY_EXIT`]（reason=killed）
    fn pty_kill(&self, pty_id: &str) -> Result<(), HostError>;

    /// 按游标拉取输出：`Ok(None)` = 已追平无新字节（`pty:io`）
    ///
    /// 轮询节奏由插件掌握（宿主不唤醒插件）：交互场景典型做法是「`pty_write` → 以
    /// 20~50ms 间隔续拉，直到 `next_offset` 不再前进」；输出风暴场景按 `truncated`
    /// 走 resync。单次返回字节数被宿主截断到上限，超出部分留待下次拉取。
    fn pty_ring_fetch(&self, pty_id: &str, from_offset: u64, max_bytes: u32) -> Result<Option<PtyRingFetch>, HostError>;

    /// 进程是否仍在运行（`pty:io`，丢失 [`PTY_EXIT`] 事件后的自愈快照）
    ///
    /// 判据含「读线程已见到 EOF」，**自然退出也如实报 false**；句柄已被摘除（退出
    /// 事件已发出 / 已 kill）时返回错误而非 false——「进程死了」与「句柄不存在」的
    /// 语义差别决定插件是否要重新 spawn。
    fn pty_is_running(&self, pty_id: &str) -> Result<bool, HostError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pty_event_topic_embeds_owner_and_event() {
        // topic 形状必须与宿主侧 format!("pty:exit.{owner}") 逐字节一致
        assert_eq!(pty_event_topic(PTY_EXIT, "com.x"), "pty:exit.com.x");
        // 属主隔离：不同插件的 topic 互不相等（非属主物理上收不到）
        assert_ne!(pty_event_topic(PTY_EXIT, "a"), pty_event_topic(PTY_EXIT, "b"));
    }

    #[test]
    fn spawn_config_json_only_carries_command_when_nothing_else_set() {
        let parsed: serde_json::Value =
            serde_json::from_str(&PtySpawnConfig::new("top").to_json()).expect("valid json");
        assert_eq!(parsed, serde_json::json!({ "command": "top" }));
    }

    #[test]
    fn spawn_config_json_uses_camel_case_and_keeps_engine_params_only() {
        let json = PtySpawnConfig::new("/usr/bin/vim")
            .args(["-u", "NONE", "main.rs"])
            .env([("EDITOR", "vim")])
            .working_dir("/tmp")
            .cols(132)
            .rows(43)
            .to_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        assert_eq!(
            parsed,
            serde_json::json!({
                "command": "/usr/bin/vim",
                "args": ["-u", "NONE", "main.rs"],
                "env": { "EDITOR": "vim" },
                "workingDir": "/tmp",
                "cols": 132,
                "rows": 43,
            })
        );
        // 宿主按 camelCase 反序列化：snake_case 键不得出现
        assert!(parsed.get("working_dir").is_none());
    }

    #[test]
    fn spawn_config_json_serializes_declared_ring_capacity_in_camel_case() {
        // 票 05：环容量属插件声明面（宿主仲裁上限），键名必须 camelCase
        let parsed: serde_json::Value =
            serde_json::from_str(&PtySpawnConfig::new("sh").ring_bytes(4096).to_json()).expect("valid json");
        assert_eq!(parsed["ringBytes"], 4096);
        assert!(parsed.get("ring_bytes").is_none(), "snake_case 键宿主读不到");
        // 未声明时省略字段——交由宿主取默认容量，而不是传 0（0 会被宿主判错拒绝）
        let unset: serde_json::Value =
            serde_json::from_str(&PtySpawnConfig::new("sh").to_json()).expect("valid json");
        assert!(unset.get("ringBytes").is_none(), "省略即交由宿主取默认");
    }

    #[test]
    fn spawn_config_json_passes_args_verbatim_without_shell_wrapping() {
        // 裁剪线证据：助手不注入 bash/sh -c，也不改写参数（业务包装归插件自己）
        let parsed: serde_json::Value = serde_json::from_str(
            &PtySpawnConfig::new("sh")
                .args(["-c", "echo a; echo b"])
                .to_json(),
        )
        .expect("valid json");
        assert_eq!(parsed["command"], "sh");
        assert_eq!(parsed["args"], serde_json::json!(["-c", "echo a; echo b"]));
    }
}
