# 03 设置页：分组 + 图标统一 + 危险按钮区分

**Status:** ready-for-agent
**Type:** task

## 问题（vision 第三/四/五批，P0/P1）

`settings-dark.png` / `settings-light.png`：
1. 6 项设置无分组标题（应分"连接/通知/安全/系统"或类似）
2. 6 个图标 5 种饱和色（蓝/黄/绿/紫/绿/灰）——彩虹墙，违反单一主色；浅色下与墨色调性冲突
3. "重置设置"/"清除所有数据"与导航项同款卡片样式，易误触；清除数据无二次确认
4. 各项无副标题描述

`settings-connection-dark.png`：
5. "重连间隔(秒)" label 在卡片外，与同组项布局不一致（需查证后修正为同行右对齐输入）

`settings-appearance-dark.png`：
6. 字体大小滑块下方 3 档标签 + 行末当前值重复显示
7. 终端数量"5"为裸数字输入，无步进控件

## 涉及

`src/views/SettingsView.vue`、`src/views/settings/ConnectionSettingsView.vue`、
`src/views/settings/AppearanceSettingsView.vue`、对应设置行组件

## 建议

- 图标统一：中性底 + 单一色（可沿用现有 chip 语义色但收敛饱和度，或全部墨色/主色）
- 危险操作按钮与导航项视觉区分（描边 + 危险色），加二次确认
- 数字输入加 −/+ 步进或 stepper 组件
- i18n：新增 key 同步 zh-CN + en

## 验证

harness 截 `settings`、`settings-connection`、`settings-appearance` 深浅两色。

## Comments

- 2026-08-12：vision 第三/四/五批评审发现。
- 2026-08-12（已修复）：主页按 连接/通知/安全/系统 分组 + 组标题；6 图标统一主色（`--mobile-accent` + accent-muted 底）；每项加副标题；重置按钮改描边样式与导航卡区分（清除数据已有确认弹窗）；字体大小行末重复值移除（保留滑块下方 3 档标签）；三处数字输入全部加 −/+ 步进（44px 按钮）。重连间隔 label 查证已在卡片内同行，无需修改。
