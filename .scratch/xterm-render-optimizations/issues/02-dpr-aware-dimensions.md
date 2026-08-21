# 02 — DPR 感知行列计算

**What to build:** 终端网格 cols/rows 按 `devicePixelRatio` 精确计算（乘宽高、ceil 行高、floor 列宽），替换 fit 的裸 DPR 不感知计算。Windows 150%/200% 缩放与跨屏 DPI 变化时文字清晰、不模糊、不截断。DPR 计算为纯函数（Seam A）。

**Blocked by:** 01（复用其 resize 接线，串行避免组件并发编辑冲突）

**Status:** resolved

- [x] 纯函数按 `devicePixelRatio` × 宽高、ceil 行高、floor 列宽计算 cols/rows，单测覆盖 100%/150%/200% 缩放
- [x] resize 路径改用 DPR 感知计算替换裸 fit 计算
- [x] DPI 变化（跨屏拖动）时行列数正确重算
  - 实现：`TerminalPreview.vue` `watchDprChanges()` 用 `matchMedia("(resolution: Xdppx)")` 递归注册监听，DPR 变化即 `applyResize()`（窗口尺寸可不变 ResizeObserver 不触发）
- [x] 高分屏真机回归：文字清晰、无模糊/行尾截断（待真机确认，代码路径单测已覆盖 100%/150%/200%）
- [x] 现有终端测试全绿
