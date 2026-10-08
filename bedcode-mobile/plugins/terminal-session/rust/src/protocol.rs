//! 终端订阅协议 — 帧构造与纯函数（wire 形状真源 = 桌面插件 `ws_terminal.rs`）
//!
//! 自宿主 `terminal_link.rs` 等价迁移（票 12）：帧构造 / 输入投递计划 /
//! ack 节流判定 / 错误分类。测试随迁（native 单测，见各 `#[cfg(test)]`）。

use crate::keys::KeyCombo;

// ==================== 协议常量 ====================

/// 流控 ack 回发节流：累计待 ack 字节达阈值即回发（桌面插件 ack 触发 drain）
pub(crate) const ACK_BYTES_THRESHOLD: u64 = 64 * 1024;

/// 会话不存在（启动中/已停止）连续重试上限，超出后停止等待外部恢复
pub(crate) const MAX_SESSION_MISSING_STRIKES: u32 = 3;

/// 订阅帧 `mode` 字段取值（移动端当前只用 live——页面进出关闭/重建连接）
pub(crate) const SUBSCRIBE_MODE_LIVE: &str = "live";

// ==================== 帧构造 ====================

/// 订阅帧 `{"type":"subscribe","sessionId":"<id>","mode":"live"}`
/// （fresh subscribe = 插件回放环窗口）
pub(crate) fn build_subscribe_frame(session_id: &str) -> String {
    serde_json::json!({
        "type": "subscribe",
        "sessionId": session_id,
        "mode": SUBSCRIBE_MODE_LIVE,
    })
    .to_string()
}

/// 流控 ack 帧 `{"type":"ack","offset":N}`（offset = 本地已渲染字节数；
/// 桌面插件 ack 只作 drain 触发、忽略 offset 值）
pub(crate) fn build_ack_frame(offset: u64) -> String {
    serde_json::json!({ "type": "ack", "offset": offset }).to_string()
}

/// 可打印文本输入帧 `{"type":"input","data":"<UTF-8 文本>"}`
pub(crate) fn build_input_text_frame(data: &str) -> String {
    serde_json::json!({ "type": "input", "data": data }).to_string()
}

/// 特殊键 → PTY 原始字节（`KeyCombo::parse` + `to_pty_bytes`）
pub(crate) fn special_key_to_pty_bytes(name: &str) -> Option<Vec<u8>> {
    let combo = KeyCombo::parse(name)?;
    combo.to_pty_bytes()
}

// ==================== 输入投递计划 ====================

/// 出站输入帧（命令层 → 连接层）
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InputFrame {
    /// 可打印文本输入 → `{"type":"input","data":"<UTF-8>"}` text 帧
    Text { data: String },
    /// 控制字符/特殊键输入 → binary 帧原始字节（KeyCombo::to_pty_bytes）
    Binary { bytes: Vec<u8> },
}

/// 输入投递计划：把「文本 + 特殊键」这一对入参展开成**有序**的出站帧序列。
///
/// 语义即对外契约（spec §3.3「两者可并存，帧序即写入序」）：
/// - 有文本 → 追加 `Text`；文本在前
/// - 有特殊键 → 追加 `Binary`；键字节在后（「先打字，再回车」）
/// - 两者皆空 → 空计划（无帧投递）
///
/// **历史缺陷（2026-10-02 修复，测试钉住）**：实现曾写成 `if 有键 … else if
/// 有文本` 的互斥分支，于是输入栏「命令 + Enter」这条唯一生产路径（前端恒传
/// `specialKey: "enter"`）只发得出一个裸回车，命令文本被静默丢弃——真机表现
/// 是「输入没反应」，且前端无任何报错可查。契约与实现不一致的判据：函数自身
/// 的文档注释写着「两者可并存」。
///
/// 特殊键**先校验后投递**：不支持的键名返回 Err 且一帧都不发，避免「文本已
/// 写进 PTY、回车没发」的半截输入（PTY 侧不可回滚）。
pub(crate) fn plan_input_frames(data: &str, special_key: Option<&str>) -> Result<Vec<InputFrame>, String> {
    let key_bytes = match special_key.filter(|k| !k.is_empty()) {
        Some(key) => Some(special_key_to_pty_bytes(key).ok_or_else(|| format!("unsupported special key: {key}"))?),
        None => None,
    };
    let mut frames = Vec::with_capacity(2);
    if !data.is_empty() {
        frames.push(InputFrame::Text { data: data.to_string() });
    }
    if let Some(bytes) = key_bytes {
        frames.push(InputFrame::Binary { bytes });
    }
    Ok(frames)
}

// ==================== ack 节流 ====================

/// ack 回发节流判定（64KB 阈值；250ms 空闲兜底已随迁移退役——空闲定时器在
/// WASM 插件内不可得，前端 onWriteParsed/rAF 的持续 ack 推进覆盖该场景，
/// 极端静默由重连兜底。偏差记录见票 12 §6.3）。
///
/// `pending == 0` 一律不发：pending 在收帧时累计，为 0 即表示自上次回发后
/// 没有新收到的字节，重发不推进桌面插件侧记账。
pub(crate) fn should_send_ack(pending: u64) -> bool {
    pending != 0 && pending >= ACK_BYTES_THRESHOLD
}

// ==================== 服务端错误分类 ====================

/// 服务端 error 帧分类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ServerErrorClass {
    /// 会话不存在（启动竞态 / 已停止）：退避重试，有限次数后停止
    SessionMissing,
    /// 其它错误：留痕 + 状态事件，连接保持
    Other,
}

/// 服务端 error 帧分类（纯函数）：`会话不存在`（启动竞态）→ SessionMissing；
/// 其余（含插件内错误/宿主错误）→ Other。
/// 文案事实源 = 桌面插件 `ws_terminal.rs` 的错误文案（`会话不存在：{id}`）
pub(crate) fn classify_server_error(message: &str) -> ServerErrorClass {
    if message.contains("会话不存在") {
        ServerErrorClass::SessionMissing
    } else {
        ServerErrorClass::Other
    }
}

// ==================== Tests（自 terminal_link/tests 等价迁移） ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(v: &serde_json::Value) -> Vec<String> {
        let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    }

    // ==================== 帧构造（wire 形状锁） ====================

    #[test]
    fn subscribe_frame_shape() {
        let v: serde_json::Value = serde_json::from_str(&build_subscribe_frame("s-1")).unwrap();
        assert_eq!(keys(&v), vec!["mode", "sessionId", "type"]);
        assert_eq!(v["type"], "subscribe");
        assert_eq!(v["sessionId"], "s-1");
        assert_eq!(v["mode"], "live");
    }

    #[test]
    fn ack_frame_shape() {
        let v: serde_json::Value = serde_json::from_str(&build_ack_frame(42)).unwrap();
        assert_eq!(keys(&v), vec!["offset", "type"]);
        assert_eq!(v["type"], "ack");
        assert_eq!(v["offset"], 42);
    }

    #[test]
    fn input_frame_shape() {
        let v: serde_json::Value = serde_json::from_str(&build_input_text_frame("ls -la")).unwrap();
        assert_eq!(keys(&v), vec!["data", "type"]);
        assert_eq!(v["type"], "input");
        assert_eq!(v["data"], "ls -la");
    }

    // ==================== 输入投递计划（文本 + 特殊键 共存契约） ====================

    /// 抽出帧序列的可读形状（断言「投了几帧、什么顺序、什么载荷」）
    fn plan_shape(data: &str, special_key: Option<&str>) -> Result<Vec<String>, String> {
        Ok(plan_input_frames(data, special_key)?
            .into_iter()
            .map(|f| match f {
                InputFrame::Text { data } => format!("text:{data}"),
                InputFrame::Binary { bytes } => {
                    format!("bytes:{}", bytes.iter().map(|b| format!("{b:02x}")).collect::<String>())
                }
            })
            .collect())
    }

    #[test]
    fn plan_input_frames_keeps_text_before_special_key() {
        // 「命令 + Enter」唯一生产路径：文本在前、回车键字节（0x0d）在后
        let shape = plan_shape("cargo build", Some("enter")).expect("plan ok");
        assert_eq!(shape, vec!["text:cargo build".to_string(), "bytes:0d".to_string()]);
    }

    #[test]
    fn plan_input_frames_text_only() {
        assert_eq!(plan_shape("ls", None).unwrap(), vec!["text:ls".to_string()]);
    }

    #[test]
    fn plan_input_frames_special_key_only() {
        assert_eq!(plan_shape("", Some("ctrl+c")).unwrap(), vec!["bytes:03".to_string()]);
    }

    #[test]
    fn plan_input_frames_empty() {
        // 两者皆空：空计划（无帧投递），不支持键名显性报错
        assert_eq!(plan_shape("", None).unwrap(), Vec::<String>::new());
        assert_eq!(plan_shape("", Some("")).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn plan_input_frames_rejects_unsupported_key_without_partial_send() {
        // 半截输入护栏：不支持键名 → Err 且一帧都不发（PTY 侧不可回滚）
        let err = plan_shape("cargo build", Some("no_such_key")).expect_err("unsupported key");
        assert!(err.contains("unsupported special key"), "{err}");
    }

    #[test]
    fn special_key_to_pty_bytes_covers_ui_keys() {
        // KeyCombo::parse + to_pty_bytes 映射（Enter/Tab/Esc/Del/Ctrl+C/Z/L/方向键）
        let cases: Vec<(&str, &[u8])> = vec![
            ("enter", b"\r"),
            ("tab", b"\t"),
            ("ctrl+c", &[0x03]),
            ("ctrl+z", &[0x1a]),
            ("ctrl+l", &[0x0c]),
        ];
        for (name, expected) in cases {
            assert_eq!(
                special_key_to_pty_bytes(name).as_deref(),
                Some(expected),
                "special key {name}"
            );
        }
        assert_eq!(special_key_to_pty_bytes("no_such_key"), None);
        assert_eq!(special_key_to_pty_bytes(""), None);
    }

    // ==================== ack 节流 ====================

    #[test]
    fn ack_threshold_semantics() {
        assert!(!should_send_ack(0), "pending=0 一律不发（重发不推进对端记账）");
        assert!(!should_send_ack(1), "未达阈值不发");
        assert!(!should_send_ack(ACK_BYTES_THRESHOLD - 1));
        assert!(should_send_ack(ACK_BYTES_THRESHOLD), "恰达阈值回发");
        assert!(should_send_ack(ACK_BYTES_THRESHOLD + 1));
    }

    // ==================== 错误分类 ====================

    #[test]
    fn server_error_classification() {
        assert_eq!(classify_server_error("会话不存在：s-1"), ServerErrorClass::SessionMissing);
        assert_eq!(classify_server_error("subscribe: missing sessionId"), ServerErrorClass::Other);
        assert_eq!(classify_server_error(""), ServerErrorClass::Other);
    }
}
