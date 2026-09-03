# 命令预设内置移动端本地，不走会话配置下发

移动端输入面板需要按 Agent CLI（claude_code / pi / codex / opencode）加载快捷键命令预设。决定**全程留在移动端本地**：预设表作为静态资源内置移动端、随 App 版本演进；Agent CLI 识别由移动端对会话配置的启动命令（command 字段）文本检测得出（claude/codex/opencode/pi 关键词匹配），识别结果与用户手动覆盖存移动端本地 JSON 文件（按会话配置 id 映射）；切换 Agent CLI 时直接覆盖面板命令列表（不合并）。桌面端 Rust/DB/协议与 SessionForm 零改动。

**Considered Options**:
- 桌面端存储 + `QuickActionList` 下发（协议现成、可编辑、跨设备一致）——需要桌面端 DB 迁移 + Rust 协议 + SessionForm 编辑 UI + 移动端拉取缓存全链路，工程翻倍；且 agent 识别只是移动端输入面板的个性化需求，无需跨设备一致；协议字段保留待未来用户级自定义指令需求再实装。
- 桌面端 DB 仅存 agent_type 标识符（不存预设）——仍需 DB 迁移 + 协议字段 + 表单选择器，而识别本身可由 command 文本在移动端完成，多一层存储无收益。
- 预设按 agent 各配一套按键集——调研四个 CLI（Claude Code / pi / Codex / OpenCode 官方文档与源码）后确认其按键高度趋同（TTY 通用键 + Esc 中断），仅命令集真正区分，故按键集全局一套 16 键、命令集按 agent 分。

**Consequences**:
- 新增/调整预设需随移动端发版，不支持远程热更新；Agent CLI 枚举同步进 CONTEXT.md（Agent CLI / 快捷命令 / 命令预设 / 发送与执行）。
- Agent CLI 识别纯文本关键词匹配，自定义包装脚本启动的会话可能误判——本地 JSON 提供按会话配置 id 的手动覆盖；未识别（generic）不加载预设，保留用户自定义命令，行为与现状一致。
- 命令项结构扩展 `{command, mode}`（发送 = 文本不带回车，skills 类补全场景；执行 = 文本 + Enter），"发送"复用既有 submit 通道（`httpSendSessionInput(text)`），"执行"复用 execute 通道（`httpSendSessionInput(text, 'enter')`）。
- 同一会话在不同移动设备上预设可不同（识别结果存本地）——输入面板个性化本按设备，可接受。
