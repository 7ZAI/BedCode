# PROTOTYPE — 移动端 UI 重设计（一次性）

> 回答的问题：移动端各页面风格/设计语言不统一，整套 UI 应该长什么样？
> 约束：功能不变，插件扩展位（nav tab / 工具箱视图 / 设置入口）保持可点。

## 运行

```bash
cd bedcode-mobile && npm run dev
# 浏览器打开 http://localhost:1420/prototype/mobile-ui
# ?variant=A | B | C 切换；底部悬浮条或键盘 ←/→ 循环切换
```

## 三个变体

| 变体 | 主张 | 结构特征 |
|------|------|----------|
| A「秩序」 | 设置页/插件页分组语言的全面推广 | 圆角分组卡 + divide-y 行 + 彩色图标 chip，无阴影无横幅 |
| B「空御」 | 状态优先的遥控器 | Bento 磁贴：连接 Hero、终端实况预览、横滑配置、悬浮胶囊导航 |
| C「素黑」 | 终端驾驶舱 | 零卡片、1px 发丝线分组、等宽数字、状态短码（RUN/WAIT/EXIT） |

每个变体都静态模拟全部页面：连接 / 会话 / 工具箱 / 设置 / 插件列表+详情 / 插件 nav tab，
数据来自 `mock.ts`，不接任何 Tauri command。

## 选型结果（2025-08）

**已选定 A「秩序」**：以设置页/插件页的分组行语言为统一设计语言推广到全应用。
实施交接文档：`%TEMP%\handoff-bedcode-mobile-ui-variant-a.md`（或见 `src/prototype/variants/VariantA.vue` 参考实现）。

## 选型后

1. 把选中的设计语言落到真实页面（正式重写，不直接搬原型代码）。
2. 将本目录 + router 中的 `/prototype/mobile-ui` 路由 + `MobileLayout.vue` 里的
   `isPrototypeRoute` 判断整体删除（或移到一次性分支留档）。
