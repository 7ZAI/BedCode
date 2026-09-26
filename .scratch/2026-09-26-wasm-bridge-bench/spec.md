# 桥接基准工程（wasm ↔ 宿主 ↔ 前端）

> 立项：2026-09-26 · 状态：**已交付并全部实跑**（宿主层 22 场景全绿 + webview 层 2 场景全绿）

## 1. 目标与动机

桌面端是「无业务内核 + wasm 应用」（ADR 0022）：一切产品事实都在 wasm 应用里，
宿主只提供引擎原语（PTY / HTTP / WS / 总线 / 存储 / 加密…）。这带来一个此前
**没有实测数据支撑**的问题：

> 一次「用户点一下 → 前端 invoke → 宿主 → wasm → 宿主原语 → wasm → 事件 → 前端」
> 的往返，**成本分布**在哪里？哪些环节是数量级瓶颈？

已有的性能探针（`.scratch/2026-09-21-terminal-output-consumer-perf/`、
`runtime/tests/ws_output_perf.rs`）只覆盖**单条链路的一段**（PTY 输出消费、WS 帧吞吐），
缺两块：

1. **横向**：没有覆盖 wasm↔wasm（总线 / 互调）、同步 vs 异步形态、大载荷分片；
2. **纵向**：没有覆盖 Tauri IPC 与前端派发（既有探针全部无头运行）。

本工程补这两块，且**不新增任何产品代码**：被测端是测试夹具（`packages/plugin-bench-test`），
驱动端是 `cargo test` 的一个自跑 target。

## 2. 边界与非目标

| 项 | 决定 |
| --- | --- |
| 放宿主还是放插件 | **都不放**。夹具是测试产物（`packages/plugin-bench-test`，不进 `wasm-apps/`、不进打包链）；harness 是 `cargo test` target（不进产品构建） |
| 是否改生产路径 | 否。harness 只**调用**公开 API（`PluginHost::new` / `activate_plugin` / `invoke_rust_command`），不改一行宿主代码 |
| 是否装进用户应用 | 仅 e2e 层会，且走生产 zip 安装路径并在 `after` 钩子里卸载 |
| 不测什么 | 业务 UI 渲染耗时、PTY 真实终端渲染、移动端（移动端是自持业务 App，判据同源但结论不共用，§5.4） |
| 产品代码面 | **仅一处**：`src-tauri/src/bench_channel.rs`（Channel 传输面基准命令），**`#[cfg(debug_assertions)]` 门 + 闸门锁测试**，release 产物不含；其余全在测试目录 / 夹具 / e2e |

## 3. 架构

```text
┌─ webview 层（e2e）──────────────────────────────────────────┐
│ e2e/specs/bench.spec.ts（真实 Tauri webview + tauri-driver）│
│   W1/W2 invoke 往返（0 B ~ 1 MiB）                          │
│   W3/W4 listen 事件到达（单大包 vs 分块）                    │
└───────────────────────────┬───────────────────────────────┘
                            │ 同一份夹具（zip 安装）
┌───────────────────────────▼───────────────────────────────┐
│ 宿主层 harness（cargo test --test wasm_bridge_bench）        │
│   support.rs   夹具编译 → 摆两个属主应用目录 → PluginHost    │
│   scenarios.rs 21 个场景 / 7 组（A 基线 … G 并发）            │
│   report.rs    中位数统计 + 数量级门禁 + JSON 导出            │
└───────────────────────────┬───────────────────────────────┘
                            │ 生产同形命令面
┌───────────────────────────▼───────────────────────────────┐
│ 被测端 packages/plugin-bench-test（wasm32-wasip3）           │
│   bench.* 命令面 + plugin:bench:probe 事件面                 │
│   两个属主：com.bedcode.bench（发）/ .peer（收 + 互调服务方） │
└───────────────────────────────────────────────────────────┘
```

**为什么走 `PluginHost` 而不是直接 `WasmRuntime`**：`plugin_invoke`（前端唯一命令入口）
= 身份校验 + `invoke_rust_command` → `invoke_wasm_command` → 实例锁 → `run_guest_call`
（`spawn_blocking` + `Arc<Mutex<LoadedWasmPlugin>>` + `block_on_async`）。
绕开它测出来的数不是前端真实感受到的数。

**为什么用两个属主实例**：总线**不投递给发送者自身**（`bus.rs`），互调回复要另一个
实例应答；同一组件以两个 plugin id 实例化即可（与 `plugin-pty-test` 的 peer 先例同构）。

## 4. 场景矩阵

见 `bedcode-desktop/bench/README.md` 的完整表格（A~G 共 21 个场景 / 60+ 测点）。
分组：A 基线 / B 原语 / C 事件 / D 互调 / E 流式 / F 异步 / G 并发。

## 5. 取数纪律

1. 每测点重复 N 次（默认 5），取**中位数**；每场景先跑一次预热（不记录）；
2. guest 侧用 `std::time::Instant`（wasi:clocks）分段计时并回传 `*Nanos`，
   墙钟 − guest 内 = 宿主侧开销（锁 / 序列化 / 事件投递 / 任务调度）；
3. 每个测点带**行为断言**（收讫字节数、调用次数、计数增量），纯计时不算数；
4. 门禁是**数量级**门（同 `terminal_output_perf.rs` 口径）：机器差异不该让基准变红，
   「慢了 10 倍」必须立刻可见；
5. **污染源登记**：`host-timer` 无注销原语，注册后每 tick 抢实例锁 → F2 必须排最后
   （实测曾把 G1 抬高一倍：145 µs vs 61 µs，已修）。

## 6. 已知边界 / 踩过的坑

- **webview 层已实跑**（`e2e/specs/bench.spec.ts`，2 场景全绿）。它的三个前置缺一不可：
  ① **vite dev server**（`pnpm run dev`）——debug 二进制走 `devUrl`
  `http://localhost:1420`，没有它 webview 里没有应用页面，`execute` 根本注入不了 API
  （首次报 "Tauri plugin not available"）；② 已构建前端产物；③ 基准包 zip。
  另外需**独占应用实例**（端口 8767 + app data 目录）。
- **`browser.tauri.execute` 只带走函数源码**：闭包变量与外层参数都带不过去
  （报 `Can't find variable: X`）。本工程用 `webviewScript()` 把 Node 侧取值
  `JSON.stringify` 后插值进脚本体。
- **凭证纪律**：`plugin_frontend_loader_session` 首调用者生效，真前端 bootstrap 已占用，
  e2e 不能再取（报 "already issued"）。解法：动态 `import('/src/plugin/commands.ts')`
  拿**同一份 ES module 实例**，`ensureHostCredential()` 返回已缓存的凭证
  （不重发）——顺带让测量走的就是应用自己的命令封装。
- **W5 的两条形态陷阱（都已实测确认）**：
  ① `Channel<Vec<u8>>` **不是**字节流——tauri 的 `IpcResponse` 泛型 blanket impl
  （`ipc/mod.rs:181`）把它序列化成 JSON 数字数组，JS 收到 `[object Array]`（100 ms/1 MiB，
  是 raw 的 6×）；真字节要 `Channel<Response>` + `InvokeResponseBody::Raw`（7 ms/1 MiB）。
  ② 块太小也贵：4 KiB×256（52 ms）比 16 KiB×64（17 ms）慢 3×。
- **Channel 构造器**取自全局 `window.__TAURI__.core.Channel`（app 配了
  `withGlobalTauri: true`）；动态注入脚本不经 vite 依赖解析，裸 specifier 解析不了。
- **WebKit 把 `performance.now()` 量化到 ~1 ms**：单次调用测出来全是整数（首次 nop
  读数 0.000 ms）。必须批量计时（一批 20~50 次取总耗时再摊每次）。
- **卸载前必须先停用**：宿主拒绝卸载运行中的插件（"Plugin is running"），残留安装还会
  让下次 `before` 钩子失败——`before` 已做成幂等（先 deactivate+uninstall 再装）。
- 无头上下文下 `host-events.emit` 降级（无 AppHandle → warn + Ok），C 组测的是
  「guest 序列化 + import 桥」，**不含** IPC 与 webview 派发；
- 宿主侧原语（B 组 crypto / storage / db）跑在 **debug 构建**上，绝对值偏低，
  用于**横向比较与回归**，不代表 release 性能；
- guest 侧无法消费 `host-http` 流式响应（流事件只到前端），故 E3 只测非流式。

## 7. 跑法

```bash
cd bedcode-desktop
pnpm run bench          # 冒烟（3 个数量级门禁探针，CI 友好）
pnpm run bench:full     # 全量场景矩阵
cd src-tauri && cargo test --test wasm_bridge_bench -- --full --group C --json ../bench/reports/x.json
```

## 8. 交付物

| 路径 | 内容 |
| --- | --- |
| `bedcode-desktop/bench/README.md` | 工程说明 + 场景矩阵 + 读数口径 |
| `bedcode-desktop/packages/plugin-bench-test/` | 被测端夹具（wasm32-wasip3） |
| `bedcode-desktop/src-tauri/tests/wasm_bridge_bench/` | 宿主层 harness（main / support / scenarios / report） |
| `bedcode-desktop/bench/scripts/build-bench-zip.mjs` | 夹具 → 可安装 zip（供 e2e） |
| `bedcode-desktop/e2e/specs/bench.spec.ts` | webview 层场景 |
| `.scratch/2026-09-26-wasm-bridge-bench/report.md` | 首轮基线读数与结论 |

## 9. 顺带发现（不在本工程范围，另立项）

见 report.md §结论：总线突发丢弃率 79%、`storage` 1 MiB 往返 26 ms、
debug 构建下 AEAD 仅 4.6 MiB/s、`host-timer` 无注销原语。
