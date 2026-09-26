# bench · 桥接基准工程（wasm ↔ 宿主 ↔ 前端）

测量**桌面端桥接链**的性能：前端命令面 → Tauri 宿主 → wasm 应用 → 宿主原语 → 回到前端。
场景全部对齐桌面端真实业务（终端输出流、文件传输、AI 供应商响应、插件互调、任务队列变更…）。

```text
bedcode-desktop/bench/                      ← 本目录：工程说明与约定
bedcode-desktop/packages/plugin-bench-test/  ← 被测端（wasm 夹具，命令面 = bench.* 场景）
bedcode-desktop/src-tauri/tests/wasm_bridge_bench/  ← 驱动端（宿主侧 harness，harness=false）
bedcode-desktop/e2e/specs/bench.spec.ts     ← 驱动端（webview 层，真 Tauri 窗口）
```

- 设计文档：`.scratch/2026-09-26-wasm-bridge-bench/spec.md`
- 夹具不随产品分发：不进 `wasm-apps/`、不进 `resources/plugins/`、不进打包脚本。

## 跑法

```bash
cd bedcode-desktop/src-tauri

# 冒烟（默认，无参数）：3 个数量级门禁探针，秒级——CI 用它守「桥接链没有数量级回归」
cargo test --test wasm_bridge_bench

# 全量场景矩阵（约 1~3 分钟）
cargo test --test wasm_bridge_bench -- --full

# 只跑某一组 / 某个测点 / 指定重复次数 / 导出 JSON
cargo test --test wasm_bridge_bench -- --full --group C
cargo test --test wasm_bridge_bench -- --full --group E1
cargo test --test wasm_bridge_bench -- --full --iters 9 --json bench/reports/latest.json

# 场景清单
cargo test --test wasm_bridge_bench -- --list

# release 构件（贴近产品；首次编译较久）
cargo test --release --test wasm_bridge_bench -- --full
```

> **必须用 rustup shim 的 `cargo`**（`~/.cargo/bin/cargo`）。夹具在 harness 内以
> `cargo build --target wasm32-wasip3` 现场编译并显式注入 `RUSTUP_TOOLCHAIN`（pin 见
> `support.rs::WASIP3_TOOLCHAIN`，可用 `BENCH_WASIP3_TOOLCHAIN` 覆盖）。
> 把 `~/.rustup/toolchains/*/bin` 前置进 PATH 会让注入失效。

或走 pnpm 包装（等价）：

```bash
cd bedcode-desktop
pnpm run bench          # 冒烟
pnpm run bench:full     # 全量
```

### webview 层（真 Tauri 窗口，补 IPC + 前端派发两段）

```bash
cd bedcode-desktop
pnpm run build                       # 应用需要已构建的前端产物
pnpm run bench:zip                   # 夹具 → 可安装 zip（bench/dist/）
pnpm run dev &                       # ⚠️ 必需：debug 二进制走 devUrl :1420
pnpm exec wdio run wdio.conf.ts --spec e2e/specs/bench.spec.ts
```

> `pnpm run test:e2e -- --spec …` **不行**：多一层 `--` 会让 wdio 把它当位置参数，
> 结果两个 spec 一起跑。要单独跑某个 spec 就用 `pnpm exec wdio run … --spec …`。

e2e 会把夹具按**生产路径**装进应用（`plugin_install_from_file` zip 安装器 →
`plugin_approve` 审批门禁 → `plugin_activate`），跑完在 `after` 钩子里卸载。

> ⚠️ **需要独占应用实例**：桌面端口（默认 8767）与 `~/.local/share/com.bedcode.app`
> 是全局的。若已有 BedCode 在跑，第二个实例会弹端口冲突对话框 → WebDriver 会话超时。
> 先关掉已运行的桌面端再跑。依赖：`tauri-driver`（`cargo install tauri-driver`，
> wdio 配置里 `autoInstallTauriDriver: true` 也会自动装）+ 系统 `WebKitWebDriver`。
> 本工程额外把 `@wdio/local-runner` 加进 devDependencies——仓库的 `wdio.conf.ts`
> 声明了 `runner: 'local'` 却没装对应包，e2e 此前跑不起来。
>
> W5 需要 **debug 构建的应用**（`cargo build --bin bedcode-desktop` 后再跑 e2e）——
> Channel 基准命令是 `#[cfg(debug_assertions)]` 门的产品代码面，release 二进制没有它。
>
> 本层踩过的三个坑（已内联在 spec 注释里，展开看）：
> `execute` 只带走函数源码（闭包变量带不过去）／凭证首调用者生效（要借应用自己的
> `commands.ts` 模块实例）／WebKit 的 `performance.now()` 量化到 ~1 ms（必须批量计时）。

## 场景矩阵

| 层 | 场景 | 业务对应 | 测的是什么 |
| --- | --- | --- | --- |
| --- | --- | --- | --- |
| **A** | A1 `nop` | 任意 UI 交互 | 命令面固定开销下界（一次完整往返，无原语调用） |
| | A2 `echo` 0 B ~ 1 MiB | 命令返回大列表/大对象 | 同步大载荷的编解码 + 搬运成本 |
| | A3 `churn` | — | guest 内部 CPU 下界；**A2 − A3 = 跨越 WASM 边界的净成本** |
| **B** | B1 `host-log` ×200 | 插件日志 | 一次 host import 调用的固定开销（最廉价的标尺） |
| | B2 storage 大 value | 会话注解 / 设置项 | 插件私有 KV 的固定开销 + 每字节成本 |
| | B3 AEAD 加解密 | 链路加密（HTTP 信封 / 传输分块） | 加密引擎吞吐（guest→host→guest 双向） |
| | B4 fs 写 + 读 | 技能文件 / 传输落盘 | 文件面往返（含 `fs_auth` 前缀校验） |
| | B5 私有库批量写 + 读回 | 会话表 / 传输历史 | 每行摊销成本 |
| | B6 `execute-batch` | 批量落库 | 事务批执行的固定开销 |
| **C** | C1 `emit` 大包单发 | 任务队列变更 / 连接状态变更 | wasm→宿主→前端 单包推送的每次成本 |
| | C2 `emit` 分块 1 MiB | 终端输出 / AI 流式回复 | **单大包 vs N 小包**的摊薄曲线（output-ack 论题的实测依据） |
| **D** | D1 总线 JSON | 插件间事件流 | wasm→宿主总线→wasm 单次发布成本 |
| | D1b 总线突发丢弃率 | 高频事件流 | 订阅队列容量 64 下的送达率（背压语义） |
| | D2 总线二进制 | 大载荷事件流 | 二进制通道吞吐（零 JSON 编解码） |
| | D3 互调（小） | 插件互调（ADR 0017） | JSON-RPC 阻塞往返成本 |
| | D4 互调（64 KiB reply） | 互调大结果回传 | reply 体积对往返成本的影响 |
| **E** | E1 `ring-fetch` 批量曲线 | 终端输出流消费 | 不同批量的单次/总成本（16 KiB 为宿主钳制值） |
| | E2 输出流 + 二进制转发 | 消费即再分发 | 输出流「拉取 + 转投另一插件」的双跳成本 |
| | E3 HTTP 非流式大响应 | AI 供应商响应 / 资源下载 | 整包响应的端到端吞吐 |
| **F** | F1 `run-sync` vs `run` | agent-hub 调 CLI | 同步阻塞 vs 异步事件回调的形态差 |
| | F2 定时器回调 | 轮询 / 心跳 | 周期任务回灌命令面 |
| **W**（e2e） | W1/W2 invoke 往返 | 前端调插件命令面 | Tauri IPC + 前端桥接的固定开销与每字节成本 |
| | W3/W4 事件到达 | AI 流式 / 进度推送 | emit 面的端到端到达（单大包 vs 分块） |
| | W5 **Channel 推送** | **output-ack P2 的运输面选型** | raw / `Vec<u8>` / text 三形态的 1 MiB 端到端 |
| **G** | G1 1/2/4/8 路并发 `nop` | 多窗口 / 多会话 | 并发扇出（`nop` 太短，串行化不可见） |
| | G2 慢调用在途时并发 | 慢 storage/http/fs 往返 | **慢调用是否堵死同实例全部交互**（并发模型专项的验收基准） |

每个测点都带**行为断言**（收讫字节数 / 调用次数 / 计数增量），不是纯计时——
数字建立在「确实跑对了」之上。

> **执行顺序有硬约束**：`host-timer` 没有注销原语，一旦注册，每 tick 都会去抢该插件的
> 实例锁。因此 **F2（定时器）排在最后**——实测放在中间会把紧随其后的 G1 抬高近一倍
> （145 µs vs 61 µs）。新增会注册周期任务的场景时同理。

## 读数口径（重要）

| 口径 | 覆盖的链路 | 不覆盖 |
| --- | --- | --- |
| harness（本工程） | 前端命令面之后的一切：`invoke_rust_command` → 实例锁 → guest 调用 → host import → 事件序列化 | Tauri IPC 帧、前端 JS 派发、webview 渲染 |
| e2e（`e2e/specs/bench.spec.ts`） | 真实 webview 里的 `invoke('plugin_invoke')` 往返 + `listen()` 事件到达 | 业务 UI 渲染 |

无头上下文（`app_handle = None`）下 `host-events.emit` 降级为「记 warn + 返回 Ok」，
所以 **C 组测到的是 guest 序列化 + import 桥**，不是真实前端投递；
真实投递由 e2e 层补齐。报告中该组已标注「无头口径」。

另外：宿主侧依赖（`serde_json` 等）在 dev profile 下仍按 `opt-level = 2` 编译，
但**宿主自身代码是未优化**的——B 组（storage / db / crypto）的绝对值只适合**横向比较
与回归**，不代表 release 性能。需要绝对值就跑 `--release`。

## 首轮结论摘要

完整读数与推导见 `.scratch/2026-09-26-wasm-bridge-bench/report.md`。要点：

| 结论 | 数据 |
| --- | --- |
| **Tauri IPC 段才是大头** | `nop` 宿主内 54 µs → **真前端 560 µs**；1 MiB 命令返回 1.87 ms → **27.7 ms**（≈ +24.6 ns/B） |
| 命令面固定开销本身不是瓶颈 | `nop` 往返 **54 µs**（16 ms 帧预算的 0.3%） |
| 跨界成本随载荷线性 | 1 MiB 同步回传 **1.77 ms**（~1.7 ns/B） |
| **Channel raw 是最快的路** | 1 MiB 单块 **7.0 ms**（vs 事件单包 33 ms、命令返回 28 ms）——若 output-ack P2 走宿主侧 push，选 Channel |
| ⚠️ `Channel<Vec<u8>>` 是陷阱 | 1 MiB **100 ms**（raw 的 6×）：tauri 的 `IpcResponse` 泛型 blanket impl 把它序列化成 JSON 数字数组；要真字节必须 `Channel<Response>` + `Raw` |
| 事件分块在**宿主侧与前端侧**都不线性恶化 | 1 MiB：宿主侧 4 次 2.62 ms vs 256 次 2.83 ms；webview 端到端单包 37 ms vs 16 KiB×64 27 ms vs 4 KiB×256 33 ms → **分块的理由应是平滑渲染/可 ack，不是性能** |
| ⚠️ 慢调用会堵死同实例全部交互 | 慢调用在途时 `nop` 从 156 µs → **26.1 ms（167×）**；4 路并发慢调用 = 4.04× 单次（完全串行） |
| 插件互调很便宜 | 单次 `api_call` **359 µs**；64 KiB reply 638 µs |
| ⚠️ 总线突发会静默丢 79% | 一次突发 512 条只送达 **20.7%**（订阅队列 64，满即丢且只有 warn） |
| ⚠️ KV 存大对象是成本洼地 | `storage` 1 MiB 往返 **26 ms**（~25 ns/B，比跨界慢一个量级） |
| 终端输出流 ~2/3 成本在宿主侧 | 1 MiB `ring-fetch` 19 ms，guest 内仅占 31% |
| 事务批处理比逐条便宜一个量级 | `execute-batch` 64 条 343 µs |

## 加新场景

1. 在 `packages/plugin-bench-test/src/lib.rs` 加一条 `bench.*` 命令（guest 侧）；
2. 在 `src-tauri/tests/wasm_bridge_bench/scenarios.rs` 的 `all()` 里登记 Scenario；
3. 重复数 / 单位 / 附注按现有口径写；必要时加一条**数量级**门禁（`Budget`）。

门禁只抓数量级回归（同 `terminal_output_perf.rs` 口径）：机器差异不该让基准变红，
但「慢了 10 倍」必须立刻可见。

## 收尾纪律

跑完请确认无残留：harness 自建临时目录并在 `shutdown()` 里停用插件 + 删目录；
HTTP 夹具服务器随 tokio runtime 退出。`cargo test` 后仍建议按 AGENTS §3 检查
端口 / 后台进程。
