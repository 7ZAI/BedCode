# 移动端终端：键盘避让 resize → lift + 随包内置中文等宽字体

- 日期：2026-10-01
- 范围：**仅 `bedcode-mobile` 前端**（无 Rust、无跨端协议、无 ABI、无版本号变动）
- 状态：代码完成，单测/typecheck/lint/build 绿；**真机核验未做**（见文末）

## 需求

1. 终端显示区避让键盘**不要用 resize**（改渲染区大小 → 重排行数 → 重发 PTY resize），
   改成与**快捷键面板弹出**一样的**向上移动**；旧的 resize 实现**注释保留**而不是删除。
2. 引入**中文等宽字体**并设置，解决终端**尾部边界 TUI 凹凸**的问题。

## 1. 键盘避让：resize → lift

**确认现状**：原实现确实是 resize 语义——`useTerminalKeyboardAvoidance` 给
`.terminal-view` 根容器 `height: calc(100vh - keyboardOffset)`，`.movable-area` 随
flex 收缩 → `ResizeObserver` 重新 fit → 行数重算 → `useTerminalResize` 同步 PTY。

**改法**：

| | 旧（resize） | 新（lift） |
|---|---|---|
| 载体 | `.terminal-view` 的 `height` | `.movable-area` 的 `transform: translateY(-offset)` |
| 网格 cols/rows | 每帧重算 | **恒定** |
| PTY resize | 每次弹/收各一次 | **0 次** |
| 顶部 | 完整可见 | 被 `.movable-clip`（overflow:hidden）裁掉 `offset` 高度 |
| 底部 | 输入栏贴合键盘上沿 | 同左（根容器底边本来就在屏幕底，平移后落在键盘上沿） |

- 旧实现在 `useTerminalKeyboardAvoidance.ts` **文件末注释保留**，附三步恢复说明。
- 顶部裁切的前提 = **布局视口不被键盘压缩**（`AndroidManifest adjustNothing`）。该前提与
  失效现象（偏移明显大于真实键盘高）写进域文件头；不在代码里加「补偿系数」兜底——
  那会把真 bug 变成不可见的怪现象。
- 合成层收尾：快捷键面板原本就有「还原后强制 refresh + 重合成」定时器，现改为同时监听
  两个位移源（键盘位移期间 vv resize 逐帧触发，定时器只在两者归零时排）。
- 回归锁：单测断言 `terminalViewStyle` **不含** `height`——防止 resize 语义被无意加回。

**未做**：横屏 + 键盘时可见行数明显变少这一代价，如实记录（用户明确要求此方案）；
若真机反馈不可接受，恢复路径已写在注释里。

## 2. 中文等宽字体

**根因**（`terminal.css`「行尾软裁切」注释里记的就是这条）：系统等宽字体给拉丁 advance
（Droid Sans Mono ≈ 0.6em），中文字形落到 1em 的比例 CJK 字体，**1em ≠ 2×0.6em**，
亚像素误差逐字累积 ⇒ 满行末字墨迹溢出（被裁右半）+ TUI 背景盒被推出网格右界 7~22px
（行尾色块随重绘摆动）。**只有 CJK 严格 2 格的等宽字体能根治**；Android/国产 ROM
不预装（Noto Sans Mono CJK 缺失）⇒ 只能随包带。

**选型**：Sarasa Mono SC（更纱黑体，OFL-1.1）。实测其度量（自写 TTF 解析器读 hmtx）：

```
unitsPerEm 1000     'W' 500(1格)   '中' 1000(2格)   '─' 500(1格)   '✓' 1000(2格)
```

**子集**：完整 TTF 14MB 对 APK 不可接受 → `scripts/build-terminal-font.mjs`
（devDep `subset-font`）按终端真实字符集做子集 = GB2312 全集 6763 汉字 + 制表符 +
块元素 + 标点 + 数学/箭头/技术符号 + 假名 + 全半角 = **10635 码位 / 1.05MB woff2**。
子集外汉字落回系统 CJK 字体，advance 恒 1em = 2×0.5em ⇒ **不变量仍成立**。

**就绪时序**（不做就会错）：格宽 fallback 0.6em → 内置 0.5em，一屏内两套度量算列行数
= 行尾错位 + fit 横跳。`ensureTerminalFontLoaded()`（3s 超时 / 失败 `logger.warn` 回退）
在 `initTerminal` 首次测量前、会话页「设备预算起步网格」前各 await 一次。
`@font-face` 全局声明（`styles/terminal-font.css`），woff2 本体仍按需下载 ⇒ 不拖慢启动。

**合规**：许可证全文入库（同目录 `LICENSE-Sarasa-Gothic.txt`，OFL 第 2 条）+ 子集保留
全部 `name` 表条目；主名不含 CJK 部分声明的保留名 `'Source'`（OFL 第 3 条）。

## 验证

| 项 | 结果 |
|---|---|
| 新增单测 `useTerminalKeyboardAvoidance.test.ts` | 15 项全绿（双通道/10px 阈值/lift 样式/防 resize 回接锁/收起回调/dispose 解绑） |
| 变异探针 ×4 | `>`→`>=`、阈值门槛、通道优先级、基准冻结 —— 逐一被对应用例打红 |
| 移动端 `pnpm run test:run` | 53 文件 / 524 用例全绿 |
| `pnpm exec vue-tsc --noEmit` | 通过 |
| 根 `pnpm exec eslint .` | 0 error / 117 warning（与改前同数） |
| `pnpm run build:fast` | 产物含 `SarasaMonoSC-Terminal-Regular-<hash>.woff2`，CSS 引用 `/assets/...` 正确 |
| 字体度量自证 | 子集内 `中`=2 格、`─`=1 格、拉丁=1 格（hmtx 解析） |

**未做（需真机）**：`pnpm run tauri:android:dev` 在 Android 真机核验——
① 键盘弹收观感与输入栏是否贴上键盘上沿 ② 顶部裁切多少行是否可接受（尤其横屏）
③ TUI（opencode/pi）边框跨行连接与满行中文行尾是否已齐 ④ 内置字体首屏是否有闪跳。
