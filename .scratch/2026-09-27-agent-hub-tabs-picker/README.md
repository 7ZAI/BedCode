# Agent Hub：日志/统计 tab 对调 + 日志目录改 fs:pick 选择器

- 日期：2026-09-27
- 范围：`bedcode-desktop/wasm-apps/agent-hub/`（Rust + 前端 + i18n + plugin.json）+ SDK dev-shell mock
- 需求（用户原话）：
  1. 日志和统计 tab 调换位置
  2. 日志 tab 中「添加日志目录」改为 fs:pick（系统选择器）形式
  3. 用项目中 `.pi/sessions` 目录测试能否正确添加、加载日志

## 1. tab 对调

`AgentHubView.vue`：tabs 数组与模板 v-if/v-else-if 链同步改——`logs` 在 `stats` 前。
Transition 分支链最后一项保持 `v-else`（StatsTab）。无测试断言 tab 顺序（已 grep 确认）。

## 2. 日志目录 fs:pick 选择器

- **guest**（`usage/sources.rs::pick_source_dir`，路由 `agent-hub.pick-source-dir`）：薄封装
  `host-platform.pick-folder`（权限门 + 选中路径授权校验都在宿主）。用户取消 → `picked:false`；
  授权拒绝 → Err（前端友好 i18n）。manifest 需单独声明 `fs:pick`——**manifest-gen 扫描到
  `platform_pick_folder()` 调用已自动补进 plugin.json permissions**（源码 Rust 调用即自动声明）。
- **前端**（SessionLogsTab）：路径手输 input 换成 `[选择目录]` 按钮 + 只读路径回显；
  选中后自动派生来源名（`utils/sources.ts::suggestSourceName`，与 guest `is_valid_source_name`
  同口径：小写字母开头 / [a-z0-9-] / ≤32，全剥空兜底 `logs`）；名称可改；确认添加仍走
  `agent-hub.add-usage-source`。
- **i18n**：pick / picking / pickFailed 三个新 key + addPath 文案改为「尚未选择日志目录」。
- **dev-shell mock**：`pick-source-dir` 返回演示目录（浏览器无系统对话框），前端链路可完整演示。

## 3. `.pi/sessions` 实机验证（临时探针，跑完已删）

临时 Rust 测试枚举 `repo/.pi/sessions/**/*.jsonl` 并经嗅探适配器（`parse_by_adapter("custom", …)`）
解析——即「添加自定义来源 → 扫描 → 加载」的 guest 侧全链路：

- **656 个 JSONL 文件**（递归 find 覆盖三层嵌套 `session-id/run-0/session.jsonl`）
- **540 个解析出事件**（合计 126,475 条归一事件），**仅 3 行跳过**（损坏行）
- 116 个空事件：均为 `sol-pi/<id>/observation-pack/ledger.jsonl`（观测台账，非对话会话，
  嗅探器无 message 事件 → 空事件会话，属预期格式差异）
- 结论：添加 `.pi/sessions` 后扫描能正确解析加载；观察台账条目会以「空会话」出现（v1 可接受）

## 验证

- 插件 `cargo test`：135 通过；fmt 干净
- 桌面前端全量：**105 文件 / 1285 通过**（+13：sources.test 6 / useUsage pick 3 / A10 选择器流 4）
- `pnpm exec eslint .`：0 error（118 warning 全为既有；我新增 1 个 prefer-const 已修）
- `pnpm run build`（含 wasmHash）：通过，源/产物 manifest 逐字一致
- SDK dev-shell `tsc --noEmit`：通过
- lens_diagnostics：本轮改动文件无 findings（useUsage 既有 console-error 惯例告警为预存模式）

## 未做 / 后续

- 真机点击核验（无 GUI）；dev-shell 浏览器演示未跑
- 扫描闸门仍走 AUTH_KEY（未整体授权时扫描 auth-required）——选择器授权的是目录本身，
  两者正交，既有设计未改
