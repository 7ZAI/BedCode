# 08 — P3：首个按需 async 化 —— `host-http.fetch`（非流式）

**Type:** task
**Spec:** `../spec.md`（§4.3 候选 ① + §5 WIT/ABI/SDK 流程 + §6 阶段 P3）
**Blocked by:** 07（P2 SDK async 路径——guest 侧需要 async lower 支持）；A1 结论（票 01 未通过则本票不启动）
**Status:** **retired（2026-09-27，实测改判）** —— 目标「解除慢 HTTP 堵死同插件其它交互」在 wasmtime 48 上**不可达**：同实例串行由**实例级门**保证，与 host import 是否 async 无关。本票不实施；实测证据与残余方向见 `## 改判记录`。

**What to build（原案，已停）：** 把非流式 `host-http.fetch` 改成 async WIT import + 手写 `func_wrap_concurrent` 注册，解除「慢 HTTP 在 import 栈内等网络 → 堵死该插件全部交互」的结构性尖角（spec §1 后果 1）。改动面最小、收益最直接，也是验证「按需 async 化」整条管线的第一个生产实例。

---

## 改判记录（2026-09-27）

**触发**：A4 门禁（票 04）已判定 WIT 层 `async func` 不可用 ⇒ 原 W1–W6 流程（WIT 标注 + ABI bump + 双端副本 + 手写 `func_wrap_concurrent` + SDK async 路径）整体停摆，路径改判为**宿主实现侧 async**（`func_wrap_async`，WIT/ABI/SDK 零变更，spec §5 前置门禁）。落地前先测「该路径是否能兑现本票的收益承诺」。

**实测（决定性）**：`bedcode-desktop/src-tauri/src/wasm_core/manager/runtime/tests/p3_async_host_import.rs`
的 `test_p3_async_import_pending_second_explicit_call_same_instance`（生产属主形态：常驻
`run_concurrent` + 显式 `start_call_concurrent`，无人工锁；`cargo test --lib test_p3_async_import_pending_second -- --nocapture`）：

```text
[probe] host invoke 进入: mode=wait   at 0ms    (active=0)   ← #1 挂起在宿主 async import
[probe] host invoke 进入: mode=normal at 1201ms (active=0)   ← #2 的宿主实现到此刻才被进入
[probe] B 第二条结算 1201ms：Ok("normal-complete:solo")       ← 外部释放时刻 = 1200ms
```

⇒ **import 挂起期间，同实例第二条显式调用零进展**：连它的宿主 import 都没被进入，直到 #1 被
放行（1200ms）才一口气跑完。同实例串行由 wasmtime 实例级门（单栈激活 / `do_not_enter`）保证，
**不因 host import 变成 async 而放开**——这与 P0 阶段 A2'（发票 02 实测、票 01 复核）同源，本票
只是把它确认到「宿主实现侧 async」这一形态上。

**第二重证据（同轮追加）**：`test_p3_async_import_suspension_stalls_owner_closure` 实测——fiber 挂起
期间**属主闭包完全不被调度**（闭包内 `sleep(20ms)` 到放行时刻 600ms 才完成）⇒ 即便换到 event-loop
模型，async 化也不会给属主循环留下「服务其它请求」的空档（与 mutex 模型持锁等待等价）。

**因此「宿主实现侧 async 化 `fetch`」的收益口径缩水为「等待期间不占住一条宿主执行线程」**：
在 mutex 模型下切换前后几无差别（调用仍持实例锁、阻塞线程照旧）；在 event-loop 模型下确实不再
`block_in_place` 占住 tokio worker，但同插件其它命令**仍要等**，用户可见收益 ≈ 0 ⇒ 按「文档
承诺兑现不了就退役」的既有裁决口径（见 AGENTS §0 / 工作区惯例），**本票退役，不改任何生产代码**。

**保留的资产**：上述实测用例（含断言锁定「被实例级门推迟」）作为**边界锁**留在树里——它不是
永久真理，而是「wasmtime 48 行为」的可执行记录：**若该断言转红**（第二条调用在挂起期间推进了），
说明运行时已支持实例级并发进入，届时本票（或非等待形态的替代方案）需要重新评估。

**可选后续方向（未立项，需用户裁决）**：

1. **非等待形态**（与 `host-process.run` / 流式 `fetch` 同形）：`fetch` 立即返回句柄 + 完成事件回调，
   guest 不再「等一次网络往返」⇒ 天然不堵实例。代价是契约与插件改造（ABI + SDK + 调用方重写），
   属新立项而非本票延伸。
2. **等运行时演进**：instance-level 并发进入可用后，再按 spec §4.1 的 C1–C4 重走一遍判据。

**不变的部分**：`host-http` 的其余两函数（`register-endpoint` / `unregister-endpoint`）与流式
`fetch` 分支本就是「立即返回 + 事件」形态，无需改动。

**现状（已确认）**：`host-http.fetch: func(request-json: string) -> result<option<string>, string>`（`packages/plugin-sdk-desktop/rust/wit/bedcode.wit:275-277`）；宿主实现 `src-tauri/src/wasm_core/host_api/http.rs:223` 在 import 栈内 `block_on_async(execute_http_request(&request))` 等网络往返（时长不受本仓库控制）→ 持实例锁等待。流式分支已在插件侧走「立即返回 stream-id + 事件」，不属于本票。

**原流程（spec §5 W1–W6，随 A4 门禁停摆，留档）：**

1. **W1**：WIT 对 `fetch` 加 `async func` 标注（**函数级**，不是 interface 级；`host-http` 的 `register-endpoint` / `unregister-endpoint` 不动）。→ 停止（A4：async-lifted 导出入口即 abort）。
2. **W2**：**ABI bump**（桌面 31 → 32）——导入签名变更 = 组件契约变更；走 ADR 0019 流程 + 双端评估。→ 停止（路径改判为宿主实现侧 async 后，ABI 零变更；改判后又因收益口径不成立而整票退役）。
3. **W3**：双端 WIT 副本同步。→ 同上，未触发。
4. **W4**：宿主手写 `Linker::func_wrap_concurrent` 注册 `fetch`。→ 停止（F13：concurrent 注册需 WIT `async func` 配对）；宿主实现侧 async 用 `func_wrap_async`（探针已实证），本票实测证明其收益不足以落地。
5. **W5**：guest 侧消费 SDK 的 async 路径（票 07）。→ 票 07 同因 A4 退役。
6. **W6**：CHANGELOG 双语条目 + 旧产物重建提示。→ 未触发（无契约变更）。

**验收：整票退役，改为以下「已完成的退役动作」：**

- [x] 收益承诺前置实测（生产属主形态、无人工锁）：`test_p3_async_import_pending_second_explicit_call_same_instance`
      ⇒ 同实例第二条显式调用在 import 挂起期间零进展（宿主 import 到放行时刻才被进入）。
- [x] 实测固化为边界锁（断言「被实例级门推迟」）+ 探针文件头记录口径；行为变更时该锁转红 = 重评信号。
- [x] 改判写回本票与 spec（§4.3 候选 ① / §6 P2·P3 / §7 风险 A2'）。
- [x] 零生产代码改动（本票不动 WIT / ABI / SDK / 宿主实现）。

**Out of scope:**

- 不动其它任何原语（`host-ws` / `host-peer` / `host-auth` 等在票 09 逐个评估）。
- 不做流式 HTTP 改造（已是正确形态）。
- 不改移动端实现（只在 W3 范围内同步副本）。

## 关联证据

- `packages/plugin-sdk-desktop/rust/wit/bedcode.wit:275-277`（`host-http` interface）、`packages/plugin-sdk-desktop/rust/src/abi.rs`（`ABI_VERSION = 31`）
- `src-tauri/src/wasm_core/host_api/http.rs:223`（`block_on_async(execute_http_request(...))`）与 `runtime_util::block_on_async`
- CM-async spec：F1/F2（`call_concurrent` / `func_wrap_concurrent` 语义）、F6（bindgen! 不生成）、F13（WIT async 标注 ↔ concurrent 注册强制配对）
- ADR 0019（wasmtime 双端锁版 / WIT 副本同步）、ADR 0022（边界）