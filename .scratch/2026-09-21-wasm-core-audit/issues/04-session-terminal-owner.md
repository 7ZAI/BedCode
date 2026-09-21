# 04: 会话 / 终端域属主闭环（P0-3，终端命令注入）

**What to build:** 「先过权限门，再查属主」这条既有不变量补齐到会话与终端域：插件只能操作**自己创建**（或被显式授权）的会话，生命周期监听的注册需要权限。修完后，持 `terminal:input` 的第三方插件无法再向用户正在用的交互终端注入命令。

**Blocked by:** 01（权限位与门禁测试形态）

**Status:** ready-for-agent

## 现状（已复核）

- `host_impl/terminal.rs:16` `terminal_send` 只查 `terminal:input`，直接 `session_manager.write_input(session_id, ...)`，不比对归属；
- `host_impl/session.rs:453` `session_close`、`:506` `session_remove` 只查 `session:write`；`session_close` 的文档注释自己写「关闭**自己创建**的会话」，代码未实现；
- `host_impl/lifecycle.rs:11` `session_lifecycle_register` **无任何权限门**（对照同文件 `:26` 的 `session_input_register` 需 `terminal:observe`）→ 任意已激活插件可得全部 session id/名称，成为注入链的第一环；
- 反证「这是遗漏非取舍」：pty / ws / mdns 三域已有统一属主判定与文案（`host_impl/pty.rs:49`、`ws.rs:51,54`、`mdns.rs:91`），core-task 的任务记录带 `owner` 字段（`manager/task.rs:111,145`）——本票照此形态补齐会话/终端域即可，无需发明新机制。
- 待本票一并核实的两项（来源审查记录，未逐行复核）：`host_impl/peer.rs:61,141` 句柄表无 owner 列；`host_impl/session.rs:645` 注解槽为全局扁平 map，跨插件同键互相覆盖。

## 验收

- [ ] **先落红测**：插件 B 用插件 A 的 session id 调 `terminal_send` / `session_close` / `session_remove` → 当前实现必须成功（这就是红），修复后必须 `not owner` 拒绝；照 `test_pty_isolation_and_contract_matrix_roundtrip` 的闭环形态
- [ ] 会话创建时登记属主（`plugin_id`），`close/remove/rename/resize/annotate` 一律先权限后属主；`create-with-spec` 若允许指定 id（v19 重启编排用），需明确「同插件重启自己的」而非任意 id
- [ ] `terminal_send` 增加属主判定；同时评估：终端输入注入是否需要与 `terminal:observe` 同级的**用户在场确认**（高危位，见票 03 裁决项 3）
- [ ] `session_lifecycle_register` 补 `session:read` 门（与输入监听的 `terminal:observe` 分档保持：生命周期是元数据、输入行是明文凭据面）
- [ ] 注解槽改按属主分命名空间（`plugin_id` + key），跨插件不可互相覆盖；`peer` 句柄表补 owner 列并统一错误文案
- [ ] 界面维持：`com.bedcode.session` 插件的现有会话编排不因属主校验而 Broken（它本就是属主），WIT 函数签名不变则不 bump ABI；若签名要改（如加 requester 参数）→ 按 ADR 0022 双端偏离登记并说明移动端影响
- [ ] 门禁：`cargo test` 全绿 + 相关 `session_e2e` / `terminal` 闭环用例全绿；`cargo check --lib --tests`（防假绿）

## Comments

- 2026-09-21 立项：来源 spec §4-P0-3。与票 03 的关系：票 03 截断「谁能拿到 `terminal:input`」，本票截断「拿到后能碰谁的会话」，两票正交、都要做。
