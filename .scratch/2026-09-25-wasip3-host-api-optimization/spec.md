# 桌面端 WASI P3 宿主能力原生异步化与标准能力收敛

Status: **ready-for-agent**
Date: 2026-09-25
范围: **桌面端插件运行时**；共享接口若改 WIT，必须先完成移动端影响评估并按双端契约同步推进
承接: A0-3 宿主 async 基础设施、`wasmtime-wasi` 48 的 P2/P3 双 linker、manifest `wasiPreopenDirs`、`host-task` 真并行任务域
关联: ADR 0019（wasmtime 双端锁版）、ADR 0022（宿主只暴露无业务语义原语）、插件开发检查清单

> 本 spec 记录优化方向与分阶段门禁，不授权一次性改完 22 个 `host-*` interface。P3 是通用 capability ABI，不是 BedCode 宿主能力的整体替代品。

---

## 1. Problem Statement

### 1.1 当前基线

桌面端已经完成以下 P3 基础建设：

1. 生产插件使用 `wasm32-wasip3` 构建，Engine 开启 Component Model async。
2. 实例化与插件导出调用统一走 async 入口。
3. 宿主同时注册 WASI P2 与 P3 linker；P3 标准接口已覆盖 CLI、clocks、random、filesystem、sockets。
4. 插件导出回调已是 async 绑定，但 BedCode 自定义 `host-*` import 仍以同步函数为主。
5. 当前 desktop ABI 为 v29，插件 world 含 22 个 `host-*` interface、约 120 个函数；绝大多数函数没有等待语义或只有短时等待。

因此，当前状态是：

```text
P3 标准 WASI import：async
BedCode host-* import：同步
宿主调用 guest export：async
guest export 外层驱动：多数入口仍经同步桥
```

### 1.2 长等待接口仍会阻塞

以下操作内部包含网络、磁盘、UI 授权或跨插件等待，但当前宿主能力实现仍通过同步函数和 `block_on_async` 驱动：

- 插件互调等待回复；
- 非流式 HTTP 请求；
- WebSocket 握手；
- peer TLS 拨号与自动重拨；
- 文件目录授权弹窗；
- 同步进程执行与同步任务 join（这两类属于显式同步语义，不自动归入 async 化）。

短调用桥开销本身很小，现有基线约为 `0.13–0.19 µs/op`，不是性能瓶颈。问题在于长等待期间 guest 调用不能沿 P3 async 语义挂起，宿主执行线程仍被占住；插件多、慢网络或授权等待叠加时，会增加 worker 占用和尾延迟。

### 1.3 标准 WASI 与自定义宿主能力重叠但不能机械替换

- manifest 声明的固定数据目录已经适合直接使用标准 `wasi:filesystem`；AI Chatbox 已通过 `/data` preopen 使用该路径。
- `host-fs` 仍负责用户运行时选择的任意目录、WSL 路径与逐调用 fs_auth，不能被 preopen 完全替代。
- `host-http`、`host-websocket`、`host-peer`、`host-mdns` 包含 BedCode 的权限、SSRF/过滤、属主隔离、认证策略、端点注册、信任与生命周期语义，不能直接下沉给通用 `wasi:http` / `wasi:sockets`。
- `host-pty`、`host-task`、`host-auth`、`host-storage`、数据库、消息总线等是 BedCode 或宿主独占原语，WASI 0.3 没有等价替代。
- Wasmtime 48 的 `wasmtime-wasi::p3` 与 `wasmtime-wasi-http::p3` 仍明确标记为 experimental、unstable、incomplete；本轮不能把生产正确性押在 P3 HTTP 的成熟度上。

### 1.4 P3 文件系统证据链仍不完整

- 生产插件已经使用 P3 preopen 文件访问。
- 宿主自动化 preopen E2E 仍主要构建 `wasm32-wasip2` fixture。
- P3 fixture 当前只明确覆盖 clocks/random，尚未把可写、只读、根外不可达三条文件能力契约完整锁在 P3 上。
- dev watch 仍残留 AI Chatbox 的 `wasm32-wasip2` 与 file-transfer 的 `wasm32-unknown-unknown` 硬编码目标路径，和共享 P3 构建链存在漂移。

---

## 2. Solution

采用“先证明、再纵切、后扩面”的优化路线：

1. **P0 机制探针**：在不修改生产 WIT/ABI 的前提下，用测试专用 async host import 证明当前锁定工具链能在 guest import 等待时让出宿主执行线程。
2. **P1 生产纵切**：探针通过后，只选择一个有真实消费者、等待明显、兼容面最小的接口做原生 async 化；先建立行为与性能基线，再扩到其它接口。
3. **P2 标准文件系统收敛**：固定声明目录统一走 P3 preopen；动态目录、WSL 与逐调用授权继续走 `host-fs`。补齐 P3 文件系统 E2E 后再决定是否移除 P2 linker。
4. **P3 扩面与收口**：按接口风险分批 async 化；共享接口同步评估移动端；清理同步桥与过时构建路径；最后再评估 P2 硬切。
5. **中期观察项**：Rust `wasm32-wasip3` cooperative threading 工具链成熟后，评估缩小 `host-task.execute-batch`；在此之前 `host-task` 继续承担真并行。

目标不是减少所有自定义接口，而是让等待型原语符合 P3 的挂起语义，同时保持安全边界、同实例串行和 ABI fail-visible 纪律。

---

## 3. User Stories

1. 作为插件开发者，我希望长时间网络或授权调用可以挂起，以便插件等待期间不无谓占住宿主执行线程。
2. 作为宿主维护者，我希望长等待原语直接使用 async，而不是在同步接口中反复嵌套 `block_on_async`，以便调度和错误传播更清晰。
3. 作为插件开发者，我希望现有同步短调用保持轻量，以免 async 化增加无收益的复杂度。
4. 作为安全负责人，我希望 async 化不绕过权限、属主、fs_auth、SSRF、流量过滤和配额，以便性能优化不降低安全边界。
5. 作为插件作者，我希望同实例仍严格串行，以免 cooperative scheduling 引入 Store 重入和插件静态状态竞态。
6. 作为插件作者，我希望 manifest 声明的数据目录优先使用标准 P3 filesystem，以便直接使用类型化、二进制和流式文件能力。
7. 作为插件作者，我希望用户运行时选择的任意目录仍能通过 `host-fs` 授权访问，以免固定 preopen 限制真实工作流。
8. 作为插件作者，我希望旧 ABI 产物在实例化期得到明确重建提示，以免 async 契约变更后静默失效。
9. 作为测试维护者，我希望 P3 filesystem 有可写、只读和沙箱边界 E2E，以便移除 P2 兼容链前有完整证据。
10. 作为性能维护者，我希望区分短调用开销与长 I/O 占用收益，以免用无意义的微基准宣称优化成功。
11. 作为移动端维护者，我希望共享接口的 async 变更有明确双端影响结论，以免桌面单方面破坏插件契约。
12. 作为架构维护者，我希望 BedCode 特有原语保持定制，以免为了“标准化”把业务或安全语义下沉到错误位置。
13. 作为发布维护者，我希望 P3 experimental 风险显式登记，以便 Wasmtime 升级时评估兼容与安全修复策略。
14. 作为 dev 开发者，我希望 watch 复制与构建脚本使用同一个 P3 target 真源，以免新前端搭配旧 WASM 产物。

---

## 4. Implementation Decisions

### D1：P3 是能力 ABI，不是宿主 API 重写授权

保留 ADR 0022 的裁剪线。WASI 能提供的能力优先使用标准接口；BedCode 权限、安全、生命周期或物理上离开宿主就无法实现的能力继续保留专用接口。

目标终态不是“尽可能少写 `host-*`”，而是“每个接口都只承载它真正独占的能力”。

### D2：按等待语义分类，不按 interface 整体一刀切

| 分类 | 典型能力 | 处理原则 |
| --- | --- | --- |
| 长等待候选 | `host-api-call.call`、非流式 `host-http.fetch`、`host-websocket.connect`、`host-peer.dial-peer`、`host-fs.request-auth` | P0 探针通过后逐项 native async 化 |
| 已是异步模型 | `host-process.run`、流式 HTTP、`host-task.submit` + 事件 | 保持现有模型，不重复造 async 通道 |
| 显式同步语义 | `host-process.run-sync`、`host-task.execute-batch` | 保留；它们分别承诺短命令结果与 OS 线程池真并行 join |
| 短时本地操作 | log、KV、短查询、bus publish、句柄入队、清单/快照查询、PTY ring fetch | 默认保持同步；只有实测阻塞达到阈值才重新分类 |

`host-task.execute-batch` 不能因 P3 cooperative threading 出现就立即删除：协作式线程是单 Store 逻辑交错，`host-task` 是宿主 OS 线程池真并行，两者不等价。

### D3：同实例串行红线保持不变

async 化只允许宿主线程在 await 点让出。插件实例锁继续跨 await 持有，禁止在 await 点释放；宿主不得并发进入同一 guest 实例。

验收必须同时证明：

1. 慢 import 等待期间，不相关 Tokio 任务仍可推进；
2. 同一实例的第二个 guest 进入仍被串行化；
3. 不同实例之间不因一把全局锁互相阻塞。

### D4：async 化属于插件契约变更

把 WIT 函数从同步改为 `async func` 会改变 Canonical ABI。实施时必须：

- bump desktop ABI；
- 更新 SDK 生成绑定与插件侧 trait/调用链；
- 旧产物在实例化期点名缺失 interface/新 ABI，并提示按目标 SDK 重建；
- 不允许旧同步 import 被静默 polyfill 成“能跑但语义不同”的路径。

若接口属于 desktop 与 mobile 共有面，先完成移动端 Wasmtime、WIT、SDK 与产物影响评估；未经明确偏离裁决，不得只改桌面副本。

### D5：生产纵切优先选择真实消费者

探针通过后，候选按以下条件排序：

1. 当前确有生产插件调用；
2. 等待时间占调用生命周期的主要部分；
3. 权限/属主/错误契约清晰，可写行为矩阵；
4. 兼容面最小，优先 desktop-only 或经双端同步的接口。

当前高价值候选包括非流式 HTTP、文件授权与 peer 拨号。`host-websocket.connect` 虽是清晰的 desktop-only 纵切，但当前生产插件无明确消费者，不应仅为“接口漂亮”而优先实施。

### D6：声明型文件访问优先使用标准 P3 filesystem

- manifest `wasiPreopenDirs` 声明并获授权的目录，作为插件固定数据根，优先直接使用 P3 filesystem。
- `host-fs` 保留给用户运行时选择的任意路径、WSL UNC、动态授权和 preopen 外的文件操作。
- 两套能力不互相推导：授权记录不自动扩大 preopen，preopen 也不替代逐调用 fs_auth。
- `canonicalize`、宿主绝对路径展示和 WSL 桥接仍属于 `host-fs` 能力，不为追求“全标准”而删除。

### D7：P3 filesystem E2E 是 P2 清理前置门

在删除 P2 linker 前必须用 `wasm32-wasip3` fixture 覆盖：

- 可写 preopen：写、读回、列目录；
- 只读 preopen：读/列允许，写被拒且宿主文件字节不变；
- 根外路径不可达；
- 写失败后 Store 仍可继续调用；
- 标准 P3 filesystem 与至少一个同步 `host-*` import 能在同一实例共存。

P2 linker 的最终删除另立硬切票，不与首个 async 纵切捆绑。

### D8：网络标准能力不作为本轮替代方案

- 不默认开放 `inherit_network`。
- 不用 `wasi:sockets` 绕过 host-http / host-websocket / host-peer 的权限与过滤链。
- 不把 `host-websocket` 或 `host-peer` 拆成 guest 自行实现的 TCP 协议栈。
- 不采用 `wasi:http` 替换现有 `host-http.fetch`，直到 Wasmtime P3 HTTP 达到项目可接受的生产成熟度并完成安全链适配评估。

### D9：不默认继承 stdio、argv、env 或 CLI world

当前插件以长生命周期 `command.invoke` 为主，没有独立 runnable CLI 需求。仅在未来明确增加 CLI plugin mode 时，才评估 `wasi:cli/command`；`host-app.install-cli` 是随包 CLI 安装与 PATH 管理，不等同于 WASI CLI。

### D10：性能收益必须用等待型证据表达

验收不只看每次调用的微秒数，至少记录：

- 慢 host import 期间，不相关 Tokio 心跳是否持续推进；
- 同一 runtime 上其它插件任务是否被延迟；
- 短调用相对现有 `~38–42 µs/op` 基线是否回退；
- 长等待前后的 worker 占用与尾延迟；
- async 化前后错误、超时、取消和清理次数是否一致。

短调用桥开销本来已低于总调用成本 0.5%，因此“移除 bridge”不是本专项的主要收益叙事。

### D11：构建与 dev watch 使用同一 P3 target 真源

插件 watch、一次性补建和发布打包必须引用共享 `wasm32-wasip3` 配置，不允许继续维护 AI Chatbox 的 P2、file-transfer 的 unknown-unknown 硬编码路径。该修正应单独成票，避免和 async 契约改造混在一起。

---

## 5. 实施阶段

| 阶段 | 目标 | 产物 / 门禁 | 依赖 |
| --- | --- | --- | --- |
| P0 | 证明 async host import 纵切可行 | 测试专用 async import + 慢等待 fixture + runtime 让出证据；生产 WIT/ABI 零变更 | 无；见 issue 01 |
| P1 | 选择一个真实生产接口纵切 | 行为/安全/兼容/性能矩阵；旧产物 fail-visible；不扩散到全量接口 | P0 |
| P2 | 补齐 P3 filesystem 证据 | P3 可写、只读、根外隔离、共存 E2E | 可与 P1 并行 |
| P3 | 按类别扩面 | 每批只迁一类等待语义；共享接口完成双端评估 | P1 + P2 |
| P4 | 清理兼容与开发链 | 评估 P2 linker 硬切；修 dev watch target；清理无用同步桥 | P3 |
| P5 | 全量回归与文档收口 | 性能报告、ADR/清单/code-map/CHANGELOG 更新、全量门禁 | P1–P4 |

P1 的最终接口在 P0 结果出来后另开 issue，不在本 spec 预先锁定，避免基于未验证机制提前改公共契约。

---

## 6. Testing Decisions

### 6.1 测试 seam

最高 seam 是“真实 P3 组件经真实 Linker 调宿主 import”。测试不得只调用 Rust 域函数或只验证 mock：

```text
P3 fixture guest export
  → async host import
  → Wasmtime Linker
  → 真实宿主测试实现
  → 可观测结果 / 错误 / 清理
```

P0 使用独立 test-only WIT world，避免生产插件契约被探针污染。

### 6.2 行为矩阵

每个生产 async 纵切至少覆盖：

1. 正常完成；
2. 立即失败；
3. 超时或取消；
4. 等待期间 runtime 仍能推进无关任务；
5. 同一实例保持串行；
6. 不同实例互不误阻塞；
7. 权限不足和属主错误；
8. trap/取消后资源清理；
9. 旧产物实例化期明确失败；
10. Store 在失败后仍健康或按既有 trap 语义整体重建。

### 6.3 文件系统矩阵

- P3 可写 preopen；
- P3 只读 preopen；
- P3 根外隔离；
- P3 文件 API 与同步 host import 共存；
- `host-fs` 动态授权正反例保持不变；
- fs_auth 任务单元仍不得弹窗。

### 6.4 性能矩阵

- 保留现有短调用基线；
- 新增长等待 fixture，对比 async 前后 runtime 推进能力；
- 记录运行环境、Wasmtime、Rust nightly 与样本数；
- 不把单次本地测量写成普遍吞吐提升；
- 若无稳定收益，不进入公共契约扩面。

### 6.5 命令

开发中只跑针对性测试：

```bash
cd bedcode-desktop/src-tauri && ~/.cargo/bin/cargo test <测试名称前缀>
```

任务收尾再跑桌面全量：

```bash
cd bedcode-desktop/src-tauri && ~/.cargo/bin/cargo test
```

若后续改前端，再按对应端执行 `pnpm run test:run`；根目录执行 `pnpm exec eslint .`。所有依赖 WASM fixture 的 Rust 测试必须使用 rustup shim 的 `~/.cargo/bin/cargo`。

---

## 7. Acceptance Criteria

1. P0 测试证明 async host import 能在等待时让出 runtime，且未改生产 WIT/ABI。
2. P1 只迁一个经数据证明确有收益的生产接口，不以函数数量为目标。
3. 同实例串行、权限、属主、fail-visible 和资源回收契约全部保持。
4. 短调用无数量级回退；长等待有 runtime 让出或尾延迟改善证据。
5. P3 filesystem E2E 覆盖可写、只读、根外隔离和同步 host import 共存。
6. `host-fs` 动态授权与 WSL 能力没有被 preopen 误替换。
7. P2 linker 未在 E2E 与兼容裁决前被误删。
8. 网络、PTY、task、auth、storage、database 等 BedCode 专属原语保持 ADR 0022 边界。
9. 移动端影响对共享接口有明确结论；无未授权的桌面单边 WIT 漂移。
10. 收尾全量 Rust 回归、格式化、clippy 与文档更新完成。

---

## 8. Out of Scope

- 不把全部 `host-*` 一次性改成 async。
- 不用 P3 重写 BedCode 的 WebSocket、peer、mDNS、PTY、数据库、认证、存储或消息总线。
- 不把 P3 cooperative threads 当成真并行能力。
- 不在本专项删除 `host-task`。
- 不开放默认网络、stdio、argv 或 env 继承。
- 不立即删除 P2 linker；删除需独立兼容硬切。
- 不升级 Wasmtime、wit-bindgen 或 Rust nightly；工具链升级继续按 ADR 0019 双端评估。
- 不修改移动端实现；共享接口只有在独立双端票中才同步推进。
- 不以减少 WIT 函数数为目标重构业务能力面。

---

## 9. 收益判断

### 可能有明确收益

- 长网络、授权、跨插件等待期间释放宿主执行线程；
- 降低多插件并发时的尾延迟与 worker 饥饿风险；
- 为未来 cooperative threading 提供必要 await 点；
- 固定数据目录使用 P3 typed stream/binary I/O，减少不必要的 JSON 与宿主往返；
- P2 清理后减少双 linker、双 fixture 和兼容测试税。

### 不应承诺收益

- 短 host 调用不会因移除 `block_on_async` 获得显著提速；
- 同实例串行不变，单插件内部不会凭此自动并行；
- 大多数 BedCode 专属接口不会因 WASI 0.3 出现而消失；
- P3 HTTP 在 Wasmtime 48 上替换现有 host-http 暂不具备生产成熟度优势。

---

## 10. Further Notes

### 当前证据

- desktop SDK ABI v29：`bedcode-desktop/packages/plugin-sdk-desktop/rust/src/abi.rs`
- P2/P3 linker 与 async 实例化：`bedcode-desktop/src-tauri/src/wasm_core/manager/runtime/component.rs`
- 插件 world 的 22 个 `host-*` import：`bedcode-desktop/packages/plugin-sdk-desktop/rust/wit/bedcode.wit`
- 互调同步等待：`bedcode-desktop/src-tauri/src/wasm_core/host_api/api.rs`
- 非流式 HTTP 同步等待：`bedcode-desktop/src-tauri/src/wasm_core/host_api/http.rs`
- WS 同步握手：`bedcode-desktop/src-tauri/src/wasm_core/host_api/ws.rs`
- peer 同步拨号/重拨：`bedcode-desktop/src-tauri/src/wasm_core/host_api/peer.rs`
- fs 授权同步等待：`bedcode-desktop/src-tauri/src/wasm_core/host_api/fs.rs`
- P2 filesystem E2E 与 P3 clocks/random E2E：`bedcode-desktop/src-tauri/src/wasm_core/manager/runtime/tests/wasi_e2e.rs`
- 短调用基线与 async 化收益边界：`.scratch/2026-09-21-a0-3-host-async/report.md`
- cooperative threading 与 host-task 边界：`.scratch/2026-09-21-host-task-concurrency/spec.md`

### 外部事实

- WASI 0.3 原生 async、stream、future 与各接口变化：<https://wasi.dev/releases/wasi-p3>
- Wasmtime 48 P3 模块成熟度：<https://docs.rs/wasmtime-wasi/48.0.0/wasmtime_wasi/p3/index.html>
- Wasmtime 48 P3 HTTP 成熟度：<https://docs.rs/wasmtime-wasi-http/48.0.0/wasmtime_wasi_http/p3/index.html>
