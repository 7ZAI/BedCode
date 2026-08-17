# 11 — 收尾：兼容标记、文档重写、全量回归

**What to build:** ① 旧协议标记：`/ws/terminal` 旧路由与 WS 认证 compat 分支加注释标记（D2），记录观察期；② 重写 `docs/knowledge/pty-output-pipeline.md` 为新架构（HTTP 认证 + 按需 WS + 每会话连接 + 快照订阅 + TB v2）；③ 本 spec 归档（标记已实施版本）；④ 双端全量回归（cargo test + 单端 vitest + 真机联调：配对/终端/断线重连/多设备）。

**Spec:** §7、§8、§10

**Blocked by:** 08, 10

**Status:** ready-for-agent

- [ ] compat 标记 + 旧协议删除计划记录
- [ ] pty-output-pipeline.md 重写（含新协议样例帧）
- [ ] 全量测试 + 真机联调 checklist 跑通
- [ ] spec 归档（实施版本、遗留项）

## Comments