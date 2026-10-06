# 01: 量测复核 + 认领在途改动（gate）

**What to build:** 见 spec §6（行内）与对应 spec 章节。

**Status:** done

**Type:** task

## 结论

**① 量测复核（工作区 HEAD `132f6846e` + 在途 +44/−31，2026-10-06 实测）**

| spec 声称 | 实测 | 处置 |
| --- | --- | --- |
| `wasm_core/` = 27,406 行 / 119 文件 | **54,394 行 / 119 文件**（`find + wc -l` 与 `git ls-files + wc -l` 双口径一致；前作 spec 在 `dbe50d229` 测 57,592 行，lib-split 迁出四域约 3,198 行 → 54,394 ✓ 自洽） | **spec 行数错（约 2x），已修 spec §1.1 / §3.1 / §8**；文件数 119 ✓ |
| `db/` = 404 + schema | 387（database 311 + models 22 + operations 54）+ schema.sql 73 | 已修 spec |
| `pty/` = 2,469 | 2,469 ✓ | 无 |
| `enums/` = 134 | **33**（plugin 6 + pty_status 15 + special_key 12） | 已修 spec（134 疑似把别的量并进来） |
| `system/process.rs` = 22 | 22 ✓ | 无 |
| `system/opener.rs` = 378 | 378 ✓ | 无 |
| `system/config.rs` = 839 | 839 ✓ | 无 |
| `auth_center.rs` = 216 | 216 ✓ | 无 |
| `session_gateway.rs` = 254 | 254 ✓ | 无 |
| `test_tokens.rs` = 130 | 130 ✓ | 无 |
| HostBusPort ≈ 70 | ports_impl.rs:81-130 ≈ 53 行（struct + impl） | 已修 spec（≈53） |
| 反向 lib→wasm_core = 9 文件 | commands.rs / lib.rs / server/ports_impl.rs / system/app_context.rs / system/lifecycle.rs / utils/auth.rs / utils/auth/auth_center.rs / utils/auth/test_tokens.rs / utils/session_gateway.rs = 9 ✓（+ crypto.rs / db.rs / crate_boundary_lock.rs 三处是锁/垫片自身的文本引用，不算消费方） | 无 |
| 5 集成测试 | ws_auth_rules / http_auth_biometric / pty_session_chain / broadcast_shutdown / wasm_bridge_bench(support.rs) ✓ | 无 |
| cross-end `desktop_ctx.rs` | `use bedcode_desktop_lib::db::Database` + `wasm_core::PluginHost` + composition/app_context ✓ | 无 |
| `bindgen!` 路径 | component.rs:43 `"../packages/plugin-sdk-desktop/rust/wit/bedcode.wit"`；discovery-engine 先例 :82 `"../plugin-sdk-desktop/rust/wit/bedcode.wit"` ✓ | 迁入 crate 后按先例改 |
| `runtime_util` `pub(crate)` | wasm_core.rs:36 `pub(crate) mod runtime_util` ✓；lib ports_impl.rs:311 消费 `crate::wasm_core::runtime_util::ambient_handle()` ✓ | 需改 `pub`（spec §3.1 注） |
| `AppHandleScope` 已存在 | context.rs:707 trait + :850 impl ✓ | 无 |

**② 在途改动认领（`git status` = 4 文件 +44/−31，2026-10-06 实测）**

| 文件 | 内容 |
| --- | --- |
| `wasm_core/host_api.rs` | +25：新增 `install_capability_domain_ports` 单入口装配函数 |
| `wasm_core/manager/host.rs` | −10/+2：`PluginHost::new` 改调单入口 |
| `wasm_core/manager/host/tests/scaffold.rs` | 夹具改调单入口 |
| `wasm_core/manager/runtime.rs` | 夹具改调单入口 |

与 spec §1 † 声明逐字一致（`install_capability_domain_ports` 单入口收口，wasm-core-lib-split 收尾工作）。**认领结论**：这批改动属并发会话的 lib-split 收尾线，与本票（整核抽出）正交不冲突；本票**不碰、不回滚、不提交**（AGENTS §11），机械搬迁时它们随 `wasm_core/` 目录整体移动、内容保留。mtime 检查（近 2h 零改动）确认并发会话当前不在写这四文件；若迁移期间它恢复写入，按 spec R5 停手等待，不得同文件双写。

**③ ⚠️ 分期矛盾裁定（spec D11 与票 02 验收冲突，本 gate 需修正）**

票 02 验收写「crate 根 cargo check 绿」，但实测 wasm_core 生产段引用 lib 模块：`crate::db`（20 文件）、`crate::pty`（3）、`crate::system::{config,opener,app_context}`（pty.rs / config.rs / host.rs / runtime.rs / platform.rs / mdns.rs / watcher.rs / boot.rs / errors.rs）、`crate::server::{peer_net_cmds,ports_impl,crate_boundary_lock}`（mdns/peer/activation/ws/register/api_bridge）、`crate::utils::auth`（auth.rs）、`crate::enums`（pty.rs）。**这些模块在票 03/04 才迁入** ⇒ 纯 wasm_core 入 crate 后 `crate::db` 等必然解析失败，票 02 的「crate check 绿」不可达。

**裁定**：票 02/03/04 的机械搬迁（M1-M11）+ 胶水替换（AppContext→注册表、peer_ctx→端口、PluginHost::new 增参、harness）在**编译闭包上是不可分割的**——crate 只有拿到全部引用闭包（M1-M11 + 注册表 + 端口 + harness）才能 `cargo check` 绿。执行上按 D11 的精神仍分步做（每步 src-tauri 侧经垫片保持可编译），但「crate 根 check 绿」的验收点移到整链机械搬迁完成之后。已同步修正 spec §6 验收列（标注「crate check 绿在 M1-M11 闭包完成后首次可达」）。issues/02..04 的验收按修正后的读法执行。

**④ 其他 gate 事实**
- 磁盘 98%（3.7G 可用）；`target/server-libs` 16G + `target/host-kits` 6.4G。票 02 开工前需 `cargo clean` 相关 target（spec R6），已确认工作树无他人 target 产物风险（只清编译产物不动源码）。
- sccache 0.18.0 在位（根 .cargo/config.toml rustc-wrapper 强制），当前 0 编译请求——首个全量构建会全 miss，但后续增量靠它。
- 锁更新清单（spec §7.3）中 `hot_path_logging_lock.rs` 的 `LOCKED_SITES` 现仍指 `src/wasm_core/…`，随票 05 同步改路径。

## Comments

- 2026-10-06：本票零代码改动（gate 纯量测 + 认领 + 文档修正）。改动仅：本文件 + spec.md 量测数字/验收列修正。
- 2026-10-06：`crate::session` 在 wasm_core 内的 1 处引用经核为**非生产段**（`grep -rl` 结果为空），不构成出边。
- 2026-10-06：host_api 的 `install_capability_domain_ports` 是 `pub(crate)`，随迁后保持 crate 内部可见即可（生产与两个夹具都在 crate 内）。
