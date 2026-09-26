# 01: 基线与假插件端点夹具（P0）

**What to build:** 冻结本专项起跑基线，并交付一套**假插件端点夹具**（本地 WS server）——
在不需要真实桌面的前提下复现桌面插件两个端点的帧协议：`session-control`（极简认证 +
7 类业务事件帧）与 `terminal`（text 控制帧 + binary 裸字节输出）。夹具是票 03/04/05 单测
与票 07 集成测试的共同地基。

**Blocked by:** 无（可与票 02 并行）

**Status:** done（2026-09-26 夹具交付；验收全勾，集成测试在票 07 统一运行通过）

## 验收标准

- [x] 基线快照：记录 HEAD commit、`git status --porcelain`（区分本专项文件与在途他人改动）、
      两端依赖版本与 wasmtime 分叉现状（AGENTS §2）。
- [x] 宿主零改动判据固化：`git diff --stat -- bedcode-desktop/src-tauri/src bedcode-desktop/packages/plugin-sdk-desktop`
      期望空输出；命令写入本票与票 07。
- [x] **夹具（Rust）`session-control` 端点**：本地 WS server 可启停 + 随机端口；校验首帧
      `{"type":"auth","token":"<jwt>"}`（可配置接受/拒绝）；可注入 spec §3.1 的 7 类事件帧
      `{"type":"event","event":"<name>","payload":{...}}`；可观察当前客户端连接数。
- [x] **夹具（Rust）`terminal` 端点**：接收 `subscribe`（`sessionId`/`mode`）、`unsubscribe`、
      `ack`、`poll`、`input`（text UTF-8）与 **binary 输入帧**；可回放 `subscribed`、
      `ring_resync`、`session_stopped`、`error` 控制帧与任意裸字节输出（支持按序分片）。
- [x] 夹具帧形状逐字段对齐 spec §3.2/§3.3 帧表（snake_case、输出**裸字节无帧头**、
      无 `from_offset`/`history_end`/per-frame offset）；字段名漂移即测试红。
- [x] 夹具可被 Rust 单测与 `src-tauri/tests/` 集成复用（同一 helper 模块，二选一：
      `tests/support/mock_plugin_ws.rs` + `#[path]` 复用到单测，或 `#[cfg(test)] mod test_support`）；
      前端如需要，提供等价 vitest mock 工厂。
- [x] 夹具自身单测：启停、认证门（接受/拒绝）、帧形状自检、客户端数观测、端口释放；无外部网络依赖。
- [x] 跑测后无残留端口/进程。

## 边界与不做

- 只交付测试支撑，**不改任何生产代码路径**（唯一例外：`src-tauri/src/lib.rs` 加一个
  `#[cfg(test)] #[path]` 模块声明——仅测试构建生效，生产构建零影响，见下方实现决策）。
- 不实现业务语义（不做会话状态机），只做协议壳与可编程应答。
- **本票不运行集成测试**（见 `README.md` 测试节奏）；夹具自检（单测性质）本票即跑。
- **vitest mock 工厂：本票不提供**（判据见下）。

## 实现决策（2026-09-26 实施时定）

1. **复用机制**：夹具唯一宿主 = `src-tauri/tests/support/mock_plugin_ws.rs`（无内嵌测试）；
   集成测试文件 `#[path = "support/mock_plugin_ws.rs"] mod mock_plugin_ws;` 引入；
   crate 内单测经 `src-tauri/src/lib.rs` 的 `#[cfg(test)] #[path = "../tests/support/mock_plugin_ws.rs"]
   pub(crate) mod mock_plugin_ws;` 复用（票 03/05 单测将 `crate::mock_plugin_ws::…`）。
2. **单监听双路径**：一个 TcpListener（127.0.0.1:0）按请求路径路由到 `session-control` / `terminal`
   两个端点（与真实桌面同 host:port 双端点一致）；未知路径握手后立即 close 1008。
3. **认证门**（两端点一致，对齐 survey §握手：坏 token / 未认证 → close 4001）：
   首帧必须为 `{"type":"auth","token":"<jwt>"}`；策略 `RequireAnyToken`（默认）/ `RequireToken(t)` /
   `RejectAll`；非 auth 首帧或拒绝 → close 4001（帧先记录后判，拒绝场景仍可断言收到 auth 帧）。
4. **帧形状锚定桌面 wire**（`wasm-apps/terminal-session/rust/src/ws_{control,terminal}.rs`）：
   `subscribed`/`session_stopped` 用 `sessionId`/`exitCode`（camelCase，两端协议事实）；
   事件帧 `{"type":"event","event":"<name>","payload":{...}}` 顶层仅 type/event/payload；
   二进制输出=裸字节（无帧头、无 per-frame offset）。「字段名漂移即测试红」由自检用例
   `fixture_frame_shapes_match_spec` 逐字段锁（含禁令字段 `from_offset`/`history_end`）。
5. **vitest mock 工厂不提供**：移动端前端测试不直连 WS（经 Tauri command/bridge 层 mock），
   WS 协议形状验证属 Rust 层；若票 03/05 单测需要等价工厂，由对应票自行评估补充（本票不阻塞）。
6. **分片语义**：`send_binary` 一次调用 = 一条 WS binary 帧；连续调用按序送达（sink 串行），
   模拟插件 ring-fetch 多片输出；测试端按到达序逐片断言。

## 基线快照（2026-09-26 冻结，实施前记录）

- **HEAD**: `9767066ac` `chore(desktop): 清理会话下沉/文件服务退役后的遗留依赖`（dev 分支）
- **本专项在途文件**（`.scratch/2026-09-26-mobile-desktop-adaptation/`）：全部未提交（`??`），与实施代码互不干扰
- **他人/其他任务在途改动**（`git status --porcelain` 其余 23 项，本次不触碰）：
  - `bedcode-desktop/src-tauri/src/**` 6 项（wasm 应用二次启用 context 修复专项，并发 agent 在途）
  - `bedcode-desktop/scripts/dev-run.js`、`bedcode-desktop/wasm-apps/{ai-chatbox,terminal-session}/**` 前端与 plugin.json
  - `.scratch/2026-09-25-plugin-reactivation-stale-context/fix.md` 等文档
  - **移动端 `bedcode-mobile/**` 零在途改动**（本专项实施时移动端工作区干净）
- **两端依赖版本**（AGENTS §2）：桌面/移动 `package.json` 与 `src-tauri/Cargo.toml` 均为 **2.1.1**；pnpm 12.2.1；Node v24.20.0；rustc/cargo 1.98.1（stable 2026-09-01）
- **wasmtime 分叉现状**（ADR 0019）：桌面 **48.0.x**（LTS 先行）vs 移动 **47**（临时分叉，`.scratch/2026-09-18-wasmtime-48-upgrade/spec.md`）——本专项不涉及 wasmtime，仅记录

## 验证（单测）

```bash
# Rust 夹具（按实际测试目标名过滤）
cd bedcode-mobile/src-tauri && cargo test mock_plugin_ws
# 前端 mock 工厂（若提供；本票未提供，见边界与不做）
cd bedcode-mobile && pnpm exec vitest run <mock 工厂测试文件>

# 宿主零改动判据（AGENTS §0.1 硬约束；收尾时要求空输出，随包产物 resources/plugins 除外）
git diff --stat -- bedcode-desktop/src-tauri/src bedcode-desktop/packages/plugin-sdk-desktop
```

## 集成测试（待票 07 运行）

- 无（本票产出的是夹具，供 03/04/05/07 使用）；夹具自检属单测。
- 本票新增的测试目标 `tests/mock_plugin_ws_fixture.rs` 只含夹具自检（不经移动端业务代码），
  属单测性质、本票即跑；不构成「集成测试提前运行」。

## Comments

- 2026-09-26：实施交付。夹具 `tests/support/mock_plugin_ws.rs`（589 行）+ 自检 `tests/mock_plugin_ws_fixture.rs`
  （469 行，14 用例）+ `lib.rs` cfg(test) 复用钩子（+9 行，生产构建零影响）。
  基线快照与实现决策登记上文。验证输出摘录：
  ```
  cd bedcode-mobile/src-tauri && cargo test mock_plugin_ws
  test result: ok. 14 passed; 0 failed（夹具自检，含 lib test target 336 例全绿过滤）
  cargo check --lib  无告警脱落；ss 无残留监听；pgrep 无残留测试进程
  ```
  实施中排掉 3 类问题：① async fn 内 Drop 类型字段不可 move（E0509）→ task 改 Option +
  `mem::swap` 取出；② `blocking_lock` 在 tokio runtime 内 panic → 访问器改 async；
  ③ 连接任务异步处理帧导致读侧竞态 → 测试侧轮询等齐（`wait_for_recorded_text`）。
  另修一个真实逻辑 bug：认证通过后未置 `authenticated`，后续帧会被误关——被自检用例（`rust-clippy`
  warning: variable does not need to be mutable ）当场抓住后修复。
