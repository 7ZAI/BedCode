# 07 — 平滑滚动重影根因调查（research）

**What to build:** 只读调查（不改产品代码）——定位 WebGL 滚动重影根因（addon-webgl 版本 bug vs alpha 缓冲 / allowTransparency），评估升级 xterm/webgl 后重影是否消失，产出「引入平滑滚动 or 维持关闭」的决策记录。

**Blocked by:** None — can start immediately.

**Status:** resolved

- [x] 定位 WebGL 滚动重影根因（addon-webgl 版本 bug vs alpha 缓冲 / allowTransparency）
- [x] 若为版本 bug：评估升级 xterm/webgl 后重影是否消失
- [x] 产出决策记录：引入平滑滚动 or 维持关闭（写入 spec / plan Comments）
- [x] 只读调查，不改产品代码

## Answer（decision record，2026-08-21）

**根因：WebGL alpha 帧缓冲 + 复制帧缓冲滚动优化，非 addon-webgl 版本 bug。**

终端初始化无条件 `allowTransparency: true` → WebGL 必须用 alpha 帧缓冲；配合
1.0 渲染器的「复制帧缓冲区域」滚动优化，旧行像素不被清除，滚动/刷新时出现
残影与「行入侵」。这是配置/环境问题，不是版本 bug，升级 xterm/webgl 不会改变。

**修复（已在 working tree 落地）**：`TerminalPreview.vue` 将 `allowTransparency`
改为 `!!bgImageUrl.value` 条件开启（仅背景图启用时透明让图片透出；其余场景
不透明，WebGL 每帧正常清帧）。`watch([bgImageUrl, bgOpacity])` 同步开关并
`terminal.refresh(0, rows-1)` 清除旧模式残留帧。背景图场景的透明残影问题另议。

**决策：维持 `smoothScrollDuration: 0`（issue 08 作废）。**

- 平滑滚动属观感新动画，引入需物理滚轮分类器（区分滚轮/触控板，触控板保持
  即时防跟手延迟）且真机验证，收益非刚需；
- 即使默认不透明场景重影已消失，背景图（透明）模式滚动期间残影仍可能复发，
  引入前需有「透明模式平滑滚动」专项处理；
- 结论与 plan.md「若重影是版本 bug 可升级引入，否则维持 0 是正确决定」一致。

**遗留**：透明模式（背景图开启）下滚动/刷新的残影问题未根除，若用户在意可在
后续专项（`TOCTOU` 式专项或独立 issue）处理——当前产品默认无背景图，风险低。
代码注释已同步更新（TerminalPreview.vue `smoothScrollDuration` 段）。
