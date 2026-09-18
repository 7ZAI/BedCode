# InputSubmitted —— 提交输入行的异步观察扩展点

---
status: accepted
---

插件（auto-task 等）需要观察用户向终端会话提交了什么内容（日志、TODO 等场景）。但 `SessionManager::write_input` 收到的是无结构的原始字节流（每键一块），不存在「提交」概念。我们决定：在 `write_input` 内按会话重建**提交输入行**（可打印字符累积、回车触发、退格编辑、控制字符与 ESC 序列丢弃、括号粘贴块视为内容），以新的 `SessionInputListener`（仿 `SessionLifecycleListener` 模式）异步分发给注册插件；SDK 以类型化 `InputSubmittedEvent { session_id, text }` 定义线协议；注册需 `terminal:observe` 权限。

## Considered Options

- **逐块观察**（每次 `write_input` 分发过滤后的可打印片段）：实现最简单，但插件收到的是碎片（`"hel"`、`"lo"`），拼接负担推给每个插件，「提交」语义不存在。否决：与需求「输入完成提交」不符。
- **MessageBus 主题**：零新 WASM 导出，但 payload 是字符串 JSON（SDK 明确反对的契约漂移风险），且总线定位是插件间通信、订阅为 manifest 静态声明。否决：不匹配「注册监听 + 回调」。
- **同步钩子（回调完成后才写 PTY）**：有顺序保证，但每次回车阻塞在所有插件回调上，第三方插件故障直接拖垮输入体验。否决：观察不需要顺序保证；修改/否决能力已有归属（`TerminalHandler::on_input` 同步管道）。

## Consequences

- **职责分界**：`TerminalHandler` = 修改输入（同步、PTY 写入前）；`InputSubmitted` = 观察输入（异步、纯通知、错误隔离）。两个扩展点不重叠。
- **有损重建（已知且接受）**：TUI 应用（Claude Code）历史回溯召回的文本不经过输入流，日志会缺行；终端启用 modifyOtherKeys 模式时多行手敲输入（修饰键+回车的 CSI 编码）会被丢弃。常见 Shift+Enter 编码 `\x1b\r` 已正确还原为换行内容；主路径（键入 + 粘贴多行 + 回车提交）正确。
- **宿主不做语义过滤**：空提交（空行回车）同样触发事件，是否忽略由插件业务决定。
- **明确延后**：① 移动端 WS 路径把特殊按键转控制字节经 `write_input` 发送，与桌面端 `send_special_key` 通道不一致——分类规则已防御性中和（控制字节被丢弃），路径重构另行立项；② 插件在回调中调用 `terminal_send` 会再次触发事件，自循环风险靠插件编码约定规避（不在机制层防护，与 `TerminalHandler` 现状一致）。
- **范围**：v1 仅 WASM 插件（与 `SessionLifecycleEvent` 现状一致）；TS-only 插件支持后续经 Tauri 事件转发补齐。静态 Rust 插件经 `TerminalHandler::on_input_submitted` 默认方法同步获得提交行。
