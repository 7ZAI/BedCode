# 02: json 模式 span 链（.with_span_list）

**What to build:** json 事件格式打开 span 列表——`logging.rs` json 分支（runtime 层 + error 层）的 `event_format(tracing_subscriber::fmt::format::json())` 补 `.with_span_list(true)`，error.json 行即携带 request_id/session_id 等 span 上下文，与 text 模式行内 span 链（desktop-logging-overhaul 05 已实现）对称。零成本、不破坏既有 json 字段。

**Blocked by:** None

**Status:** done

- [x] json 分支两处（runtime + error）event_format 加 `.with_span_list(true)`（并配 `JsonFields`——span 字段必须由 json 层自己的字段格式化器记录，默认 DefaultFields 的 ANSI 文本会让 span 序列化 panic）
- [x] 验证：带 span 的事件 json 输出含 `"spans"` 数组且 request_id 字段正确；既有字段（level/time/target/line_number）不变
- [x] text 分支与控制台格式零改动；既有 logging 测试全量通过
- [x] 回归：error.*.log（json 模式）仍只含 ERROR 事件（FilterFn 语义不变）