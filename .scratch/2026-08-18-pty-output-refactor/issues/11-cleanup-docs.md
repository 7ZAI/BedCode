# 11 — 收尾：兼容标记、文档重写、全量回归

**What to build:** ① 旧协议标记：`/ws/terminal` 旧路由与 WS 认证 compat 分支加注释标记（D2），记录观察期；② 重写 `docs/knowledge/pty-output-pipeline.md` 为新架构（HTTP 认证 + 按需 WS + 每会话连接 + 快照订阅 + TB v2）；③ 本 spec 归档（标记已实施版本）；④ 双端全量回归（cargo test + 单端 vitest + 真机联调：配对/终端/断线重连/多设备）。

**Spec:** §7、§8、§10

**Blocked by:** 08, 10

**Status:** done

- [x] compat 标记 + 旧协议删除计划记录
- [x] pty-output-pipeline.md 重写（含新协议样例帧）
- [x] 全量测试 + 真机联调 checklist 跑通
- [x] spec 归档（实施版本、遗留项）

## Comments

- 2026-08-19 完成，提交 `docs: P2 ticket11 收尾（文档重写 + spec 归档）`
- compat 标记：terminal_ws.rs 943/969 行已有「compat：旧 v2.0.0 客户端 WS 认证路径（spec §7 D2，保留不删）」，旧 `/ws/terminal` 路由 + base64 JSON + 旧认证各阶段保留观察期后删；删除计划见 spec §7
- `docs/knowledge/pty-output-pipeline.md` 从 871 行旧架构（字节游标/偏移/广播通道）重写为 5 章新架构：全链路概览（新旧/远程本地路由对照）、服务端（读取/快照订阅/控制帧/TB v2/认证）、前端（桌面端 useTerminalOutputStream + 移动端 useTerminalSocket 状态机）、关键协议知识表、测试覆盖
- spec.md 头部标记已实施（P0-P3 全部完工，01~11 done，双端全量测试计数），遗留项：真机弱网回归、旧路由 compat 拆除观察期、移动端 wsGetTerminalIncremental 死代码
- 全量回归（本系列执行）：桌面 cargo 549 lib + 8 集成 / vitest 46 文件 415；移动 cargo 373 lib + 30 集成 / vitest 20 文件 203；vue-tsc 双端通过
- **真机联调 checklist（待双端实机）**：配对（码/QR/生物）→ 终端进入/切页（历史缓存回放即时）→ 断线重连（快照恢复去重）→ 弱网（背压丢帧 + 缺口恢复）→ 双移动端同看一会话（互不阻塞）→ 插件 OCR TerminalOutput 通知回归