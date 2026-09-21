# 终端输出消费插件化 · 性能前置验证

> 状态：**进行中**——探针 `bedcode-desktop/src-tauri/src/plugin/manager/wasm_runtime/tests/terminal_output_perf.rs`
> （只读探针，不碰生产路径）；结论贴档见本目录 `report.md`。
> 日期 2026-09-21。
> 上游：`.scratch/2026-09-10-plugin-kernel-roadmap/spec.md` 阶段 3（终端下沉前置条件）、
> `.scratch/2026-09-19-pty-base-service/spec.md`（host-pty v16，输出面「单生产者环 + 游标拉取」）、
> `.scratch/2026-09-19-terminal-session-plugin/spec.md`（D3 终端渲染管线留内核红线）。
> 关联红线：AGENTS §7（ABI 批次）、ADR 0022（裁剪线 / 双端偏离）、roadmap 阶段 3「输出分发
> 管道的『内核保有 + 消费插件化』先行验证通过」（**未验证**，本文即该项验证）。

---

## 1. 定位与划界

### 1.1 现状（2026-09-21 实测）

- **业务会话线输出路径**：`PtyReader 读线程 → UnifiedOutputQueue（单生产者环 + 游标 + ack → 订阅者执行体 → Tauri Channel 直推前端（Raw 字节），**全程不进 WASM**（`.scratch/2026-09-17-pty-pull-subscribers` 确立背压形态；`commands/terminal_stream.rs` 为桌面本地唯一消费路径）。
- **host-pty 线（ABI v16，已落地）**：插件私有 PTY。`PtyRing`（单生产者环，生产端零暂停）→ 插件按游标 `ring-fetch(pty-id, from-offset, max-bytes)` 拉取；`list<u8>` 直传（spec D7：不做 base64/JSON 包装，复用 publish-binary / ws send-binary 先例）。单次返回上限 `PLUGIN_PTY_RING_FETCH_MAX_BYTES`（16 KiB，读侧截断 + `next-offset` 续拉）。
- **fixture / 脚手架**：`packages/plugin-pty-test`（`pty-spawn` / `pty-ring-fetch` / `pty-write` 命令面，`maxBytes` 可控制批量）；`build_pty_test_component()` 测试内构建（`RUSTUP_TOOLCHAIN=nightly-2026-09-16`）；`setup_wasm_runtime` + `lock_pty_fixture_e2e`（pty 注册表进程级全局 + 互斥）；`a03_probe.rs` P5 微基准形态（宽松门槛防 CI 抖动）。

### 1.2 验证目标（回答 roadmap 前置问题）

**终端 UI/渲染下沉到插件（`com.bedcode.terminal` 或既有插件扩展域）后，PTY 输出路径多一跳 WASM 边界，这一跳的附加成本是否可接受？**

具体产出：
1. **宿主侧纯 Rust 基线**：`PtyRing` 写入 + 拉取全量输出的每字节成本（无 WASM 边界的下界）；
2. **插件侧确定性数据**：向同一 pty 的 ring 预写入 M 字节后，guest 循环 `ring-fetch` 的：
   - 单次调用固定开销（µs/op，含 wasmtime 桥 + `list<u8>` 编解码）；
   - 每字节成本与批量大小（4K / 16K / 64K / 256K）的关系曲线；
3. **真 PTY 端到端**：spawn 真实命令按固定字节率产出的消费进度，验证「生产端零暂停 + 拉取侧追赶」在真实负载下成立；
4. **终端典型负载折算**：4ms 合并窗口（`LOCAL_FLUSH_INTERVAL_MS`）等价于多少批量/频率；100 KB/s ~ 1 MB/s 实时流下插件消费路径的单核 CPU 占比估算；
5. **结论判定**：可行性 + 需要的批量策略（合并窗口 / 拉取节流）+ 红线修订建议（哪些可迁、哪些必须留内核）。

### 1.3 探针不动（硬约束）

- **生产路径零改动**：`wasm_runtime.rs` / `host_impl/pty.rs` / `PtyRing` / `plugin-pty-test` 产线代码一律不碰；仅 `wasm_runtime.rs` 的 `mod tests` 内新增一行 `mod terminal_output_perf;` 声明（测试域）。
- **fixture 只读复用**：不新增 fixture 命令；场景 2 的确定性数据经测试侧操作同一 `PTYS` 注册表写入（同 crate 测试可见），不依赖假插件。
- **宽松门槛**：数量级回归才失败；软性结论以贴档数据为准（同 a03 P5 先例）。
- 移动端零改动（host-pty 为桌面独有，双端偏离既有口径）。

---

## 2. 探针场景设计

### P1 · 宿主侧纯 Rust 基线（PtyRing 直操作）

- 构造 `PtyRing`（cap 4 MiB），push N=1 MB 确定性字节（单次 4 KiB 块 × 256）。
- 测量：全量拉取（fetch 循环到追平）的总耗时 → 每字节成本（ns/B）；单次 fetch 固定开销（µs/op）。
- 意义：无 WASM 边界的下界；后续所有倍数以它为分母。

### P2 · 插件侧确定性数据（同 ring、跨 WASM 边界）

- `setup_wasm_runtime` + 实例化 pty fixture；`pty-spawn` 真命令（`sleep 30`，产出可忽略）拿句柄；
- 测试侧向该 pty 的 `PtyRing` 写入 1 MB（绕过 PTY 读线程，确定性数据）；
- guest 循环 `pty-ring-fetch`，按不同 `maxBytes`（4K / 16K / 64K / 256K）分组采集：
  - 每轮单次调用的平均耗时（µs/op）→ 固定开销 + 编解码；
  - 全量 1 MB 拉完总耗时 → 每字节成本（ns/B）；
  - 相对 P1 的倍数。
- 意义：WASM 边界（wasmtime 桥 + `list<u8>` 拷贝 + JSON-RPC 命令通道）的净附加成本。

### P3 · 真 PTY 端到端（生产端真实节奏）

- `pty-spawn` 真命令产出固定字节率（如 `cat` 一个预生成 4 MB 文件，或 `yes | head -c N`）；
- 插件侧以 16 KiB 批量持续拉取到追平 + `pty:exit`；测量端到端耗时与消费侧追赶性；
- 意义：验证「生产端零暂停 + 拉取侧追赶」与 P2 测出的单字节成本在真实路径上的叠加无意外放大。

### P4 · 终端负载折算（结论判定）

按以下假设计算：
- 终端实时滚动典型：100 KB/s 连续输出（缓慢滚屏）；风暴：1 MB/s（`yes` / 日志刷屏 / `ssh-keygen` 类）。
- 当前桌面本地路径合并窗口 4 ms → 每窗口平均 400 B（100 KB/s）~ 4 KB（1 MB/s）。
- 插件路径若沿用 `PLUGIN_PTY_RING_FETCH_MAX_BYTES`=16 KiB 批量：100 KB/s → ~7 次/s；1 MB/s → ~64 次/s。
- 判定：单核 CPU 占比 = 每秒调用数 × 单次调用开销（P2 实测）；< 5% 视为可接受，10% 预警。

---

## 3. 通过标准（宽松门槛）

1. P2 单次 `ring-fetch` 往返 < 1 ms/op（数量级门；预期 µs 量级）；
2. P2 每字节成本相对 P1 倍数 < 100×（数据面；预期 < 10×）；
3. P4 折算 1 MB/s 风暴下插件消费路径单核占比 < 10%（硬件差异容忍，数据为准）；
4. 全部结论以贴档实测数据表达，不以估算断言。

---

## 4. 与在途工作的隔离

- 工作区存在外部 host-rust-residue 票线的未提交改动（`wasm_runtime.rs` 等 82 项在途）；
- 本探针仅：`mod tests` 内新增 `mod terminal_output_perf;` 一行（wasm_runtime.rs）+ 新增 `tests/terminal_output_perf.rs` + 本 spec/report；
- 提交建议：独立 commit，不与其他在途改动混提。

---

## 5. 验收

- `cd bedcode-desktop/src-tauri && cargo test --lib terminal_output_perf -- --test-threads 1` 通过；
- `report.md` 贴出 P1–P4 实测数据与结论；结论分「可迁 / 需先补批量策略 / 不可迁」三态。