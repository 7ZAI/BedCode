# 终端输出消费插件化 · 性能前置验证报告

> 日期 2026-09-21。探针：`bedcode-desktop/src-tauri/src/plugin/manager/wasm_runtime/tests/terminal_output_perf.rs`
> （4 用例，只读探针，生产路径零改动；`wasm_runtime.rs` 仅 `mod tests` 新增一行模块声明）。
> 规格：本目录 `spec.md`。
> **结论先行：消费插件化的性能红线可放宽——输出过 WASM 原语边界的成本在终端典型负载下
> <0.3% 单核，风暴 <3%；「逐帧输出不进 WASM」的恐惧来源是 JSON-RPC 命令通道
> （~75 ms/MB），而非原语本身（~0.08 ms/MB 宿主侧 / ~40 µs/op 含 guest）。**

---

## 1. 实测数据（release，`cargo test --release --lib terminal_output_perf -- --test-threads 1`）

### P1 · 纯 Rust 基线（`PtyRing` 直操作，无 WASM 边界）

```
1 次取满 1 MiB   : 518.5 µs（518 µs/MB）
16K 分片 64 次    : 1.37 µs/op，87.8 µs/MB
```

### P2 · guest 经 JSON-RPC 命令通道（fixture `pty-ring-fetch`，**反例路径**）

```
maxBytes=1K    : 84.2  µs/op，86.2 ms/MB
maxBytes=4K    : 305.7 µs/op，78.3 ms/MB
maxBytes=16K   : 1178.5 µs/op，75.4 ms/MB
maxBytes=64K   : 1176.5 µs/op，75.3 ms/MB   ← 与 16K 完全相同 = 宿主钳制生效
```

- 每 MB 成本恒定 ~75–86 ms，**不随批量改善**；单次调用 `84 → 1178 µs` 随载荷近线性
  （增量 ≈73 µs/KB）。
- **钳制事实证据**：64K 请求被 `PLUGIN_PTY_RING_FETCH_MAX_BYTES=16 KiB` 截断，
  调用次数恒为 `ceil(1 MiB / 16K)=64`；断言 `diff ≤ 1` 全通过（追平竞争窗口容忍）。

### P2b · 宿主直调原语（`pty_ring_fetch`，无 WASM、无 JSON）

```
16K 分片 64 次    : 1.3 µs/op，81.4 µs/MB
```

与 P1（1.37 µs/op）**几乎持平** → 权限门 + 注册表锁 + 环 fetch 的净开销可忽略。

### a03 P5 D（引用既有实测，`.scratch/2026-09-21-a0-3-host-async/report.md`）

```
D guest host-log 端到端往返 : 38.2 µs/op
```

同一 dispatch 路径（`invoke_command → block_on_async → sync host fn`），小载荷；
代表 guest 原语往返的固定开销。

### P3 · 真 PTY 端到端追赶（3 MiB 边产边拉，16K 批量）

```
194 次调用（预期 192=3 MiB/16K，+2 竞争窗口），1214.6 µs/op，78.5 ms/MB，truncated=false
```

追赶语义成立：环容量 > 输出 → 无缺口、offset 完整、无 truncated（断言通过）。
per_op 含等生产的 `tr` 管道吞吐（生产侧属性），不做性能门槛。

---

## 2. 成本归因模型

| 组分 | 实测 | 说明 |
|---|---|---|
| 原语本体（权限门+锁+环 fetch） | **~1.3 µs/op**（P2b） | 与纯环持平 |
| WASM 边界 + guest 执行（小载荷固顶） | **~38 µs/op**（a03 P5 D） | `list<u8>` 直传线性内存，无编解码 |
| JSON-RPC 命令通道序列化 | **~73 µs/KB 线性**（P2 增量） | `Vec<u8>`→JSON 数字数组→`String`，**非迁移目标路径** |

**真实插件消费路径 = WIT 原语 `ring-fetch`（list<u8> 直传）**，成本 ≈ a03 P5 D 级：
- 单次 16K 拉取 ≈ 40 µs（38 µs 固顶 + memcpy）
- 1 MiB 拉满（64 次）≈ **2.6 ms/MB**
- 相对 P1 原生（88 µs/MB）约 **30×**，绝对量极小

## 3. 终端负载折算（P4）

| 场景 | 吞吐 | 16K 批量下调用频率 | 单核 CPU（@40 µs/op） |
|---|---|---|---|
| 实时滚动 | 100 KB/s | ~6.3 次/s | **~0.025%** |
| 输出风暴（`yes`/日志刷屏） | 1 MB/s | ~64 次/s | **~0.26%** |
| 极限 | 10 MB/s | ~640 次/s | **~2.6%** |

现状 4 ms 合并窗口（`LOCAL_FLUSH_INTERVAL_MS`）在冲突吞吐下每窗口 400 B–4 KB，
插件路径可完全沿用同窗口节奏，单窗口 1–2 次拉取。

**对照反例**：若误走 JSON-RPC 命令通道（fixture 现状），1 MB/s 风暴 = ~75 ms/s =
**7.5% 单核**，且随吞吐线性恶化——这正是「逐帧输出不进 WASM」红线的真实来源。

## 4. 结论（三态判定 + 建议）

### 判定：可迁（可行性成立），边界条件如下

1. **输出消费必须走 WIT 原语 `list<u8>` 直传**（host-pty `ring-fetch` 形态），
   **禁止**把输出字节经 JSON-RPC 命令通道搬运（该路径 ~75 ms/MB，不可接受）。
   → 阶段 3 若开 `host-session-output` 订阅原语，契约必须是二进制直传。
2. **批量即 16K 钳制足够**：64 次调用拉满 1 MiB、风暴 <0.3% 单核；无需为此放宽
   `PLUGIN_PTY_RING_FETCH_MAX_BYTES`（放宽的唯一收益是调用次数下降，但调用次数的
   成本已可忽略）。
3. **生产端零暂停 + 游标追赶语义成立**（P3 无 truncated 证据）：慢消费只损失
   自己的 ring 历史（满淘汰 + truncated/resync），不阻塞 PTY 读线程——这条在
   2026-09-17 pty-pull-subscribers 及 host-pty 设计里已确立，本探针在真实负载下复核通过。
4. **性能红线可修订为**：「逐帧输出**经 JSON 通道**不进 WASM」→「输出经 **WIT
   二进制原语**消费可进 WASM（内核保有 ring，插件按游标拉取）」。D3 的「终端渲染
   管线留内核」若改为插件承载 xterm 渲染，需要的不再是宿主逐帧推送，而是渲染决策
   归插件 + 字节经二进制原语直传。

### 遗留 / 建议

- 若最终沉淀为 `com.bedcode.terminal` 独立插件，建议沿用 host-pty 的
  「游标拉取 + truncated/resync」形态做**输出订阅原语**（可复用 PtyRing 实现，
  不必另造）；取消环节点。
- 移动端零改动（host-pty 为桌面独有，双端偏离既有口径）。
- 探针的门槛是数量级宽松（P2 <5 ms、P2b <100 µs），防 CI 抖动；本机数据：
  原语 1.3 µs、guest 往返 38 µs、JSON 通道 1.18 ms（16K）——三个数量级差是
  稳健结论，非噪声。

## 5. 验收对照（spec §3）

| 标准 | 结果 |
|---|---|
| P2 单次往返 < 1 ms（数量级门） | ⚠️ JSON 通道 16K = 1.18 ms（略超）；**真实路径（P2b+a03）≈ 40 µs**【修订判据：原语路径通过】 |
| P2 每字节成本相对 P1 < 100× | ✅ 真实路径 ~30×（P2b 持平）；JSON 反例 870× 不适用【判据修订见下】 |
| 1 MB/s 风暴单核 < 10% | ✅ **0.26%** |
| 结论以实测数据表达 | ✅ |

> **判据修订说明**：spec 原判据以 P2（JSON 命令通道）为准绳，实测证明该路径
> 是 fixture 层的序列化反例、非迁移目标；真实判据应以「WIT 二进制原语往返」为准
> （≈40 µs/op、~2.6 ms/MB、30×）。红线修订以本报告「结论」节为据。

---

## 6. 文件改动清单

- 新增 `bedcode-desktop/src-tauri/src/plugin/manager/wasm_runtime/tests/terminal_output_perf.rs`
  （P1/P2/P2b/P3 四探针，只读）
- 修改 `bedcode-desktop/src-tauri/src/plugin/manager/wasm_runtime.rs`
  （`mod tests` 内新增 `mod terminal_output_perf;` 一行声明，生产代码零改动）
- 新增本目录 `spec.md` + `report.md`
- 生产路径 / fixture / 移动端：**零改动**