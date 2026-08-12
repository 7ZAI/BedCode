# 04 预设任务页：激活态 + 双播放按钮 + 触达区

**Status:** ready-for-agent
**Type:** task

## 问题（vision 第二/五批，P0/P1）

`preset-tasks-dark.png` / `preset-tasks-light.png`：
1. "可重复/不可重复"分段控件激活态不清晰（两态视觉权重接近）
2. **双播放按钮**（已查证）：`PresetTaskCard.vue` 左侧 icon-chip 播放图标（点击执行）+ 右侧操作组又一个播放按钮（同一 `handleExecute`）——功能重复，删除其一
3. 行内 3 个操作按钮（执行/编辑/删除）触达区 < 44px，右侧密集
4. "未使用"徽章小字号对比度临界
5. 删除按钮无二次确认

## 涉及

`src/components/PresetTaskCard.vue`、`src/views/PresetTasksView.vue`（分段控件）

## 建议

- 分段控件激活态：主色填充 + contrast 文字
- 删除左侧 icon-chip 播放（保留行内操作组）或反之，与任务行交互统一
- 操作按钮扩触达区（w-11 h-11 级）或收纳为 overflow menu
- 删除二次确认

## 验证

harness 截 `preset-tasks` 深色 + 浅色。

## Comments

- 2026-08-12：vision 第二/五批评审发现；双播放按钮已查证确认。
- 2026-08-12（已修复）：RepeatableToggle 激活态改主色实底填充；删除左侧 icon-chip 播放（改为装饰性任务图标，执行保留右侧操作组）；操作按钮保持 44px 最小触控区；「未使用」徽章改 `--mobile-chip-zinc` 浅灰并去 opacity 0.85（对比度达标）；删除加二次确认弹窗。
