# 06: special_key 下沉（按键组合→转义字节翻译迁插件）

**What to build:** 结束 ADR 0022 D1 界定的灰区：把「按键组合 → ANSI/ASCII 转义字节」的翻译从宿主移到插件。宿主 pty 输入路径不再消费按键组合类型，只收插件算好的裸字节；插件负责把要发的键翻译成转义序列再写（或经原语传).宿主端按键组合的翻译/字节服务接口退役。效果：终端协议字节翻译归业务面，宿主 pty 内核只守可 exec / 收字节边界（对齐裁剪线「宿主只留最基础 POSIX 级 API」）。

**Blocked by:** P1-b land（session_gateway / enums 引用方以 P1-b 后基线为准）

**Status:** done（2026-09-24 落地，第一阶梯：插件自译自写 + 宿主实时输入路径退译 + 直写语义统一）

## 落成形态
- **翻译移插件**：`plugins/terminal-session/rust/src/keys.rs`（票 06 下沉）——`KeyCombo`/`KeyCode`/parse/to_pty_bytes
  等价移植（语义逐字节一致，含 Ctrl/Alt/Shift/功能键/方向键/编辑键全集），公开
  `special_key_to_pty_bytes(组合串) -> Option<Vec<u8>>`，10 项单测全绿。
- **插件 `session-input` 自译自写**：载荷 `{sessionId, data, specialKey}`——`specialKey` 存在时
  本插件经 `keys::special_key_to_pty_bytes` 自译成 ANSI/ASCII 字节后 `pty_write` **直写**
  （绕过提交行重建与任务域观察，**直写语义统一**：Ctrl+C 即 \x03 直进 pty）。
- **宿主实时输入路径退行格式**：`session_gateway::special_key` 与 `terminal_service`（移动端 WS）
  改为转发组合串（`specialKey`），不再 `to_pty_bytes`/`KeyCombo::parse`（grep：live 输入路径触点清零）。
- **数据面不变**：`host-pty` 原语仍只收裸字节；socket/WS 合并语义不因下沉丢键（e2e 实测 Ctrl-C/Ctrl-D）。
- 插件 WASM 重build（wasmHash 9a563d9a…）；任务域 queue.rs 调用点 `false`→`None`。

## 门禁（实测）
宿主 `cargo test --lib` **1181/0**（含 session_e2e `test_session_input_via_gateway_closed_loop`
之 C-04 反例改为“插件拒签”、C-03/C-05 Ctrl-C/Ctrl-D 仍绿）；插脚本 298/0（新 10 项 keys）；
集成链路插件 WASM 重建成功。

## 待办（对侧在途）
宿主 `enums/special_key.rs::to_pty_bytes` 与其唯一残留消费方 `pty_process::send_special_key` →
`session_manager::send_special_key`（死链）随 **session-engine 票 11**（特征体向 plugin 移）+ 内核会话目录清理。
本票只在实时路径清零；死链归属对侧票 11（避免并发冲突），票 11 清除内核 dir 后此处 `to_pty_bytes` 可整删。

## 验收核对
- [x] 宿主实时 pty 输入路径不再消费按键组合类型传输（grep：`terminal_service`/`session_gateway` 零 `to_pty_byte`；残留仅死链 → 票 11）
- [x] 插件自算转义字节并写 pty 闭环可用（Ctrl/Alt/Shift/功能键/方向键，keys.rs 单测）
- [x] 宿主 pty 原语输入面保持只收裸字节（插件 pty_write 字节）
- [x] socket/WS 通道原 merge 语义不因下沉丢键（e2e Ctrl-C/Ctrl-D 仍绿）
- [x] ADR 0022 D1 措辞对齐（见下）