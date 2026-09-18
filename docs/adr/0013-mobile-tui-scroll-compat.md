# 移动端 TUI 滚动兼容：手势 → SGR 滚轮转发

移动端 xterm 无法滚动 alt-screen TUI 应用（opencode 等）：备用屏幕缓冲区无 scrollback，且移动端输入通道（HTTP-only、`disableStdin`、无 onData 接线）送不进应用内部滚动所需的信号。决定：运行时检测（`buffer.active.type == alternate` + 输出流嗅探 DECSET 1006 鼠标启用，双条件门控）确认 TUI 模式后，把触摸拖动翻译成 SGR 滚轮序列（`ESC[<64/65;col;rowM`）经 WS `ws_send_input_async` 转发到主机 PTY，让应用自己滚动；退出备用屏幕即自动恢复现有 scrollback 滚动。纯前端实现，桌面/移动 Rust 零改动（`ws_send_input_async` 与桌面端 `write_input` 原样写 PTY 均为既有通道）。

**Considered Options**:
- hardcode 会话启动命令白名单（含 opencode 才启用）——覆盖不了"通用 shell 会话里手动启动 TUI"（预设任务只往现有会话注入文本），且每加一个 TUI 应用都要改代码；运行时检测天然覆盖。
- 截帧进 scrollback（iTerm2 式 alt-screen scrollback）——需自维护帧历史并覆写渲染，与 WebGL 渲染器冲突大，工程成本不成比例。
- 仅 alt-screen 条件即转发——vim 等不启用鼠标上报的 TUI 会收到无意义序列，故增加 1006 嗅探条件。

**Consequences**:
- 手势节流 ~40ms 合并发送；发送在途时不丢弃积压（滚动量保留，窗口结束后补发，避免快速翻历史时滚动距离严重缩水），仅积压超过一屏两倍（120 行）时丢弃最旧部分；单次发送上限一屏量（60 行），超出分批补发。与自研 WS client 的背压语义互为防线。
- 第一版只做滚动，不做 tap→点击转发（与长按选择手势仲裁冲突，二期扩展点在 `useTuiCompat`）。
- TUI 模式下隐藏自定义滚动条（alt buffer 下 thumb 恒 100% 误导）；长按选择、清屏、自动跟随维持现状。
- 桌面端不受影响（`TerminalPreview` 已有 `onData` → PTY 通道，滚轮原生转 SGR）。
