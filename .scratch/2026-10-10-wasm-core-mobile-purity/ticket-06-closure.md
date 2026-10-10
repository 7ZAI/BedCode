# 票 06 · 收口（锁 / 文档 / CHANGELOG / ADR）——实施记录（2026-10-10）

> 输入：`spec.md` §4 票 06 + §5 统一验证清单 + §6 R11。
> 前置：票 01（`ticket-01-domains.md`）→ 02（`ticket-02-mapping.md`）→ 03（`ticket-03-peer-reuse.md`）
> → 04（形态换型 + 移动独有域宿主化）→ 05（`ticket-05-domain-sinking.md`）。

## 1. ① 防回接锁（新增）

`bedcode-mobile/src-tauri/tests/mobile_wasm_core_purity_lock.rs`（**6 例**）：

| 判据 | 用例 | 拦什么 |
| --- | --- | --- |
| 移动独有域不回流 | `mobile_owned_domains_stay_in_mobile_host` | `bare/host_impl/` 不得再出现 5 个域文件 + 移动宿主 adapter 必须在场（正向） |
| 生产装配段无其 `Host` impl | `production_linker_does_not_own_mobile_only_interfaces` | 按 `#[cfg(test)]` 切段：crate 自测替身合法，**生产**路径长出实现即红 |
| 壳不含域逻辑 | `form_shells_do_not_regrow_domain_logic` | http 壳禁 `reqwest` / `execute_*`；fs 壳禁 `std::fs::` / `create_dir_all` / `remove_file`；全目录禁 `HandleTable` / `fn redial`（跳注释行） |
| 共享实现核在场 | `shared_implementation_cores_stay_addressable` | `server-http::egress::http_fetch` / `host-api-core::fs::fs_read` / `server-peer-net::plugin_binding::peer_*` + 两壳的转发证据 |
| 壳行数双上限 | `form_shells_stay_thin` | 单文件 ≤ 400（现最大 ws.rs 333）/ 目录合计 ≤ 2400（现 2045） |
| 退役引擎不复活 | `retired_mobile_http_engine_stays_retired` | `host_api/http_engine.rs` 不得存在 + `host_api.rs` 无 `pub mod http_engine` |

**变异自检 4/4**（注入 → 红 → 还原核 sha256 → 绿）：

| # | 注入 | 转红用例 | 还原 sha256 |
| --- | --- | --- | --- |
| ① | fs 壳插入 `std::fs::read_to_string(path)` | `form_shells_do_not_regrow_domain_logic`（点名 `fs.rs:112`） | `d9ebedef…40a6` 一致 |
| ② | 伪造 `bare/host_impl/notify.rs` | `mobile_owned_domains_stay_in_mobile_host` | 文件已删除 |
| ③ | 生产段注入 `#[cfg(any())] impl host_notify::Host` | `production_linker_does_not_own_mobile_only_interfaces` | `3ed40455…3f9c` 一致 |
| ④ | ws 壳追加 80 行（333→413） | `form_shells_stay_thin` | `5ab22784…46bf` 一致 |

注：③ 首版注入未加 `cfg(any())` ⇒ 方法集不全导致**编译期**即红（E0046）。这本身也是
「回接会被拦」的证据之一，但不构成用例级验证，故改为可编译形态重跑。

## 2. ② 双端 code-map + 锁索引

- `bedcode-mobile/docs/code-map.md`：wasm-core 段改写为四层（机制核 / 运行时与装配 /
  共有机制域壳「纯转发 + 域逻辑在共享 crate」/ 端口与适配），并列出移动独有域 adapter
  与 `bindings.rs`；`src-tauri/src/plugin/` 段补 adapter 与端口接线；锁索引新增本锁条目。
- `bedcode-desktop/docs/code-map.md`：`bedcode-server-http` 行补出站执行 + `jwt_auth_token`
  可选端口（桌面不覆写 = 零变化）。

## 3. ③ ADR / ④ CHANGELOG / ⑤ AGENTS

- **ADR 0047**：状态行更新为「票 02–05 已落地、票 06 收口中」；实施记录补票 03 / 04 / 05 / 06
  四条（含票 04 的四个机械伤修正点：`bedcode_host_kit::limits::defaults::*`、
  `self.host_ctx(state)`→`host_ctx(self.state)`、`block_on_async` 单参形态、`StoreSpec`
  需进 `runtime.rs` 的 bare 形态 re-export）。
- **CHANGELOG.md / CHANGELOG_zh.md**：`[未发布]` 段顶新增双语条目（命名中立化 / 移动独有域
  落移动宿主 / 含逻辑域下沉 / 防回接锁 / 零 ABI·WIT 变更）。
- **AGENTS.md §5.4**：补「形态命名与落点纪律」一段（零端名·零机制名 + 域落点三则 + 锁名）。

## 4. ⑤ 统一验证（实跑结果）

| 项 | 结果 |
| --- | --- |
| 内核 bare 形态 `cargo check`（`--no-default-features --features bare-host`） | **0 error**（修 3 处票 02–04 机械伤） |
| 内核 common 形态 `cargo check`（默认） | **Finished，0 error**（4m08s） |
| 内核 lib 单测 | 未单独跑（`cargo test --lib` 需 host-kits 全量重链，受磁盘约束；形态面单测由移动全量套件的 219 lib 例间接覆盖） |
| `bedcode-mobile/src-tauri` `cargo test --no-fail-fast` | **320 passed / 0 failed**（基线 314/0，+6 = 本锁） |
| `bedcode-desktop/src-tauri` `cargo check --lib` | **0 error**，零回归 |
| `bedcode-server-http` `cargo test --lib` | **86 passed / 0 failed**（含新增 jwtAuth 三例） |
| `bedcode-host-api-core` `cargo test` | **38 lib + 2 边界锁 passed / 0 failed**（含新增 fs 7 例） |
| 移动端前端 `pnpm run test:run` | **78 文件 / 895 用例全绿** |
| 桌面端前端 `pnpm run test:run` | **116 文件 / 1624 用例，1 failed** = 在途基线 `terminal-session/.../terminalPreview.test.ts`（resize 首次失败重试的覆盖确认弹窗；HEAD 即红，非本轮引入） |
| 根 `pnpm exec eslint .` | **0 error**（97 warnings） |
| `cross-end-tests` | **未跑（用户本轮指示不跑）** |
| 锁变异自检 | **4/4**（见 §1） |
| 真机冒烟（通知 / 终端流 / SAF 落盘 / 配对） | **未跑**：无 Android 真机与构建链可用，逐项如实登记 |

**磁盘**：本轮三次触发 100% 满，用 `cargo clean`（server-libs 5.6G / host-kits 20.2G + 14.2G）
释放，sccache 命中下重编可接受。**手删 target 子目录的两次审批超时**（与既往记录同），
后续同场景直接用 `cargo clean`。

## 5. 遗留（登记，不本轮处理）

1. `cross-end-tests` 未跑（用户指示）；若后续开跑，重点看 `session_http_flow`（会话控制
   HTTP 面，是票 05 的直接 blast radius）。
2. 桌面侧 fs 是否改用 `host-api-core::fs` 另票（其实现体耦合 WSL 桥与任务单元两消费者）。
3. 内核 `cargo test --lib` 双形态未独立跑（受磁盘约束），留待磁盘宽裕时补。
4. 真机冒烟未跑（无设备）。
