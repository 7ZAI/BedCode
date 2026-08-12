# 05 连接首页：已连接空态 + 历史卡信息密度

**Status:** ready-for-agent
**Type:** task

## 问题（vision 第一/五批，P0/P1）

`devices-connected-dark.png`：
1. 已连接态"会话配置"分组下方 ~50% 屏幕空白，无空态引导（可加引导文案/插画，或折叠空分组）

`devices-disconnected-dark.png`：
2. 连接历史卡只有名称+IP，缺上次连接时间等 meta（mock 数据含 `lastConnected` 但 UI 未展示）
3. 5 条卡图标全部相同（显示器），无设备类型区分
4. "清除"无二次确认；历史区标题无计数

## 涉及

`src/views/DevicesView.vue`、`src/components/DeviceHistoryCard.vue`（如存在）

## 建议

- 会话配置空态：居中引导（终端图标 + "暂无配置，连接后自动生成"），或空分组折叠
- 历史卡副行加时间（`formatRelativeTime` 复用现有工具）
- 图标按设备名/类型区分（可选，P2）
- i18n key 同步 zh-CN + en

## 验证

harness 截 `devices`（connected=1 与 connected=0）深色 + 浅色。

## Comments

- 2026-08-12：vision 第一/五批评审发现。
- 2026-08-12（已修复）：已连接空态垂直居中占满剩余空间（min-h-[45vh]）；历史卡副行加相对时间（刚刚/x 分钟前/x 小时前/x 天前/日期，i18n `mobile.time.*`）；历史标题加条数徽章；清除历史加二次确认弹窗。设备类型图标区分（P2）未做，保留显示器图标。
