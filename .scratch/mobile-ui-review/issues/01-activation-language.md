# 01 统一全应用"激活态"视觉语言

**Status:** ready-for-agent
**Type:** task

## 问题

全应用三种"激活/选中"信号互相矛盾：
- Toggle 激活态用米黄色底（`settings-notifications-dark.png`、`settings-connection-dark.png`）
- 分段控件（segmented control）选中态用纯白底（`settings-authentication-dark.png`"配对码"）
- 导航/可点击行用 chevron

违反单一主色原则，深色下米黄/纯白与浅色下的墨色激活态也无对应。

## 涉及

- Toggle 组件（`src/components/Toggle.vue` 或 `--mobile-toggle-active-color` token）
- 分段控件（`SettingsSubPage`/认证页"优先认证方式"切换）

## 建议

统一为 `var(--mobile-accent)`（深色米白/浅色墨色）+ `var(--mobile-text-on-accent)` 文字，
关闭态中性灰。全局搜索 `--mobile-toggle-active-color` 使用点。

## 验证

harness 截 `settings-connection`、`settings-notifications`、`settings-authentication`
深色 + 浅色，确认激活态与主按钮同色语言。

## Comments

- 2026-08-12：vision 第四批评审发现（P0）。
- 2026-08-12（已修复）：Toggle 已用 `--mobile-accent`；RepeatableToggle 与认证页「优先认证方式」激活态改为主色实底填充 + `--mobile-text-on-accent` 文字；删除未使用的 `--mobile-toggle-active-color` token。
