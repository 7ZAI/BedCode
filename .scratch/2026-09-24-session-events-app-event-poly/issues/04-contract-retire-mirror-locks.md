# 04: 删镜像 + 防回接锁 + 全量回归（contract）

**What to build:** 宿主事件路径上**不再存在**会话业务枚举与事件镜像：`DesktopSyncEvent`、旧 `From`、Handler 业务 match、宿主本地 wire 定义全部删除；防回接锁保证后续 PR 不能把镜像加回来。文档（code-map、ADR 0022 H2 措辞）与全量门禁收口——本专项 done。

**Blocked by:** 03（生产路径已全部走 publish/瘦 Handler）.

**Status:** 代码与文档已落地；**宿主 `cargo test` 门禁未跑**（被同 worktree 另一场会话在
`wasm_core/host_api/**` 的在途 trait 化改造挡住，见末节）。**本票不算 done**——门禁实跑后
再改状态，命令与判据已在末节备好。

- [x] 删除 `events/sync_event.rs` 的 `DesktopSyncEvent` 与 `From<SyncEvent>`；删除 Handler 残留业务分支与宿主本地被 01 re-export 替代的定义/死代码
- [x] D7：清点 `Message::SessionEvent`（`session_event` 构造器）生产调用方；仅测试使用则退役构造器或标注遗留并移出会话事件面
- [x] 防回接锁：宿主 `events/**` / `enums/**` 不得再定义会话业务枚举或 `DesktopSyncEvent` 同类镜像（编译期探针或 grep 锁，命名对齐 `retired_kernel_session_domain_is_not_reintroduced` 先例）
- [x] 更新 `bedcode-desktop/docs/code-map.md` 全局事件系统一节、ADR 0022 H2 相关表述（形状定义在 SDK、宿主持 Message 枚举不解语义）
- [ ] 全量门禁：`cd bedcode-desktop/src-tauri && cargo test`；`cd bedcode-desktop && pnpm run test:run`；根目录 `pnpm exec eslint .` 0 error
      —— 前端两条**已跑并绿**（见第六节），宿主一条**待跑**
- [ ] 测试后清理残留进程/端口（AGENTS §3）—— 同上，随宿主门禁一起做

---

## 实施记录（2026-09-24）

### 一、删除面

| 删除 | 内容 |
| --- | --- |
| `src/events/sync_event.rs`（整文件，`git rm`） | `DesktopSyncEvent` 八个变体的镜像枚举、穷尽 `From<SyncEvent>`、双轨期的 `to_sync_payload → None` 说明、`status_from_wire()` 兜底 |
| `src/events.rs` | `pub mod sync_event;` 与 `pub use sync_event::DesktopSyncEvent;` |
| `server/websocket/message.rs` | `Message::SessionEvent` 变体、`Message::session_event()` 构造器、`message_id` / `message_type` / `expect_response` / `token` / `with_token` 五处分支、六处测试引用（含专测构造器的用例）；`use crate::enums::summary::SessionSummary;` 随之外提失效——**移进 `mod tests`**（退役后会话摘要形状只剩测试样本 `sample_session()` 在用，留在文件头就是非 test 构建下的 unused import） |

删除后 `grep -rn DesktopSyncEvent src/ tests/` 只剩三处**记账性**出现：
`events.rs` 锁的注释、`host_sync_event.rs` 的历史叙述、`sync_handler.rs` 锁的 marker 串。
生产引用面为零（票 03 已清零，本票删的是类型本体）。

### 二、D7 取证（`Message::SessionEvent` 为什么可以删而不是标注遗留）

1. **桌面现状**：`src-tauri/src` + `tests/` 内 `session_event(` 只出现在 `message.rs`
   自身的定义与测试里；宿主 WS 层无该类型的路由/处理分支（`grep SessionEvent src/server/`
   除 message.rs 外零命中）——即历史上收到也只是解析出来后无人处理。
2. **移动端现状**：只有两处，且都在它自己那份 `Message` 的**入站路由键表**与
   该表的测试样本里（`bedcode-mobile/src-tauri/src/router/registry.rs:38,170`）。
   移动端从不构造该帧出站。
3. **历史**：`git log -S "Message::session_event(" --all` 在桌面侧命中 3 个提交
   （两个 release 快照 + 一个「协议/认证/插件宿主测试覆盖补齐」），
   即该构造器**自诞生只被测试调用过**，没有生产发送方存在过。
4. **前端**：两端 `src/**` 的 TS/Vue 里 `session_event` 零命中（唯一命中是一条提到
   同名 Rust 文件名的注释）。

据此选择「退役」而不是「标注遗留」。

**风险记账**：删变体同时也删掉宿主的**入站解析能力**——若某个已分发出去的旧移动端
真的发过 `session_event` 帧，新宿主会在 `Message::from_json` 处报错而非忽略。
上面 1–3 条不支持这种可能存在（无任何生产发送点），且 §9 的「老端忽略未知字段」
是字段级增量原则，不承担删除整个消息类型的义务。若日后出现该形态的兼容需求，
**恢复路径是插件同步面**（`SyncPayload::session_*`），不是将变体加回宿主 `Message`。

### 三、防回接锁（两把，都在宿主 `cargo test` 门禁内）

1. `src/events.rs::tests::retired_session_event_mirror_is_not_reintroduced`
   （命名对齐 `retired_kernel_session_domain_is_not_reintroduced` 先例）——
   扫 `src/events/**` 与 `src/enums/**` 的**实现段**（`#[cfg(test)]` 之前）四格：
   ① 再定义名字含 `SyncEvent` 的枚举；② 再引用镜像类型名字面量；
   ③ 在 `src/events/**` 里按变体构造 `SyncPayload::…`；
   ④ 在 `src/events/**` 里出现 `SessionStatus`（宿主重新折业务枚举取值）。
   两个工程细节：**needle 用数组拼接构造**（否则锁文件自己会被自己扫成违规），
   以及**防空转断言**（扫到的源文件数 < 8 即红——目录被改名/清空时锁不得静默通过）。
2. `src/events/sync_handler.rs::tests::sync_handler_does_not_interpret_session_variants`
   （票 03 落）——处理器实现段零变体分支。

**变异自检**：把 HEAD 版 `events/sync_event.rs` 喂给同一套判据 → 命中 12 处
（`pub enum DesktopSyncEvent` / `impl AppEvent for DesktopSyncEvent` /
`impl From<…events::SyncEvent> for Desktop…` / 各 `DesktopSyncEvent::变体`），
现网扫描面 11 个文件命中 0 处。判据成立且非恒真。

### 四、文档收口

| 文档 | 变更 |
| --- | --- |
| `bedcode-desktop/docs/code-map.md` | 目录树 `enums/` / `events/` 两行注（改为「四个文件是 SDK re-export 垫片」+「统一 publish 入口 / HostSyncEvent 薄适配」）；protocol 分界节新增「跨端 wire 真源在 SDK `bedcode-plugin-api::wire`」段；**「全局事件系统」整节重写**（三个文件的职责、`publish` 的校验→查源→投递顺序、`to_sync_payload` 无默认实现的理由、插件事件的一条路、WIT 无返回值 → 失败可见点在宿主 `error!`、三条防回接锁点名） |
| `docs/adr/0022-plugin-host-interface-primitive-boundary.md` | 新增「会话事件面与线协议真源（专项票 01–04，ABI 不变）」节（八条：SDK 真源 / AppEvent 发送协议 / 两跳合一 wire 及 `source_device` 例外 / 薄适配与瘦处理器 / 镜像与遗留面退役 / **H2′ 口径修订** / 移动端零改动与双轨实测 / CI 补口）；上文「WS 动作词表声明式化」第 4 点（H2）就地改措辞并指向 H2′ |
| `docs/knowledge/plugin-development-checklist.md` | 新增「同步事件面」检查项（硬约束同级）：一份 wire 的两个形状名与例外、概要字段用 `wire::SessionSummary` 且 `status` 是展示字符串、禁止宿主侧镜像与逐变体搬运（点名两把锁）、WIT 无返回值所以自足性必须在发布点之前、旧产物解析期点名拒绝 |
| `CHANGELOG.md` / `CHANGELOG_zh.md` | 「基础建设」新增一条（wire 合一 + ABI 不 bump 但第三方插件须重建 + SDK 成跨端真源 + CI 补口）；「改进 → 桌面端」新增一条（AppEvent 发送协议 / 处理器瘦身 / 镜像与 `Message::SessionEvent` 退役 / 两把锁） |
| `bedcode-mobile/docs/code-map.md` | 两处过期描述更正：链路图里的 `Desktop PluginManager → DesktopSyncEvent` 改为新链路；模式切换端点的插件 id `com.bedcode.auto-task` → `com.bedcode.terminal-session`（后者在 v8 批次已合并退役） |
| 顺带修掉的正被踩到的陈旧描述 | 桌面 code-map「自动化任务执行机制」节的「生命周期扩展 / 输入扩展点经 `SessionLifecycleListener` / `SessionInputListener`」——两个回调随 v27 `terminal-hooks` 已退役，编排改在插件自己的 `launch::run_creating_integration` / `session::input_line`；同节「涉及目录」仍写着票 11 删掉的 `src-tauri/src/session/`，一并按实际更正 |

### 五、票 01–04 的终态事实面（供后续会话一眼接手）

- 会话事件的**形状与解释**：`bedcode-plugin-api::events::SyncEvent`（插件产出口）
  = `bedcode_plugin_api::wire::SyncPayload`（出站）；两把 SDK 同构锁 + 标签集合锁钉住。
- 宿主事件面只剩四件：`app_event.rs`（trait + `publish`）、`matcher.rs`（TypeId 分发）、
  `host_sync_event.rs`（唯一薄适配）、`sync_handler.rs`（折载荷 + 排除 + 广播）。
- 宿主 `enums/` 只剩两个自有定义：`auth`（认证 wire）与 `pty_status`（引擎枚举），
  其余四个是 re-export 垫片；`enums.rs` 的类型身份锁 + 垫片零定义锁守着不双份。
- 会话变更通知的**唯一**出站面是 `SyncPayload::session_*`（由
  `com.bedcode.terminal-session` 发布）；`Message::SessionEvent` 已不存在。

### 六、门禁

**已跑并绿（本票）**

| 门禁 | 结果 |
| --- | --- |
| 根目录 `pnpm exec eslint .` | **0 error**（120 warning，按 AGENTS §10 不计入门禁），exit 0 |
| `bedcode-desktop` 前端全量（forks 池跑法：`NODE_OPTIONS=--max-old-space-size=4096 pnpm exec vitest run --pool=forks --maxWorkers=2`） | **82 files / 804 tests 全绿**，exit 0（本专项零前端改动，此跑为票面要求的全量回归） |
| 宿主 `cargo check --lib` | **本票开工初一度通过**（删除镜像与 D7 退役后生产面自洽）；随后对侧的 trait 化改造落盘，同一命令当前报 151 error，**全部落在 `wasm_core/host_api/*`**（非本专项文件），故本票不声称此刻可复现 |
| 新增两把锁的判据模拟 | 现网 0 违规 / HEAD 镜像文件 12 违规（变异自检，见第三节） |
| 全票改动的语法自检 | 对本专项改过的 `.rs` 逐个跑 `rustfmt --edition 2021 --emit stdout`（只看解析，不写回）：**首轮零 parse error**（唯一提示是已删除的 `events/sync_event.rs` 文件不存在）。随后复跑时 `wasm_core/**` 一批文件报 `unknown start of token`——与本专项无关，是同 worktree 对侧的 trait 化改造正在写盘（此刻 `host_api/events.rs` 的 `emit_event` / `broadcast_sync` / `notify` 签名已被改成 `&dyn AppHandleScope` + `&dyn PermissionScope` 形态，函数**体内**我的 `publish(HostSyncEvent)` 逻辑原样保留）。事件面与 enums 面的自有文件（`events/*.rs`、`enums*`、SDK `wire/*`、插件三个文件）两轮都无 parse 报错 |

**待跑（被对侧在途改动挡住，非本专项问题）**

宿主 `cargo test`（lib + 7 个集成 target）当前**无法编译**：同 worktree 另一场会话正在把
`wasm_core/host_api/**` 的上下文与权限判定 trait 化（实测期间出现
`BusScope` / `StorageScope` / `AppHandleScope` / `PermissionScope` / `CapabilityScope`
等 trait 边界迁移与「takes 4 arguments but 3 supplied」的签名变更，错误数在
1 ↔ 1657 之间随其写盘节奏波动），其中一处签名变更同时打到
`wasm_core/host_api/events.rs` 的**既有**测试调用点（`emit_event(&ctx, …)` 一类的旧形态，
非本专项引入的调用）。**未碰对侧文件、未代改签名**
（AGENTS §11 在途改动规范 + 项目记忆「并发会话在途改动」）。

另需说明：本票期间该文件里出现过一个**非我写入**的重复形参（`config.rs:79` 的
`key` / `default` 各出现两次）——已按 §11 归因给对侧（该文件在我开工前就在其
`git status` 变更清单里），未代改。

对侧落地后按顺序补跑并回填第六节：

```bash
cd bedcode-desktop/src-tauri && cargo test          # lib + 全部集成 target
# 之后：ps / ss 核残留进程与端口（AGENTS §3）
```

票 03 那轮已实跑过同一批 target（宿主 lib 1044/0、7 target 全绿、`[skip]` 计数 0）；
本票的增量是**纯删除 + 测试面重写 + 文档**，风险集中在「删掉的类型是否还有引用」
（`cargo check --lib` 与 `grep` 已覆盖）与「新锁是否恒真」（已变异自检）。

### 七、遗留与后续项（本专项不做）

1. **SDK 版本号**：`bedcode-plugin-api` 仍 0.1.1，但 `SyncEvent` 是破坏性契约变更
   （字段类型 + JSON 形状）。是否 bump（0.1.x → 0.2.0）与 npm/cargo 同步发布
   属发布决策，见 `docs/knowledge/sdk-publish.yml` 的版本一致性检查——待用户裁定。
2. **移动端 SDK 门禁对称**：本票只给桌面 SDK 补了 `cargo test` 步骤，
   `plugin-sdk-mobile` 的同一空档未补。
3. **`KeyCombo` 的第三份拷贝**：插件 `keys.rs` 自带一份 KeyCode/转义规则（票 06 时的
   「宿主不代译」产物）。现在 SDK `wire::key` 是真源，插件可改吃 SDK 定义去掉这份拷贝；
   宿主 `pty_process::send_special_key` 也仍在调 re-export 后的 `to_pty_bytes`
   （票面明确「引擎路径不迁」）。已在 `enums/special_key.rs` 注释里记账。
4. **`SyncEvent::SessionStatusChanged` 是纸面变体**：形状两侧齐全、双轨对照过，
   但插件从不广播它（宿主已无状态机）。要真用起来得由插件状态机迁移点发布，
   属会话功能线而非本专项。
5. **`wire_shim_files_contain_no_definitions` 的已知取舍**：按行首词判定义，
   缩进的 `fn ` 会被放行。刻意如此——否则 SDK 内部结构体（如 `SessionRecord`）
   的合法访问器也会撞锁；要收紧就改成「非空行必须是 pub use / 属性 / `}` / `{` / 元组条目」
   的白名单形态，并同步核 SDK 侧用例。
