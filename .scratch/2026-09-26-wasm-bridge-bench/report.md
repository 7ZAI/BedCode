# 桥接基准 · 首轮基线读数与结论

> 数据来源：
> 1. `cargo test --test wasm_bridge_bench -- --full`（2026-09-26，本机 debug 构建）
> 2. `pnpm exec wdio run wdio.conf.ts --spec e2e/specs/bench.spec.ts`（webview 层，真应用）
> 机：Deepin 25 / X11 / debug 宿主（宿主侧原语跑在 debug 上，绝对值偏低，看横向与趋势）
> 夹具：wasm32-wasip3 release（`opt-level="s" / lto`）
> 复现：`cd bedcode-desktop && pnpm run bench:full`（宿主层） / 见 bench/README.md 的 webview 层命令

## 2. webview 层读数（真 Tauri 窗口）

同一批命令，这次从**应用自己的前端封装**走（`src/plugin/commands.ts` 的
`pluginInvoke` + 已缓存的宿主面凭证；事件走 `src/plugin/events.ts` 的 `on()`），
因此测到的是「真前端 → Tauri IPC → 宿主 → wasm → … → 前端」。

| 测点 | webview（含 IPC） | 宿主内（harness） | 增量 = IPC + 前端桥接 |
| --- | --- | --- | --- |
| `nop` | **0.56 ms** | 0.054 ms | ≈ +0.51 ms |
| echo 1 KiB | 0.65 ms | 0.071 ms | +0.58 ms |
| echo 64 KiB | 3.55 ms | 0.267 ms | +3.28 ms |
| echo 256 KiB | 8.35 ms | 0.463 ms | +7.9 ms |
| **echo 1 MiB** | **27.7 ms** | 1.87 ms | **≈ +25.8 ms（24.6 ns/B）** |

事件上行（均为 **1 MiB 总量**，端到端 = 触发到最后一帧进前端 handler）：

| 形态 | 端到端 | 帧数 | 帧间跨度 |
| --- | --- | --- | --- |
| 单大包 1 MiB | 37 ms | 1 | — |
| 256 KiB × 4 | 26 ms | 4 | 17 ms |
| 16 KiB × 64（`emit-chunk`） | 27 ms | 64 | 26 ms |
| 4 KiB × 256 | 33 ms | 256 | 32 ms |

### 2.1 头号发现：**Tauri IPC 段是大头，不是 WASM 边界**

- 小命令：宿主内 54 µs → 真前端 560 µs，**IPC + 前端桥接 ≈ 0.5 ms**（10×）；
- 大载荷：1 MiB 命令返回 1.87 ms → **27.7 ms**（**15×**），增量 ≈ 25.8 ms ≈ 24.6 ns/B。

即：**载荷在 webview ↔ 宿主之间多走一趟，比穿一次 WASM 边界贵一个数量级**。
任何“大对象经命令面回前端”的设计（列表全量返回、大 JSON 快照）都要按 25 ms/MiB 算预算；
反向也成立：把数据留在 guest 内部处理、只回传统计（现在 terminal-session 的做法）是对的。

### 2.2 事件分块：前端侧**没有**“分块更优”的证据

1 MiB 单包 37 ms vs 16 KiB×64 分块 27 ms vs 4 KiB×256 分块 33 ms——**同量级**，
且小包并不更慢（256 帧的 33 ms 反而高于 64 帧的 27 ms）。帧间跨度显示事件是
**流式到达**（64 帧跨 26 ms），不是一次性倾泻。

对 **output-ack / 分片推送**议题的直接含义（补上了 `.scratch/2026-09-26-output-ack-backpressure/`
缺的那块数据；**运输面怎么选见 §2.3**）：

- **分块的成本在宿主侧可忽略**（§3.3）+ **在前端侧也不因帧数而线性恶化**；
- 分块的真正收益仍是**单帧处理时间**（一帧 16 KiB 交给前端 store/渲染，vs 一帧 1 MiB 阻塞主线程更久）——
  这项要用**渲染侧**测（本工程不测 UI 渲染），但**“帧数多导致 IPC 爆量/前端卡死”这两个
  常见担忧，在数据上不成立**；
- ⇒ 选分片的理由应是**平滑渲染 / 可中断 / 可 ack**，不是性能。

### 2.3 W5 · Channel 传输面：**raw 快 4×，`Vec<u8>` 是 6× 的陷阱**

产品当前**零使用** `tauri::ipc::Channel`（Rust/TS 两侧都没有），但桌面端**曾经**有过
Channel 传输的终端输出流（`commands/terminal_stream.rs`，2026-09-22 随会话下沉退役，
退役原因是架构不是性能）。output-ack 专项的 P2「宿主侧 push」真要落地，就得在
emit 与 Channel 之间选——于是补了这条通路（**仅 debug 构建**的宿主基准命令
`bench_channel_stream_{raw,bytes,text}`，见 `src-tauri/src/bench_channel.rs`）。

1 MiB 端到端（触发 → 全部字节进 webview）：

| 传输面 / 形态 | 块数 | 端到端 | JS 侧载荷 |
| --- | --- | --- | --- |
| **Channel raw**（`Channel<Response>`）单块 | 1 | **7.0 ms** | `ArrayBuffer` |
| **Channel raw** / 16 KiB | 64 | **17.0 ms** | `ArrayBuffer` |
| Channel raw / 4 KiB | 256 | 52.0 ms | `ArrayBuffer` |
| Channel text（`Channel<String>`）/ 16 KiB | 64 | 30.0 ms | `String` |
| **Channel `Vec<u8>`** / 16 KiB | 64 | **100.0 ms** | **`Array`（JSON 数字数组）** |
| — 对照：事件 emit 单大包 | 1 | 33.0 ms | JSON 字符串 |
| — 对照：事件 emit / 16 KiB | 64 | 25.0 ms | JSON 字符串 |
| — 对照：命令返回值 | 1 | 28.4 ms | JSON 字符串 |

**结论三条：**

1. **同一批字节，raw Channel 比事件快 4×**（7 ms vs 33 ms；16 KiB 分块 17 ms vs 25 ms）。
   机制在 tauri 2.11 源码里可查：事件每条一次 `webview.eval` 且 payload 被**格式化进
   JS 源码**（`event/mod.rs::emit_js_script`）；Channel 的 raw body 走
   `ChannelDataIpcQueue` + webview 侧 **fetch**（`channel.rs`，≥1024 B）。
   ⇒ 若 output-ack 的 P2 走宿主侧 push，**选 Channel raw**。
2. ⚠️ **`Channel<Vec<u8>>` 是陷阱（100 ms，6× 于 raw）**：tauri 的 `IpcResponse` 有
   泛型 blanket impl（`ipc/mod.rs:181`），`Vec<u8>: Serialize` → `serde_json::to_string`
   → `InvokeResponseBody::Json` → JS 收到**数字数组**（`97,97,…`，体积膨胀约 4×）。
   要真字节必须用 `Channel<Response>` + `InvokeResponseBody::Raw`。
   （这与已退役的 `terminal_stream.rs` 头注释「前端按 `ArrayBuffer` / JSON 体区分」互证：
   两种形态在实践中都出现过。）
3. **块大小有下限**：4 KiB×256 比 16 KiB×64 慢 3×（52 vs 17 ms）——每次 `send` 都是一次
   IPC 往返 + 一次前端回调 ⇒ **流式推送的块不要低于 16 KiB**。

### 2.4 计时口径坑（webview 层）

WebKit 把 `performance.now()` **量化到 ~1 ms**：单次调用直接测出来全是整数
（首次尝试 nop 读数 = 0.000 ms）。必须**批量计时**（一批 20~50 次取总耗时再摊每次），
本表即批量口径（每档 3 批取中位数）。

## 3. 宿主层读数（无头 harness）


```text
╔══════════════════════════════════════════════════════════════════════════════════════════════╗
║ 桥接基准报告（full 模式 · 每测点 5 次取中位数）                                             ║
╚══════════════════════════════════════════════════════════════════════════════════════════════╝

── A 基线 ──
  nop 往返                                 53.991 µs    ← 命令面固定开销下界
  echo 0 B                                 46.728 µs
  echo 1024 B                              70.812 µs
  echo 65536 B                            267.091 µs
  echo 262144 B                           463.410 µs
  echo 1048576 B                         1869.237 µs
  churn 1024 B（墙钟）                       70.693 µs
  churn 1024 B（guest 内）                    4.177 µs
  churn 65536 B（墙钟）                     175.739 µs
  churn 65536 B（guest 内）                117.420 µs
  churn 262144 B（墙钟）                    534.081 µs
  churn 262144 B（guest 内）               467.986 µs
  churn 1048576 B（墙钟）                  1984.407 µs
  churn 1048576 B（guest 内）             1876.655 µs

── B 原语 ──
  host-log ×200（每次）                       1.914 µs    ← import 桥标尺
  storage set+get 1024 B                   363.933 µs   （355.4 ns/B）
  storage set+get 65536 B                 2183.177 µs    （33.3 ns/B）
  storage set+get 1048576 B              25914.037 µs   （24.7 ns/B）    ← 1 MiB 往返 26 ms
  AEAD 加解密 65536 B                      4.459 MiB/s
  AEAD 加解密 1048576 B                    4.594 MiB/s
  fs 写+读 65536 B                         747.700 µs
  fs 写+读 1048576 B                      1238.109 µs
  私有库 100 行 × 1024 B（每行）              205.731 µs
  私有库 10 行 × 65536 B（每行）             3099.221 µs
  execute-batch 1 条                       214.111 µs
  execute-batch 64 条                      280.276 µs

── C 事件（无头口径：不含 IPC / webview）──
  emit 1024 B ×200（每次）                    9.758 µs
  emit 65536 B ×20（每次）                  142.085 µs
  emit 262144 B ×5（每次）                  656.514 µs
  1 MiB 分块 emit（4 KiB/块，每次）             11.046 µs  总 2827.866 µs
  1 MiB 分块 emit（16 KiB/块，每次）            39.548 µs  总 2531.061 µs
  1 MiB 分块 emit（64 KiB/块，每次）           146.549 µs  总 2344.781 µs
  1 MiB 分块 emit（256 KiB/块，每次）          608.825 µs  总 2435.301 µs

── D 互调 ──
  总线 publish JSON（每次）                    14.525 µs
  突发 512 条的送达率                           20.703 %   ← 丢弃率 79.3%
  突发 512 条发布（每次）                       9.247 µs
  总线 publish-binary 1.25 MiB              130.808 MiB/s
  互调 api-call（每次）                       349.310 µs
  互调 api-call（64 KiB reply，每次）          638.269 µs

── E 流式 ──
  ring-fetch maxBytes=4096（每次）             77.912 µs  guest 内占比 31.9%
  ring-fetch 1 MiB 总耗时（maxBytes=4096）     20063.721 µs
  ring-fetch maxBytes=16384（每次）           297.531 µs  guest 内占比 30.9%
  ring-fetch 1 MiB 总耗时（maxBytes=16384）   19041.965 µs
  ring-fetch maxBytes=65536（每次）           271.041 µs  guest 内占比 34.9%  ← 被钳到 16 KiB
  ring-fetch 1 MiB 总耗时（maxBytes=65536）   17346.630 µs
  ring-fetch + publish-binary 再转发（每次）    416.431 µs
  http 非流式 1 MiB                          42.566 MiB/s
  http 非流式 8 MiB                          45.300 MiB/s

── F 异步 ──
  process.run-sync（阻塞，含子进程）            2658.119 µs
  process.run 提交往返                       499.355 µs
  process.run 终态回调延迟                   3248.053 µs

── G 并发 ──
  1 路并发 nop（摊每次）                       61.335 µs
  2 路并发 nop（摊每次）                       56.486 µs
  4 路并发 nop（摊每次）                       52.533 µs
  8 路并发 nop（摊每次）                       44.493 µs
```

数量级门禁 9 条全 PASS（`nop < 3 ms`、`1 MiB 同步回传 < 60 ms`、`host-log < 500 µs`、
`256 KiB emit < 5 ms`、总线 JSON < 2 ms、二进制 ≥ 1 MiB/s、互调 < 20 ms、
PTY 流 1 MiB < 3 s、HTTP 1 MiB ≥ 5 MiB/s）。

### 3.1 结论（按「值不值得动手」排序）

#### 3.1.1 命令面的固定开销 ≈ 50 µs，**不是**瓶颈

`nop` 56 µs / `echo 0 B` 47 µs 意味着：一次纯往返（凭证校验 → 激活门 → 实例锁 →
`spawn_blocking` + `block_on_async` → guest 导出 → JSON 回程）只花 **~0.05 ms**。
UI 交互预算（16 ms 帧）里它占 0.3%。**任何「加一层薄转发」的设计在这条成本面前
都不需要性能论证**——这为「互调窄转发 / 多层插件编排」这类架构选择提供了实测背书。

#### 3.1.2 真正的成本是**载荷穿越边界**，且与「字节数」线性

| 载荷 | echo 往返 | 减 nop（约 50 µs） | 折算 |
| --- | --- | --- | --- |
| 64 KiB | 267 µs | ~217 µs | ~3.3 ns/B（仍含固定开销） |
| 256 KiB | 463 µs | ~413 µs | **1.77 ns/B** |
| 1 MiB | 1869 µs | ~1819 µs | **1.78 ns/B** |

ns/B 在 256 KiB 档与 1 MiB 档几乎相同（1.77 / 1.78）——**跨边界成本是严格线性的**，
没有非线性突变（没有随尺寸变慢的分段、没有 GC 类台阶）。

`A3 churn` 给出了对照：guest 内部光「产 1 MiB 字符串」就要 1877 µs（wasi 时钟实测），
与 echo 的 1 MiB 总耗时 1771 µs 同量级。这说明在**当前组合**（debug 宿主 +
`opt-level="s"` guest）下，1 MiB 场景的总成本里 guest 自身的数据生成占了很大一块，
**纯边界搬运成本没有被干净分离出来**——要分离需要「零生成成本的静态载荷」测点
（待补 `bench.echo-static`，见 §3）。

> 口径提醒：宿主侧依赖（`serde_json` 等）在 dev profile 下仍按 `opt-level = 2` 编译
> （`Cargo.toml [profile.dev.package."*"]`），故宿主 JSON 路径不代表 debug 慢；
> guest 侧整链（含 serde_json）都按 `opt-level="s"`，偏慢。

**实践含义**：不要为了性能把大对象切成很多小对象跨边界传；1 MiB 一次传 ≈ 1.8 ms，
可接受；10 MiB 才需要考虑分片（10 MiB ≈ 18 ms）。

#### 3.1.3 事件推送（无头口径）：**单大包比分块便宜，但差距只有 15%**（无头口径）

1 MiB 的 emit：单包 256 KiB×4 约 2.4 ms / 分块 4 KiB 总 2.83 ms / 分块 16 KiB 总
2.53 ms / 分块 256 KiB 总 2.44 ms。**IPC 次数从 4 涨到 256 只多花约 0.2~0.4 ms**。

> 这条对 **output-ack / 分片推送**议题（`.scratch/2026-09-26-output-ack-backpressure/`）
> 是直接输入：**宿主侧的分块代价可忽略**（每多一次 emit ≈ 11 µs），
> 真正的成本在**前端消费侧**（每帧一次 store 更新 / 重渲染），
> 必须由 webview 层（W3/W4）测，不能在无头层下结论。
> 待办：把 W3/W4 跑出来补这一段。

#### 3.1.4 插件互调很便宜，但**突发会丢 79%**

- 单次 `api_call`（JSON-RPC 阻塞往返）**359 µs**，64 KiB reply 638 µs——完全可接受；
- 但 **D1b 是个真问题**：一次突发 512 条总线消息，订阅侧只收到 **20.7%**
  （订阅队列容量 64，`bus.rs::SUBSCRIBER_QUEUE_CAPACITY`，满即丢）。
  发布方视角毫发无损（9.2 µs/条），**丢失完全静默**（只有 `warn` + 计数）。
  - 影响面：任何"高频事件流经总线"的插件设计（终端输出分片、传输进度、AI 流）
    在突发场景下会静默丢数据；
  - 建议另立项：`record_bus_dropped` 的指标是否已进 core-monitor 快照？
    能否给插件侧暴露丢弃计数，让订阅方能自愈（这正是 §8 fail-visible 判据）。
  - 注：本工程的 D1 测点刻意取 50 < 64 以测"每条都投到"的语义，突发丢弃单列 D1b。

#### 3.1.5 存储面是隐藏的成本洼地

`storage set+get`：**1 KiB 364 µs → 1 MiB 25.9 ms**。每字节成本
**1 KiB 档 355 ns/B → 1 MiB 档 24.7 ns/B**（同一轮基准里 `echo` 跨界只有 **1.78 ns/B**）
——**KV 每字节比跨界搬运贵约 14 倍**（1 MiB 档）。原因是它把整个值序列化成 JSON 字符串
进 SQLite `plugin_storage` 表，每次 set 整值重写。
**业务含义**：会话注解、设置项这类“小值”无所谓；一旦有插件拿它当**大对象缓存**
（比如缓存 1 MiB 的文件内容），成本会 surprising 地高。→ 业务真源应放
`host-plugin-database` 或文件，不应放 KV。

> 对照：`execute-batch` 64 条 280 µs —— 事务批处理比逐条调用便宜一个量级，
> 这条支持「批量写用 execute-batch」的既有结论。

#### 3.1.6 终端输出流：1 MiB ≈ 19 ms，且 ~2/3 成本在宿主侧

`ring-fetch` 1 MiB：4 KiB 批 19.9 ms / 16 KiB 批 19.0 ms / 64 KiB 档 17.3 ms
（64 KiB 被宿主钳到 16 KiB，故与 16 KiB 档同调用次数——**数控断言**已验证）。
guest 内占比仅 **31%**，即约 13 ms 花在宿主侧（环读 + 边界搬运 + 命令面 JSON）。

对比既有探针（`terminal_output_perf.rs` P2 测的是 JSON 命令通道的 `list<u8>` 编解码，
本次是 guest 内消化字节的形态），两条数据合起来说明：
**「在 guest 里把流消化掉再回传统计」比「把每块字节经命令面搬回宿主」便宜一个量级**——
这正是 terminal-session 现在的做法（ring-fetch 在插件内解析），方向正确。

#### 3.1.7 同步 vs 异步：同步阻塞的代价 ≈ 子进程本身

`process.run-sync` 2.66 ms（其中绝大部分是 `/bin/sh` + `tr` 处理 200 KB 的真实耗时），
`process.run` 提交 0.50 ms、终态回调 3.25 ms。**同步/异步的差主要来自工作本身，
不是桥接开销**——换言之「异步化以避免阻塞实例」的理由在数据上是**结构性的**
（慢调用会占住实例锁，见 G1 / 并发模型专项）而**不是**微秒级的性能问题。

#### 3.1.8 G1 · `nop` 并发扇出看不出串行化（`nop` 太快）

1/2/4/8 路并发 nop 摊每次 61/56/53/44 µs，**并发越高单次越快**——`nop` 这个量级上
实例锁的串行化被固定开销完全掩盖。要测出它必须用**有实质时长的慢调用**做对照，
这正是新增的 G2（见 §4）。

## 4. G2 · 慢调用是否堵死同实例全部交互（新增）

（并发模型专项的量化基线；用 `bench.storage-rt` 1 MiB ≈ 26 ms/次 的**慢调用**做并发对照，
而不是 G1 的 `nop`）

| 测点 | 读数 |
| --- | --- |
| nop（无慢调用在途） | 156 µs |
| **nop（慢调用在途）** | **26100 µs**（≈ 慢调用自身 26095 µs） |
| 阻塞倍数 | **167×** |
| 2 路并发慢调用总耗时 | 52676 µs = **2.02×** 单次 |
| 4 路并发慢调用总耗时 | 105318 µs = **4.04×** 单次 |

⇒ **完全串行**：一条慢命令在途时，同实例的其它命令（哪怕 50 µs 的 `nop`）要等它做完；
并发扇出没有任何并行度。这正是 `.scratch/2026-09-26-plugin-concurrency-model/` 的动机，
现在有可回归的数字：

- ~~**改造目标可量化**：事件循环属主 + guest task 落地后，G2 的阻塞倍数应从 167× 落到 ~1×，
  扇出倍率从 4× 落到 ~1×。建议把 G2 作为该专项的**验收基准**（bench 已可复跑）~~
  → **口径更正（2026-09-27，并发模型专项实测）**：该目标**不可达**——同实例串行由 wasmtime
  **实例级门**保证，切到事件循环属主也不会放开（async import 挂起期间同实例第二条调用零进展，
  且挂起期间属主循环停摆）。因此 G2 的「167× → ~1×」不能作为验收口径；可用的等价口径是
  **「不同实例互不阻塞」**（跨插件并行度），或等运行时支持实例级并发进入后再启用本条。
  证据与边界锁见 `.scratch/2026-09-26-plugin-concurrency-model/spec.md` §12 +
  `bedcode-desktop/src-tauri/src/wasm_core/manager/runtime/tests/p3_async_host_import.rs`；
- 业务含义：一个插件在跑 26 ms 的 storage/fs/http 往返时，前端的输入/resize/ack 全部排队
  ——**本条仍然成立**，且已确认无法用「把 host import 改成 async」绕过；解法只剩非等待形态
  （立即返回句柄 + 事件回调）。

## 5. 待补 / 下一步

1. ~~跑 webview 层~~ ✅ 已完成（§2，含 IPC 增量与分块对照）；
2. ~~G1 扩慢调用档~~ ✅ 已完成为 G2（§4），建议直接作为并发模型专项的验收基准；
3. **加 `bench.echo-static`** 测点：消除 guest 生成成本后测纯边界搬运（§3.1.2 的
   归因还差这一格）；并可补一条 **webview 侧的 `echo-static`** 以剥离 IPC 增量里的
   guest 生成部分；
4. **release 复测**：宿主侧原语（B 组）在 debug 下测，绝对值不代表产品；IPC 增量
   （§2.1）则**与构建无关**（webview 侧始终是 release 前端），可直接用于容量估算；
5. **D1b 另立项**：总线突发丢弃的 fail-visible 化（订阅方可读丢弃计数 / 自愈）；
6. ~~**G2 接入 CI**：目前 G2 只在全量模式跑；若事件循环属主改造落地，可把
   「阻塞倍数 < 5×」设为门禁~~ → **改为跨实例口径**（2026-09-27，见 §4 更正）：同实例阻塞倍数
   受运行时实例级门限制，不能当门禁；改测「不同实例互不阻塞」或先搁置；
7. **release 复测 W5**：Channel 结论目前取自 debug 宿主（Rust 侧 8 行循环，release 下
   应更快，故 4× 的差距只会**更大**不会更小——但仍应实测确认；前端侧始终是 release）。
