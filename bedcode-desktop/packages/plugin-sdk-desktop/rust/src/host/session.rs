//! 宿主能力：会话查询与生命周期/输入监听

use super::HostError;

/// 一次会话输出环拉取的返回（对应 WIT `ring-fetch-result`，票 04）
///
/// 与 [`crate::host::pty::PtyRingFetch`] 同形：会话输出环与插件私有 PTY 环共享
/// 同一字节偏移语义（`[实际起点, next_offset)` 区间，游标续拉不重复）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRingFetch {
    /// `[实际起点, next_offset)` 区间的原始字节（未解码，可能是非 UTF-8）
    pub data: Vec<u8>,
    /// 下一次拉取应传的游标
    pub next_offset: u64,
    /// 传入游标落后于环驻留起点（有字节被淘汰）→ 需 resync 重建上下文
    pub truncated: bool,
}

/// 会话信息与生命周期
///
/// 查询类方法需要 `session:read` 权限（会话配置列表含 working_dir 等路径信息）。
/// 生命周期事件通过 [`WasmPlugin::on_session_lifecycle`](crate::wasm::WasmPlugin::on_session_lifecycle)
/// 回调接收，不走消息总线。
/// 提交输入行事件通过 [`WasmPlugin::on_input_submitted`](crate::wasm::WasmPlugin::on_input_submitted)
/// 回调接收，注册需要 `terminal:observe` 权限（见 ADR 0001）。
pub trait HostSession {
    /// 列出所有会话（JSON 数组）
    fn session_list(&self) -> Result<Option<serde_json::Value>, HostError>;

    /// 获取单个会话；不存在返回 `Ok(None)`
    fn session_get(&self, session_id: &str) -> Result<Option<serde_json::Value>, HostError>;

    /// 注册会话生命周期监听器
    ///
    /// 调用后宿主为该插件创建监听器并注册到 SessionManager，
    /// 事件（creating / created / stopping / stopped）通过
    /// `on_session_lifecycle` 回调投递
    fn session_lifecycle_register(&self) -> Result<(), HostError>;

    /// 注册提交输入行监听器
    ///
    /// 调用后宿主为该插件创建监听器并注册到 SessionManager，
    /// 用户提交输入（回车触发）时通过 `on_input_submitted` 回调
    /// 异步投递重建后的完整输入行。需要 `terminal:observe` 权限，
    /// 未授权时返回错误
    fn session_input_register(&self) -> Result<(), HostError>;

    /// 按启动规格创建会话（v19，需要 `session:write` 权限）
    ///
    /// `spec` 为插件算好的 launch spec-json（camelCase）：
    /// `{name, command, args?, cwd, cols?, rows?, env?, environment?, configId?, start?}`。
    /// 宿主只做执行：shell 包装 / 发行版转换 / 尺寸缺省 / ID 预生成；映射决策
    /// （命名唯一化 / config→launch / 是否立即启动）在插件侧完成。返回预生成的
    /// session_id（实际创建异步执行，`Created` 生命周期事件携带同一 id）。
    fn session_create_with_spec(&self, spec: &serde_json::Value) -> Result<String, HostError>;

    /// 关闭（终止）会话（v7，需要 `session:write` 权限）
    ///
    /// 停止会话 PTY 并置为 Stopped，会话记录保留（与用户手动关闭一致）；
    /// 宿主分发 `Stopping` / `Stopped` 生命周期事件。用于插件在任务
    /// 执行完毕后清理自己创建的会话（如定时自动任务会话）
    fn session_close(&self, session_id: &str) -> Result<(), HostError>;

    /// 移除会话（v19，需要 `session:write` 权限）
    ///
    /// 输出管理器 / PTY 注册表 / 会话记录 / 正统端归属一并清理，进程随句柄释放
    /// 终止；宿主广播会话删除同步事件。未知 id 幂等成功。
    fn session_remove(&self, session_id: &str) -> Result<(), HostError>;

    /// 改名（v19，需要 `session:write` 权限）→ 返回改名前的名字
    ///
    /// 未知 `session_id` / 空名显性报错。不新增线协议事件（会话列表拉取即可见）。
    fn session_rename(&self, session_id: &str, name: &str) -> Result<String, HostError>;

    /// 带请求端标识的尺寸调整（v19，需要 `session:write` 权限）
    ///
    /// **只登记与执行**（透传 PTY winsize + 把正统端归属置为请求方），不做裁决
    /// ——正统端判定与覆盖确认策略归插件。`requester` 形如
    /// `{"kind":"desktop"}` / `{"kind":"mobile","deviceName":"Pixel"}`。
    /// 返回 `{previousCanonical, canonical}`。
    fn session_resize(
        &self,
        session_id: &str,
        cols: u16,
        rows: u16,
        requester: &serde_json::Value,
    ) -> Result<serde_json::Value, HostError>;

    /// 会话注解槽写入（v19，需要 `session:write` 权限，票 11）
    ///
    /// 向会话的不透明注解槽写一条 `key → value`：内核只搬运透传、绝不解释键名
    /// （spec D5——旧任务字段的 expand 期并行写入面）。宿主校验权限门 + 会话
    /// 存在性（未知会话显性报错）+ 参数形状；写入按调用方插件记录归属。
    /// 读取面是 [`HostSession::session_list`] / [`HostSession::session_get`]
    /// 回执的 `annotations` 字段（同槽透传）。
    fn session_annotate(&self, session_id: &str, key: &str, value: &str) -> Result<(), HostError>;

    // connections_list 已迁 `crate::host::HostConnection`（票 04）：
    // 它是宿主 server 的连接事实，不随本 interface 退役（ADR 0022 v12 裁决 5）。

    /// 会话输出环拉取（票 04，需要 `terminal:output` 权限 + 属主校验）：按游标拉取
    /// 会话输出原始字节（WIT `list<u8>` 直传，不 JSON 化）。
    ///
    /// 数据面语义与 [`crate::host::pty::HostPty::pty_ring_fetch`] 完全一致：
    /// - `Ok(None)`：游标已追平产出端，无新字节
    /// - `Ok(Some)`：自游标起的字节 + `next-offset`（续拉不重复）；`truncated = true`
    ///   表示游标落后于环驻留起点（中间字节已被淘汰），调用方需清屏重锚（resync）
    /// - `Err`：会话不存在 / 非属主 / 权限缺失
    ///
    /// 宿主 GlobalOutputManager 保有环本体；游标由调用方自持——慢消费只损失
    /// 自己的历史，背压绝不回传到产出端。
    fn session_output_ring_fetch(
        &self,
        session_id: &str,
        from_offset: u64,
        max_bytes: u32,
    ) -> Result<Option<SessionRingFetch>, HostError>;
}
