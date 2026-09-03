# 06 — L2 组合集成测试

**What to build:** 前端跨模块协作在测试内可用：真实 Pinia store + composable + 组件挂载（仅 mock Tauri invoke 边界）的组合测试，覆盖四条用户路径——配对流程（生成码/轮询待配/状态流转）、服务器控制（启停/指标轮询/配置回显）、终端会话（输出流渲染到终端/输入参数构造）、插件管理（清单加载/启停联动）。

**Blocked by:** 05 — 契约 fixtures 工厂（组合测试统一从工厂取数）

**Status:** resolved

- [x] 4 条路径各有独立组合测试，每条至少含 2 个 composable 或 store 的协作断言
- [x] 只 mock `@tauri-apps/api` 边界，Pinia/router/composable 内部逻辑真实执行
- [x] 测试连续运行 3 次无 flake
- [x] `npm run test:run` 全量通过

## Answer

实现于 4d67c652（2026-08-16）。`src/__tests__/integration/` 4 文件 16 测试（415 全绿，连跑无 flake，vue-tsc 0 错误）：配对流（usePairing×useDeviceStore×useSettingsStore×DevicesView）、服务器流（useServer×useSettingsStore×ServerView）、终端流（真实 xterm×useTerminalOutputStream×useSessionStore×useTerminalInputMarkers）、插件流（pluginLoader×usePluginManager×PluginsView）。

**本票实证价值**：挂载真实 DevicesView 暴露 05 票 fixture 契约 bug——`db::Pairing` 实为 `#[serde(rename_all="camelCase")]`，05 写成了 snake_case；已修正 fixture + 5 处消费方 + 文件头注释（见 05 ticket Answer 的评审修正记录）。review 修正：过期用例恒真断言改正向断言先行；弱界改精确值后暴露 mount 时列表加载两次的冗余调用（`DevicesView.onMounted` 直接加载 + `refreshDevices` 内加载，生产小低效，未在本票修）。

**验收口径说明**：插件流协作实体为 pluginLoader 单例 + DOM（usePluginManager 未直接断言实例），按字面验收属边缘达成，review 已评估；渲染层桩（vue-echarts / WebSocket / qr 返回 null）均为非被测逻辑的必要 seam，文件头说明。
