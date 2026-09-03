# 宿主 mobile.css 无效 calc 模式修复

- **Type:** task
- **Status:** resolved
- **发现:** 2025-08-09（file-transfer 插件 UI 优化期间）

## 问题描述

宿主 `bedcode-mobile/src/styles/mobile.css` 存在 **22 处**无效 CSS calc 模式：

```css
/* 无效：calc() 不允许「长度 × 长度」乘法，整个声明被浏览器静默丢弃 */
font-size: clamp(0.75rem, 0.875rem + (100vw - 360px) / 840 * 0.125rem, 1rem);
```

CSS 规范（css-values-4）规定 `*` 运算符两侧至少一侧必须为 `<number>`；
`长度 / 数字 * 长度` 中最终一步是 `长度 × 长度` → 非法 → 整条声明不生效，
字号回退为继承值（默认 16px），**流体字号 scale 全部静默失效**。

同类 bug 也存在于：

- `bedcode-mobile/packages/plugin-sdk-mobile/dev-shell/src/styles/mobile.css`（**已修复** 2025-08-09）
- `bedcode-mobile/plugins/file-transfer/src/*`（**已修复** 2025-08-09）
- 可能波及 `bedcode-desktop/` 与 `.agents/skills/frontend-styles/` 文档示例（未核查）

## 正确写法（等价，@16px root）

| 原写法（无效） | 修正 |
|----------------|------|
| `/ 840 * 0.125rem` | `/ 840 * 2`（0.125rem = 2px） |
| `/ 800 * 0.25rem` | `/ 800 * 4`（0.25rem = 4px） |
| `/ 800 * 0.0625rem` | `/ 800`（0.0625rem = 1px，乘 1 直接省略） |

已验证：`clamp(2.75rem, 2.75rem + (100vw - 400px) / 800 * 4, 3rem)`
在 400px 视口 = 44px、1200px = 48px（Chrome headless 实测）。

## 执行清单

1. `bedcode-mobile/src/styles/mobile.css`：替换全部 22 处无效模式
2. 核查 `bedcode-desktop/` 前端 CSS 是否含同类模式
3. 核查并修正 `.agents/skills/frontend-styles/` 文档（SKILL.md 自身示例即含此无效写法）
4. 验证：vue-tsc + 宿主 dev 页面肉眼复核字号缩放

## Comments

- 2025-08-09：用户在插件优化期间确认此问题存在，先记录待执行；当前优先完成 file-transfer 插件 UI 优化与 mock 数据填充。

## Answer

2025-08-10 执行完毕，全部按映射表替换（rem→px，@16px root）：

| 位置 | 替换数 | 说明 |
|------|--------|------|
| `bedcode-mobile/src/styles/mobile.css` | 23 | 文档计数 22，实测 23 处全部替换 |
| 宿主 `bedcode-mobile/src/components/*` + `views/*`（12 个 .vue） | 31 | 同类 bug 未列文档，一并修复 |
| `bedcode-mobile/packages/plugin-sdk-mobile/src/ui/Select.vue` | 2 | 0.5rem→*8、0.25rem→*4，漏网修复 |
| `bedcode-mobile/plugins/auto-task/src/panel.css` | 1 | 漏网修复 |
| `.agents/skills/frontend-styles/`（SKILL.md / MOBILE.md / TOKENS.md） | 10 | 文档示例同步修正 |

`bedcode-desktop/` 核查无同类模式。

验证：
- `vue-tsc --noEmit` 通过（bedcode-mobile）
- Chrome headless 实测四种换算（*2 / 省略 1px / *4 / *40）计算值与公式逐位一致，clamp 上下限命中
