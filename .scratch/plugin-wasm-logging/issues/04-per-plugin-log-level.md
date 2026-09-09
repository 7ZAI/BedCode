# 04: per-plugin 日志级别

**What to build:** `BEDCODE_PLUGIN_LOG=com.bedcode.auto-task=trace` 形式的按插件局部放开日志级别——只看某个插件的详细日志，不必全局热调（`set_log_level`）刷爆整个 runtime 文件；未列出的插件沿用宿主全局级别，release 下排查单个插件问题成为可能。

**Blocked by:** None（可立即开始）

**Status:** ready-for-agent

- [ ] 插件日志入口（`[plugin:xxx]` 事件产生处）按 `plugin_id → 级别阈值` 映射过滤：低于阈值的级别直接丢弃，不产生日志事件
- [ ] 映射来源解析 `BEDCODE_PLUGIN_LOG`（逗号分隔 `id=level`）；未知插件、非法级别容错（忽略该条目，不 panic、不影响其他条目）
- [ ] 未列出的插件沿用宿主全局过滤语义，行为与现状完全一致
- [ ] `[plugin:xxx]` 前缀与插件日志统一格式不变（既有 grep 习惯不受影响）
- [ ] 测试：用既有捕获订阅者模式断言——低于阈值的级别不出现、高于阈值的正常出现、未列出的插件不受影响
