# 2026-09-26 · Cargo 编译空间治理（多 crate 共享 target）

## 1. 问题

仓库内 30+ 个 `Cargo.toml` 但**无根 workspace**：每个 crate 都是独立 workspace，
`cargo build --manifest-path` 未指定 target 目录时产物落进**各自 crate 根的 `target/`**。
后果是依赖图被重复编译、重复占盘。

2026-09-26 04:30 实测（本机 157G 盘，ext4）：

| 位置 | 占用 |
| --- | --- |
| `bedcode-desktop/src-tauri/target` | 6.5G |
| `bedcode-mobile/src-tauri/target` | 8.5G |
| `bedcode-mobile/plugins/auto-task/target` | 174M |
| `bedcode-mobile/packages/plugin-component-test/target` | 173M |
| `bedcode-desktop/wasm-apps/terminal-session/rust/target` | 150M |
| **合计** | **15.3 GiB**（剩余空间 35G） |

历史峰值（2026-09-26 凌晨审计）：桌面端 **17 个 target 目录共 15.8G**，
其中 11 个夹具 crate 占 **6.0G**（每 crate 429M~836M）、4 个 wasm 应用占 **5.8G**。

## 2. 根因分析

### 2.1 重复编译发生在哪

夹具 crate 的依赖图**几乎完全相同**：`bedcode-plugin-api`（SDK）→ `wit-bindgen` +
`bedcode-plugin-api-macros`（proc-macro crate，须按宿主三元组编译）→ `serde` / `anyhow` / `inventory`。
11 个夹具 crate 各自一份 target，等于把这份图编译 11 遍。

wasm 应用同理：4 个应用都依赖同一个 SDK，SDK 的 wasm 产物与 proc-macro 宿主产物重复 4 遍。

### 2.2 为什么"两端宿主共享"收益低（先排除的方案）

- **wasmtime 版本分叉**：桌面 48 / 移动 47（ADR 0019）。cargo 按版本分产物，
  最大的依赖块（wasmtime + cranelift + wasmparser，各端 ~2G）**无法共享**。
- **目标三元组不同**：移动端产物是 `aarch64-linux-android`。
- **feature 集不同**：两端 `tauri` / `tauri-plugin-*` 特性集不同，解析结果不同的依赖不共享。

结论：宿主侧 14G 是"活产物"（`deps/` 里几乎每个 crate 只有 1 个哈希版本，
陈旧残留仅 ~10M，`cargo-sweep` 收益可忽略），**不是垃圾，是必要成本**。
真正的浪费在 12~15 个小独立 target 上。

### 2.3 三个已实测验证的前提

| # | 结论 | 验证方式 |
| --- | --- | --- |
| 1 | `cargo test` 运行测试期间**不持有** target 目录锁 | 探针 crate：测试睡眠 25s 期间并发 `cargo build` 真实重编译 0.04s 完成，无 "Blocking waiting for file lock" |
| 2 | cargo 配置按 **cwd 祖先链**发现，与 `--manifest-path` 无关 | 在 `X/.cargo/config.toml` 配 `target-dir`，从 `X/app` 构建→生效；从 `/tmp` 构建同一 manifest→不生效 |
| 3 | `build.target-dir` 相对值以 **`.cargo/` 所在目录**为基准 | 同上探针：`.cargo/config.toml` 写 `target-dir = "shared-target"`，cwd=`X/app` 时产物落 `X/shared-target` |

前提 1 决定了夹具共享 target **不会**与父 `cargo test` 死锁。

## 3. 方案与决策

| 方案 | 预计回收 | 决策 |
| --- | --- | --- |
| A 夹具共享 target | 6.0G → ~1G | **采纳** |
| B 桌面 wasm 应用共享 target | 5.8G → ~4G | **采纳** |
| C 移动端夹具共享 target | ~0.35G → ~0.2G | **采纳**（低成本，同构） |
| D 两端宿主共享 target | 估 2~3G | **不做**：编译期独占锁使并发构建串行化；`cargo clean` 爆炸半径覆盖全端；tauri/gradle/脚本/文档路径假设全要改；且 §2.2 已证去重空间有限 |
| E 单一根 workspace | 增量有限 | **不做**：单一 `Cargo.lock` 耦合 wasmtime 47/48 分叉（ADR 0019 双端偏离条款）；stable vs nightly 工具链冲突；workspace feature 统一会污染 wasm 产物；`cargo build --workspace` 会试图按宿主三元组编译 wasm 应用 |
| F sccache | 不省空间 | **不做**：sccache 不缓存增量编译单元（需 `CARGO_INCREMENTAL=0`），dev 迭代反而更慢，且自占缓存空间。仅 CI 适用 |
| G btrfs + compress=zstd | 14G → 5~7G | **不做**：需独立分区，loop 挂载性能损失不可接受 |
| H 定期回收 | 立即 ~4G | **采纳**（Step 0 + 监控脚本扩展） |

### 3.1 目录布局决策

夹具与 wasm 应用**分开两个共享目录**，不合并：

- `bedcode-desktop/target/fixtures` —— 9 个夹具 crate
- `bedcode-desktop/target/wasm-apps` —— 4 个 wasm 应用
- `bedcode-mobile/target/fixtures` —— 2 个夹具/plugin

不合并的**硬理由**：夹具 crate 在各自 `Cargo.toml` 里定义了
`[profile.release] opt-level = "s" / lto = true`，而 4 个 wasm 应用**没有** `[profile.*]`
（用 cargo 默认）。profile 参与 cargo 产物指纹 → 同目录内会为同一份依赖图产出**两份**产物，
合并反而多占空间、且掩盖去重效果。

不并入宿主 `src-tauri/target` 的理由：`cargo clean`（含 `check-target-size.js` 超阈值自动清理）
只针对该目录，混在一起会让 15G 阈值统计失真，且清宿主缓存时连带清掉夹具缓存。

## 4. 实施清单

- [x] Step 0：删除两端 `debug/incremental`（3.9G）+ 旧 per-crate target（~500M）
- [x] Step 1A：`runtime/fixture_target.rs` 共享模块 + **12 处**调用点改用
      （`runtime.rs` ×8、`runtime/component.rs`、`runtime/tests/p3_async_host_import.rs`、
      `host/tests/wasm_flow_test.rs`、`host/tests/system_component_test.rs`）
- [x] Step 1B：`wasm-apps/.cargo/config.toml` + 4 个 `scripts/build.js`（`--target-dir` +
      产物路径常量），真源加进 `scripts/plugin-wasm-config.mjs` 的 `WASM_TARGET_DIR`
- [x] Step 1C：移动端 2 处夹具构建（`component.rs` `fixture_target_dir()`，CRLF 安全改写）
- [x] Step 1D：`scripts/wasip3-toolchain.sh` 的 `fixture` / `health` 两个子命令
- [x] Step 1E：`check-target-size.js`（两端）扩展为列出共享 / 遗留 target 目录
- [x] Step 1F（实施中发现并补做）：`packages/.cargo/config.toml`（桌面 + 移动）
- [x] Step 2：文档同步（`build-process.md` / `wasip3-toolchain.md` / `AGENTS.md` §3 / `CHANGELOG.md`）
- [x] Step 3：`terminal-session` 插件契约测试同步（见 §6）

### 4.1 实施中发现的新事实（补做 Step 1F 的依据）

首轮只改了脚本路径，实测发现 **clippy / rust-analyzer 之类的工具链探针会在应用 crate
目录内跑 `cargo check`**，绕过 `build.js` 显式传的 `--target-dir`，在
`wasm-apps/terminal-session/rust/target/debug/` 静默重建了 **629M** per-crate 目录。
补 `packages/.cargo/config.toml` 与 `wasm-apps/.cargo/config.toml`（cargo 按 cwd 祖先链
发现配置，实测）后，手工构建与工具链探针都落到共享目录，已验证
`cd packages/plugin-component-test && cargo build` 产物落在
`target/fixtures/wasm32-wasip3/release/`，且该 crate 下不再出现 `target/`。
两份 config 均**不影响宿主构建**（`packages/` / `wasm-apps/` 不是 `src-tauri/` 的祖先）。

## 5. 验证结果（2026-09-26）

| 项 | 结果 |
| --- | --- |
| 夹具共享（桌面） | 9 夹具 → 1 目录 **417M**；7 个域（component/sdk/pty/ws/http/task/wasip3）过滤测试全绿 |
| 夹具共享（移动） | 2 夹具 → 1 目录 **334M**；`cargo test` **400 passed / 0 failed** |
| wasm 应用共享 | 4 应用 → 1 目录 **150M**；`build.js --rust-only` 与 `wasip3-toolchain.sh health/fixture` 均落新目录、magic 校验通过 |
| 手工 / 工具链路径 | `cd packages/plugin-component-test && cargo build` 落共享目录，无 per-crate 残留 |
| 桌面 `cargo test --no-fail-fast` | 890 passed + 集成 9 项，仅 2 项失败（**均非本任务**，见 §6） |
| 前端 | 桌面 **844 passed / 87 文件**；移动 **467 passed / 49 文件** |
| `pnpm exec eslint .` | **0 errors**（120 warnings 为既有） |
| prettier | 改动文件全部通过（`bedcode-mobile/scripts/check-target-size.js` 的既有 trailing-comma 未动，见 §7） |
| `lens_diagnostics mode=all` | 无 error |
| 磁盘 | Step 0 即时回收 **3.9G**（15.3G → 11.4G，剩余空间 35G → 39G） |

> 注：桌面宿主 target 在验证期间因重跑 `cargo test` 回升（`test` profile 依赖产物与
> `incremental` 重新生成，合计约 +3G），属正常成本而非泄漏；`incremental` 可随时删除。

## 6. 两项失败的责任归属（均非本任务引入）

`ws_e2e::test_session_control_endpoint_direct_roundtrip` 与
`tests/pty_session_chain.rs::pty_session_chain_flow` 失败签名完全相同：

```
start 回包类型, got: {"event":"session:created", ... , "type":"event"}   // 期望 "start_session"
```

根因是**并发 agent 的在途改动**：`wasm-apps/terminal-session/rust/src/session/events.rs`
的 `publish()` 新增了 `crate::ws_events::broadcast_event(&WasmHost, event_name, payload)`
（移动端适配专项票 02），`session:created` 事件因此先于动作回包到达 `session-control`
端点。两个用例都在「收到首帧即断言动作回显」处失败。
后者的断言宿主链路会加载真实内置产物 `com.bedcode.terminal-session`，而该产物被本次
`build.js --rust-only` 重建（产物在 `.gitignore` 内，属构建输出，对方的改动落地时
同样要重建）。**未擅自修**——它属于对方票的范围，改断言会与其工作冲突。
另注：首轮全量跑时 `pty_e2e` 2 项偶发失败、二轮不复现，属首次并发编译全部夹具
（`jobs=4`）下的时序抖动，单独跑与二轮全量跑均通过。

## 7. 刻意未做 / 已知遗留

- **移动端插件 target 未共享**：发布态 SDK CLI（`packages/plugin-sdk-mobile/bin/cli.js`）
  在三处硬编码 `rust/target/...` 产物路径，改动需连带 SDK 评估（约 500M 量级）。
  `bedcode-mobile/packages/.cargo/config.toml` 的注释已记录该边界。
- **`bedcode-mobile/scripts/check-target-size.js` 的既有 prettier 差异**（`getDirectorySize`
  里 `{ encoding: 'utf-8' }` 少尾逗号）未修：那是 HEAD 已有的行，且该文件是全 CRLF
  （仓库无 `.gitattributes`），跑 `prettier --write` 会把 177 行整体转成 LF、制造全文件
  diff——正是本仓库 CRLF 事故的成因，故只对我改动的区域做了 CRLF 安全改写。
- **`[profile.release.build-override]`（宿主导 proc-macro 降优化级别）未做**：
  共享目录里 `release/build`（wasmparser / wit-parser / syn 等宿主导依赖）占 376M，
  理论上 `opt-level = 0` 可再省数百 M，但需改 13 个 Cargo.toml 的 profile，
  超出本次「目录收敛」范围，留作后续评估项。
