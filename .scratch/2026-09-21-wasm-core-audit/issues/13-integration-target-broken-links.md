# 13: 五个集成测试 target 的退役符号断链清理

**What to build:** `bedcode-desktop/src-tauri/tests/` 下五个集成 target 恢复到能编译并跑绿，
或在其语义已被 in-tree 用例覆盖时**显式删除并记明被谁覆盖**。目标不是「凑绿」，而是让
AGENTS §10 要求的 `cargo test`（全 target）重新成为一个真实门禁——现在它编译不过，
任何人只看 `cargo test --lib` 就得到半瞎的绿灯。

**Blocked by:** 无（纯测试面，不改生产代码）

**Status:** ready-for-agent

## 现状（2026-09-22 逐 target 实测：`cargo check --test <name>`）

| target | 行数 / 行尾 | 编译错误 | 断链符号 |
| --- | --- | --- | --- |
| `ws_session_route` | 342 / CRLF | 3 | `server::services::pairing_service`、`AppContextBuilder::pairing_service`、`utils::auth::QrTokenManager` |
| `pty_session_chain` | 665 / **LF** | 5 | 上三条 + `SessionManager::from_database`、`SessionManager::restart_session` |
| `ws_auth_rules` | 482 / CRLF | 4 | `pairing_service`（模块 + builder）、`QrTokenManager`、`SessionManager::from_database` |
| `http_auth_biometric` | 552 / CRLF | 4 | 同上（`pairing` 引用最密：37 处） |
| `broadcast_shutdown` | 680 / CRLF | 3 | `pairing_service`（模块 + builder）、`QrTokenManager` |

合计 2721 行测试代码不可编译。`git grep <符号> -- src-tauri/src` 全空 ⇒ 符号确实不存在。

## 断链来源（两批退役，非测试本身的问题）

1. **配对 / QR 的宿主降级实现整体退役**（2026-09-21）：`utils/auth/auth_center.rs` 的配对 / QR 桥接、
   `PairingService`、`QrTokenManager`、`utils/auth/pairing.rs` 与 `AppContext` 装配链全部删除
   （AGENTS §8「配对 / QR 的宿主降级实现已整体退役」）。这些 target 还在按「宿主能签配对码」写测试。
2. **host-session v21 收敛**：`create` / `restart` 从内核删除，创建与重启编排归插件
   （`restart_session`、`SessionManager::from_database` 随批退役）。

现役替代路径（改写时照这个走，**不要**在宿主侧找回归符号）：配对码 / QR 的编排与签发在
`plugins/terminal-session/rust/src/pairing/`，密钥托管与记录面留宿主（`host-auth` secret-store、
`pairings` / `connection_history` 表）；会话创建经 `com.bedcode.terminal-session` 的
`session-create` 编排（`utils/session_create_bridge.rs`，插件必需、无宿主降级）。

## 验收

- [ ] `cargo check --lib --tests` 零错误；`cargo test`（**全 target**，非 `--lib`）跑绿，
      五个 target 各自的断言在恢复后仍测它们原本要测的东西
- [ ] 逐 target 先做**归属判断**并写在票面 Comments：
      ① 语义已被 in-tree 用例覆盖（如 `src/plugin/manager/wasm_runtime/tests/session_e2e.rs`
      的会话闭环、`host_impl/tests/pty.rs` 的属主矩阵）→ 允许删除，但必须点名「被哪条用例覆盖」；
      ② 覆盖不到（HTTP/WS 认证规则面、广播关停时序）→ 改写为经插件路径驱动；
      禁止「注释掉断言」式修复
- [ ] 认证类 target（`ws_auth_rules` / `http_auth_biometric`）改写后必须保住原命题：
      未认证 / 错 token / 过期 / 首消息超时一律拒（AGENTS §8：WS/HTTP 接入必须过认证与过滤链），
      不得因为配对签发换人就削弱断言强度
- [ ] 不碰生产代码：若发现断链暴露了真实的功能缺口（例如某条认证规则在插件退役后**确实**无人测），
      停下来向用户报告，不在本票顺手补实现
- [ ] 行尾纪律：`ws_session_route` / `ws_auth_rules` / `http_auth_biometric` / `broadcast_shutdown`
      是 CRLF，**只有 `pty_session_chain` 是 LF**。改写用 Edit 工具；必须用脚本时
      `open(..., newline='')` + 显式 `\r\n`，改完核 `git diff --ignore-cr-at-eol`
      只剩目标行、CR 计数 == 行数。整 crate `cargo fmt` 禁止

## 门禁跑法（本票专用）

```bash
cd bedcode-desktop/src-tauri
for t in ws_session_route pty_session_chain ws_auth_rules http_auth_biometric broadcast_shutdown; do
  cargo check --test "$t"   # 逐个收敛，别等全量
done
cargo test                  # 全 target；跑前按 AGENTS §3 重出插件产物，核 [skip] 计数为 0
```

## 为什么值得单独一票（登记出处）

同一事实在四处只作为「顺带登记」出现，没有一票接手：
`issues/01` §既有红、`issues/02` 末条、`issues/04`（本票面 L115 附近）、
`handoff-2026-09-22.md` §6，以及根 `CHANGELOG.md`「Tests & Quality」第一条
（原文：*Cleaning them up is a separate item, deliberately not folded into an audit ticket*）。
用户 2026-09-22 定档：单独立票。

## Comments

- 2026-09-22 立项（票 05 收尾时经用户确认「新立清理票」）。
- 并发提醒：`utils/auth/`、`session/`、`server/` 是对侧「终端下沉 / 插件改名」线的活跃改动区，
  开工前先看 `git log --oneline -10` 与 `git status`，避免与对侧在途改动撞在同一批接线文件上；
  同 worktree 双线互卷已有先例（票 05 的 5 个文件被对侧 `5b008eb5c` 一起提交，见 `issues/05` 实施记录第 9 条）。
