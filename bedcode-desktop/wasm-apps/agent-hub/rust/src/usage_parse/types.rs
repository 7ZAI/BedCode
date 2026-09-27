//! 归一数据结构（适配器与聚合层共用的跨边界形状）
//!
//! [`TokenUsage`]（按 message.id 去重后的一条助手用量）/ [`NormalizedEvent`]
//! （会话日志视图的行）/ [`ModelUsage`]（单模型用量）/ [`ParsedSession`]
//! （会话解析结果 = 聚合记录 + 事件流，一次解析两处消费）。

// ==================== 归一数据结构 ====================

/// 消息级 token 明细（按 message.id 去重后的一条助手用量）
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct TokenUsage {
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub reasoning: i64,
}

impl TokenUsage {
    fn add(&mut self, other: &TokenUsage) {
        self.input += other.input;
        self.output += other.output;
        self.cache_read += other.cache_read;
        self.cache_write += other.cache_write;
        self.reasoning += other.reasoning;
    }
}

/// 归一事件（会话日志视图的行；token 字段仅助手事件携带）
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NormalizedEvent {
    /// 事件时间（epoch ms；不可得为 None）
    pub ts: Option<i64>,
    /// user / assistant / tool / system
    pub role: &'static str,
    /// 展示文本（工具事件为「名称 + 参数/结果摘要」）
    pub text: String,
    /// 助手事件附带的模型
    pub model: Option<String>,
    /// 助手事件附带的 token 明细
    pub tokens: Option<TokenUsage>,
}

/// 单模型用量（主导模型判定与按模型聚合的明细）
#[derive(Default, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ModelUsage {
    pub model: String,
    /// 出现次数（去重后的助手消息数）
    pub messages: u32,
    pub tokens: TokenUsage,
}

/// 会话解析结果：聚合记录 + 事件流（一次解析两处消费）
#[derive(Clone, Debug, Default)]
pub(crate) struct ParsedSession {
    pub cli_session_id: String,
    pub project: Option<String>,
    pub title: Option<String>,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub models: Vec<ModelUsage>,
    pub tokens: TokenUsage,
    /// 有则存（claude totalCostUSD / pi usage.cost.total），null 不估算
    pub cost_total: Option<f64>,
    pub events: Vec<NormalizedEvent>,
    /// 事件流被截断（超 MAX_EVENTS）
    pub events_truncated: bool,
    /// 解析过程中被跳过的损坏行数（截断行 / 非法 JSON）
    pub skipped_lines: u32,
}

impl ParsedSession {
    /// 主导模型：按输出 token 量最大者（无任何助手消息时 None）
    pub fn dominant_model(&self) -> Option<&str> {
        self.models
            .iter()
            .max_by_key(|m| (m.tokens.output, m.tokens.input, m.messages))
            .map(|m| m.model.as_str())
    }

    pub(super) fn record_assistant_usage(&mut self, model: &str, usage: &TokenUsage) {
        self.tokens.add(usage);
        match self.models.iter_mut().find(|m| m.model == model) {
            Some(m) => {
                m.messages += 1;
                m.tokens.add(usage);
            }
            None => self.models.push(ModelUsage {
                model: model.to_string(),
                messages: 1,
                tokens: *usage,
            }),
        }
    }
}
