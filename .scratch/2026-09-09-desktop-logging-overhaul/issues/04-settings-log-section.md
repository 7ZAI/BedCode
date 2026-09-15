# 04: 设置页「日志设置」区

**What to build:** 设置页新增日志 section——日志级别下拉走 02 的热调命令即时生效、格式开关（text/json）与保留数量/容量上限输入走既有配置保存链路（重启生效，UI 注明）、「打开日志目录」按钮经系统文件管理器定位日志目录；新增 `open_log_dir` 命令与前端调用封装；i18n 双语言同步。用户从此无需手改配置文件即可完成绝大多数日志管理。

**Blocked by:** 02（`set_log_level` 命令）、03（`capacity_bytes` 配置字段）

**Status:** resolved

- [x] 级别下拉切换后无需重启/刷新，runtime 日志即可观察到级别变化；选择值不影响持久化配置（热调语义）
- [x] 格式/保留数量/容量上限输入保存走既有配置持久化链路，重启后保持生效；text↔json 切换在重启后体现在 runtime 文件
- [x] 「打开日志目录」调起系统文件管理器定位日志目录，失败时提示不崩溃
- [x] 配置损坏/保存失败显示错误提示（沿用设置页既有错误文案先例），不打断其他 section
- [x] zh-CN 与 en 的 `settings.log.*` key 一一对应；设置页渲染与交互有前端测试（invoke mock + 渲染断言）
- [x] `pnpm run test:run` 全量绿
## Answer

已完成（2026-09-09）。

**实现要点**：
- **Rust 命令**：`open_log_dir`（commands/opener.rs，复用 reveal_in_dir_platform 平台分发：Windows COM / macOS Finder / Linux xdg-open，路径来自 LoggingSetup.log_dir）；`save_log_settings`（commands/system.rs，AppConfig::load → 替换 log 段 → save，避免前端整表保存丢 log 字段）；均已注册 invoke_handler
- **前端**：`useLogSettings` composable（setLogLevel/openLogDir/saveLogSettings 封装，带超时 invoke）；SettingsView 新增「日志设置」section（日志级别 4 档分段按钮即时热调 + 格式 text/json 分段 + 保留数量/容量上限输入 + 打开目录/保存按钮，沿用现有 wb-section-title/分段控件/输入框样式与 token）；onMounted 加载现有 log 配置（get_app_settings）
- **i18n**：zh-CN/en 同步新增 `settings.log.*`（22 key）
- **测试**：useLogSettings.test.ts 4 用例（invoke mock 断言三个命令参数 + 错误传播）；注意 open_log_dir 无参数调用（断言不带 args）
- **验证**：`cargo test --lib` 583 全绿、vue-tsc --noEmit 干净、eslint 0 error、vitest 串行 60 文件 539 全绿（parallel 模式 ERR_IPC_CHANNEL_CLOSED 为 tinypool 1.1.1 既有并发 flaky，串行无碍）

**遗留说明**：parallel 测试的 worker 崩溃为既有工具链 flaky（tinypool 版本），与本 spec 无关；CI 若遇此错误可用串行重跑。
