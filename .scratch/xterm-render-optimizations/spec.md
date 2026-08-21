# xterm 渲染输出优化 — Spec

Status: ready-for-agent

> 前置探索与 VS Code 源码对照详见同目录 `plan.md`（2026-08-21 创建，含逐项参考文件与行号）。本 spec 为计划的可执行规格，测试缝合点已与用户确认。

## Problem Statement

桌面端终端（BedCode desktop）在真实使用场景下存在三类可感知的渲染/吞吐问题，而核心渲染优化（WebGL 渲染器 + 纹理图集、Unicode 11 宽度、rAF 合并写入）已就位：

1. **窗口拖动 resize 开销大**：任何方向（含水平）的尺寸变化都走「rAF 节流 + fit + 条件全量 refresh」同一路径。水平 resize 会触发整屏 reflow（昂贵），拖窗时逐帧重复执行；而垂直 resize 又得不到即时响应。
2. **高分屏行列数不精确**：fit addon 不感知 `devicePixelRatio`，Windows 150%/200% 缩放或 DPI 变化时 cols/rows 计算不精确，导致文字模糊/行尾截断。
3. **写入管线单向洪水、无背压**：`pty_reader.rs` 无限 `while` 读 → 环形队列 → WS 推送 → 前端 `writeQueue`（仅有防御性上限 + `setTimeout(0)` 让出）。`cat` 大文件 / `tail -f` / 构建输出风暴时峰值内存与 writeQueue 积压不可控，UI 可能冻结。

另有低优先项：unicode 符号走 canvas 光栅化（渲染不一致、开销高）、平滑滚动因 WebGL 重影被整体关闭（物理滚轮/触控板无差别禁用）。

## Solution

参照 VS Code 终端实现对桌面端写入/渲染链路的节制层优化，核心是「让渲染解析速度反向钳制数据源流速」与「让昂贵的重绘只发生在真正需要时」：

- **P1 resize 分层**：垂直 resize 立即处理；水平 resize 单独 100ms 防抖；仅当 cols/rows 实际变化才全量 refresh + 同步 PTY；`flush()` 保证拖动结束后最终尺寸必达。决策逻辑抽为纯模块，组件只留接线。
- **P1 custom glyphs**：WebGL 渲染器启用 `customGlyphs`，更多 unicode 符号直接 GPU 光栅化。
- **P2 背压反馈环**：复用 TB v2 帧 `seq`——前端写入解析完成后回发 ack 帧（携 `last_rendered_seq`），Rust 侧据此暂停/恢复从 PTY 读取，渲染速度反向钳制源头流速，压住峰值内存与 writeQueue 积压。
- **P2 DPR 感知行列计算**：移植 VS Code `getXtermScaledDimensions`（`devicePixelRatio` 换算 canvas 像素），高分屏/多屏 DPI 变化时行列数精确。
- **P3 平滑滚动（前置问题解决后）**：滚轮分类器区分物理滚轮/触控板，仅物理滚轮开平滑滚动；先查清 WebGL 滚动重影根因，若为版本 bug 升级后可引入，否则维持 0 是正确决定。

## User Stories

1. 作为桌面终端用户，我希望拖动窗口水平改变宽度时 resize 被防抖合并，这样拖窗过程中不会每帧触发一次整屏 reflow。
2. 作为桌面终端用户，我希望垂直方向改变高度时 resize 立即生效，这样行数与布局对高度变化即时响应。
3. 作为桌面终端用户，我希望拖动结束后终端最终尺寸一定与窗口一致（flush 保证），这样不会残留尺寸偏差。
4. 作为桌面终端用户，我希望仅当 cols/rows 实际变化时才触发全量重绘与 PTY 尺寸同步，这样 subpixel 抖动/微调不会造成无谓重绘。
5. 作为 Windows 150%/200% 缩放或高分屏用户，我希望终端网格行列数按 devicePixelRatio 精确计算，这样文字清晰、不模糊、不截断。
6. 作为多屏 / DPI 动态变化环境用户，我希望 DPR 变化时行列数随之正确重算，这样跨屏拖动后显示依旧精确。
7. 作为运行 `cat` 大文件、`tail -f` 日志风暴或长构建输出的用户，我希望写入管线有背压，这样峰值内存与 writeQueue 积压被钳制、UI 不冻结。
8. 作为使用 TUI 应用（opencode / vim 等）的用户，我希望 unicode/box-drawing 符号由 WebGL custom glyphs 光栅化，这样渲染一致且开销更低。
9. 作为桌面终端用户，我希望 WebGL 上下文丢失时 custom glyphs 启用后仍能 1s 恢复/回退 DOM，这样渲染器恢复路径不被回归。
10. 作为触控板用户，我希望滚动保持即时（无平滑），这样没有跟手延迟。
11. 作为物理滚轮用户（P3 落地后），我希望滚轮滚动启用平滑动画，这样滚动手感自然。
12. 作为维护者，我希望背压 ack 复用现有 TB v2 帧 `seq` 作为坐标空间，这样不引入新的字节游标/坐标系。
13. 作为维护者，我希望背压仅在渲染解析跟不上时介入、跟上时零开销放行，这样本地环回高吞吐场景收益不被反噬。
14. 作为实现者，我希望背压暂停读期间不丢字节——ack 恢复后继续转发全部输出，这样与「快照重订阅 + 重播跳过已渲染」语义兼容。
15. 作为实现者，我希望现有终端测试（seq 连续性、快照重订阅、历史截断、断线重连）保持全绿，这样 ack/背压改动不破坏快照模型契约。
16. 作为维护者，我希望 resize 决策逻辑、DPR 计算、滚轮分类器是纯函数/纯类，这样无需 DOM/GPU 即可单测。
17. 作为实现者，我希望背压落地后 `writeInChunks` 的 `setTimeout(0)` 让步得以简化，这样管线层次更清晰。
18. 作为产品决策者，我希望获得平滑滚动重影根因的明确结论（版本 bug vs alpha 缓冲），这样能决定是升级 xterm/webgl 后引入还是维持关闭。
19. 作为可选诉求用户，我希望在启用字体连字时 WebGL 渲染器自动重建，这样纹理图集能吸收 DOM 侧字体样式。

## Implementation Decisions

- **范围**：桌面端 `TerminalPreview`（组件）、本地 WS 输出流 composable、PTY 读取器、TB v2 转发链路。移动端 TerminalView 不在本 spec 范围（其 WebGL init 更简、无对应优化诉求）。
- **resize 分层**：新独立纯模块（对齐 VS Code `TerminalResizeDebouncer` 语义）——垂直立即分支、水平 `DebounceResizeXDelay=100ms`、小尺寸变化立即分支、`flush()` 保证最终尺寸必达。调用方（组件）仅把「容器尺寸→回调」接线到该模块；仅 cols/rows 实际变化才触发全量 refresh + PTY 尺寸同步，subpixel 抖动不触发。
- **custom glyphs**：WebGL 渲染器构造时启用 `customGlyphs: true`。BedCode 单窗口无 VS Code 辅助窗口 canvas 被 block 的限制，可直接启用；需实测内置 glyph 覆盖差异决定是否保留。
- **背压反馈环（wire 契约）**：前端在每次 write 解析完成（`onWriteParsed`）后，经现有 WS 连接回发 ack 帧，内容为已渲染到的 `last_rendered_seq`（TB v2 帧 seq 维度，对齐 VS Code「字符数 ack」语义但改用帧 seq）。Rust 侧收到 ack 前，当未 ack 字节/水位超过阈值时暂停从 PTY read；ack 推进后恢复。前端只对「已被消费（渲染完成）」的帧 seq 回发，未渲染部分不 ack。控制消息仍走现有文本协议，ack 走二进制帧（复用帧编码，带 ack 标记位/新帧类型）。
- **背压收益边界**：明确接受「本地环回 WS 吞吐远高于 VS Code remote 场景、背压增益较小」——收益以压住 `cat` 大文件 / `tail -f` / 构建输出峰值内存与 writeQueue 积压为主，不作为吞吐提升承诺。
- **DPR 感知行列计算**：移植 VS Code `getXtermScaledDimensions` 语义为纯函数（`devicePixelRatio` 乘宽高、ceil 行高、floor 列宽），替换 fit addon 的裸 DPR 不感知计算；保留 fit 触发路径，仅替换尺寸计算。配套可选：优先取 xterm 实测 cell 尺寸做字体测量回退。
- **平滑滚动（P3，gated）**：仅当重影根因查清（版本 bug → 升级 xterm/webgl；alpha 缓冲 → 维持关闭）后落地。滚轮分类器按 VS Code `MouseWheelClassifier` 语义提取为独立纯类，仅对物理滚轮开 `smoothScrollDuration=125`，触控板保持即时。
- **不新造字节游标**：背压 ack 与现有 seq 连续性/重播去重共用 `last_rendered_seq`，不引入独立坐标系；快照重订阅语义保持不变。

## Testing Decisions

- **好测试的标准**：只测外部行为，不测实现细节——防抖类断言「给定尺寸序列与时间推进，回调是立即还是延迟触发、flush 后最终尺寸必达」；背压断言「未 ack 超水位时读暂停、ack 推进后恢复、暂停期间零字节丢失」；DPR 断言「给定 devicePixelRatio 与 cell 尺寸返回精确 cols/rows」。不 mock WebGL/GPU、不测组件内部 DOM。
- **Seam A（主导，纯逻辑模块单测）**：resize 防抖模块（vitest fake timers 推进时间断言立即/延迟分支与 flush）、DPR 行列计算纯函数、P3 滚轮分类器。先例：`useTerminalOutputStream.test.ts` 用确定性 mock 与时间推进；vitest fake timers 可支撑防抖时序断言。
- **Seam B（现有 composable）**：`useTerminalOutputStream` 扩展——复用现有 `MockWebSocket` 测试骨架，断言 ack 帧在帧交付/解析完成后发出、携正确 `last_rendered_seq`、窗口上限内不回发多余 ack、重连/重订阅后 ack 语义不变。先例：同文件现有 TB v2 帧构造器与 seq 游标断言。
- **Seam C（现有 Rust）**：PTY 读取器背压——复用 `MemoryReader` 可控读模式，断言「水位超限暂停、ack 推进恢复、恢复后全部字节转发无丢失」；ack 帧 wire 编码按 `forward.rs` 现有帧编码单测风格覆盖（帧头/标记位/seq 透传）。先例：`reads_output_and_reports_stopped_on_eof` / `empty_input_exits_with_stopped_lifecycle` 与 `test_output_buffer_v2_*`。
- **回归门禁**：`useTerminalOutputStream.test.ts`、`terminalScrollback.test.ts`、`integration/terminal-flow.test.ts` 保持全绿；涉及 Rust 改动跑 `cargo test`（pty 模块现有用例模式）。
- **真机/真实场景回归（非自动）**：高分屏 150%/200% 缩放、多屏 DPI 变化、拖动窗口连续 resize（含水平快速拖）、`cat` 大文件、`tail -f` 日志风暴、WebGL context loss 恢复路径。

## Out of Scope

- 移动端终端渲染（TerminalView 不在范围）。
- 字体连字（ligatures）：可选，仅当产品有连字诉求时立项。
- 平滑滚动落地本身：P3 被「先查清 WebGL 重影根因」前置，本 spec 只含分类器提取与决策框架。
- 计划中已明确跳过项：addon 懒加载（本地 Tauri 无启动体积诉求）、无缝重启缓冲（已被快照重订阅 + 截断→清屏全量重播覆盖）、OSC 133 序列拆分（无 shell-integration）、不可见时 idle resize（常驻窗口不划算）、DOM renderer 懒初始化、WebGL 纹理图集截图（无截图需求）。

## Further Notes

- VS Code 源码基准与逐项参考行号见同目录 `plan.md`（2026-08-21 核对，相对 `D:/vscode-main/src/vs`）。
- **并发在途改动警告**：`git status` 显示 `system.rs` / `lib.rs` / `pty_reader.rs` / `logging.rs` / `TerminalPreview.vue` / `package.json` / `package-lock.json` 存在未提交改动（疑似并发 file-transfer 会话遗留，见 `plan.md` Comments）。实施前必须确认这些文件无重叠在途工作，避免冲突；本 spec 触达的正是其中 pty_reader.rs 与 TerminalPreview.vue。
- 背压落地前先接受「本地场景收益有限」的排期前提；落地后 `writeInChunks` 的 `setTimeout(0)` 让步可顺带简化。
- P1 项低风险高回报、建议先做；P2 项中等投入、收益需评估；P3 谨慎、被前置问题 gate。

## Comments

- 2026-08-21：由 `.scratch/xterm-render-optimizations/plan.md` 生成；测试缝合点（Seam A 纯逻辑提取 + Seam B 现有 composable + Seam C 现有 Rust + 真机回归）已与用户确认。状态置 `ready-for-agent`，无需额外 triage。

## Comments

- 2026-08-21（接手后）：P1 落地并验证。01 resolved（防抖纯模块+单测+组件接线）；02 resolved（terminalDimensions.ts 补齐修复 import 断裂+单测）；03 代码完成待真机 glyph 覆盖验证；07 resolved（根因=alpha 帧缓冲，条件化 allowTransparency 已修，决策维持 smoothScrollDuration 0，08 作废）。详见 issues/ 下各票。下一步候选 = 04-06 背压反馈环（前置：接受本地环回收益有限的排期前提 + 清理/隔离 pty_reader.rs 等文件中另一会话的临时 dump 注释残留后再动 Rust 侧）。

- 2026-08-21（背压落地）：04/05/06 已实现（wire 契约 = TB v2 头 + ACK 标志位 0x02 + acked_seq + session_id 负载；Rust 账本 1MB 水位 + 8K FIFO 冻结；PtyReader read 前查水位暂停 5ms 轮询零丢失；前端 onWriteParsed→confirmWriteParsed 64KB+250ms 节流）。`writeInChunks` 的 `setTimeout(0)` 保留未简化（另一会话 WRITE_YIELD_THRESHOLD 水线，见 issue 06）。e2e 真机回归待做。验证：cargo test --lib 567 绿 / vitest 436 绿 / vue-tsc 干净。
