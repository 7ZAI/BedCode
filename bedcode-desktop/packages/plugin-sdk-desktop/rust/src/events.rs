//! 共享类型化载荷
//!
//! 宿主 ↔ 插件之间的事件载荷定义。两端引用同一份类型，
//! serde 表示即线协议 —— 新增/修改事件时编译器强制两端同步，
//! 杜绝字符串契约漂移。
//!
//! **websocket 业务下沉票 08 退役**：`SyncEvent`（同步事件枚举）已删除——宿主
//! `host-events.broadcast-sync` / `SyncPayload` 同步广播面整体退役（插件事件改
//! `host-bus.publish` + `host-events.emit`，载荷为插件自有 JSON，不再经宿主
//! 类型化转发）。`ProcessDoneEvent`（host-process 回调）与 `PluginQuestion`
//! （前端问题结构）保留。

use serde::{Deserialize, Serialize};

/// 进程执行完成事件（宿主 host-process → 插件回调）
///
/// 由 [`WasmPlugin::on_process_done`](crate::wasm::WasmPlugin::on_process_done)
/// 接收。三种结束形态：正常退出（exit_code 为 Some）、被信号终止
/// （exit_code 为 None）、超时 kill（timed_out = true）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProcessDoneEvent {
    /// 宿主返回的 run-id（对应 `process_run` 的返回值）
    pub run_id: String,
    /// 退出码（正常退出 = Some(code)；被信号终止 = None）
    pub exit_code: Option<i32>,
    /// 是否因超时被宿主 kill
    pub timed_out: bool,
}

/// 插件推送的问题结构（任务询问，前端展示用）
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PluginQuestion {
    /// 问题文本
    pub question: String,
    /// 问题简短标题
    pub header: String,
    /// 是否多选
    #[serde(default)]
    pub multi_select: bool,
    /// 选项列表
    #[serde(default)]
    pub options: Vec<PluginQuestionOption>,
}

/// 插件推送的问题选项
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PluginQuestionOption {
    /// 选项标签
    pub label: String,
    /// 选项描述
    #[serde(default)]
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== PluginQuestion ====================

    #[test]
    fn test_plugin_question_defaults() {
        // 宿主/移动端可能构造缺省字段的旧载荷，default 保证可解析
        let json = serde_json::json!({ "question": "q", "header": "h" });
        let q: PluginQuestion = serde_json::from_value(json).unwrap();
        assert!(!q.multi_select);
        assert!(q.options.is_empty());
    }

    /// ProcessDoneEvent serde 形状锁：snake_case 字段 + 可选退出码
    #[test]
    fn test_process_done_event_shape() {
        let json = serde_json::json!({
            "run_id": "run-1",
            "exit_code": 0,
            "timed_out": false
        });
        let e: ProcessDoneEvent = serde_json::from_value(json).unwrap();
        assert_eq!(e.run_id, "run-1");
        assert_eq!(e.exit_code, Some(0));
        assert!(!e.timed_out);

        // 被信号终止：exit_code = None
        let json = serde_json::json!({ "run_id": "run-2", "exit_code": null, "timed_out": true });
        let e: ProcessDoneEvent = serde_json::from_value(json).unwrap();
        assert_eq!(e.exit_code, None);
        assert!(e.timed_out);
    }
}