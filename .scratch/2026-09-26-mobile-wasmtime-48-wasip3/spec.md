# 移动端 wasmtime 47 → 48 升级 + wasip3 评估

Status: **Phase A done**（2026-09-26 移动端 wasmtime 48 落地全绿，双端重新对齐）；
**Phase B（wasip3）待用户决策**（可行性已探针实证，方案与代价见 §4）
Date: 2026-09-26
范围: 移动端 `bedcode-mobile/`（宿主 + 构建链评估）；桌面端零代码改动
关联: `.scratch/2026-09-18-wasmtime-48-upgrade/spec.md`（桌面 48 升级，方法论来源）、
`docs/adr/0019-wasmtime-version-locked-across-ends.md`（双端锁死）、
`docs/knowledge/wasip3-toolchain.md`（桌面 wasip3 工具链）、
`.scratch/2026-09-21-a0-3-host-async/spec.md`（桌面宿主 async 化，本次 §4 的形态来源）

---

## 1. 目标

1. **Phase A（本次已完成）**：把移动端 wasmtime 从 47.0.3 升到 48.x（LTS），关闭 2026-09-18 的双端分叉，ADR 0019 恢复「两端锁死」。
2. **Phase B（本次只做评估 + 决策）**：按桌面端 A0-1/A0-3/A0-4 的经验评估移动端是否跟进 wasip3（`wasm32-wasip3` + p3 linker + 宿主 async 路径），产出票拆分与代价，**待用户拍板再实施**（涉及已发布 npm SDK 的构建链变更，见 §4.5）。

---

## 2. Phase A：wasmtime 47 → 48

### 2.1 决策

- 目标版本：`wasmtime = "48"`（caret → 48.0.3，48 线最新 patch；MSRV 1.95.0，47 为 1.94.0）
- **不**升级 `wit-component`（仍 `=0.256.0`）：组件编码格式向后兼容，与桌面端同一处置（桌面 spec §2.3）
- **不**新增 `wasmtime-wasi`：移动端宿主**从不依赖 WASI**（无 preopen、无 wasi import），桌面端那次 `DirPerms/FilePerms → FsPerms` 改造（48 的 wasi-filesystem 权限简化 #14010）在移动端**不存在对应面**
- **不**动 SDK 绑定：移动端 guest 侧仍 `wit-bindgen =0.60.0`（与桌面同版本）

### 2.2 桌面 spec §3 的 48 变更逐条在移动端的判定

| 48 变更 | 移动端影响 | 处置 |
|---|---|---|
| wasi 默认 deny TCP/UDP socket 创建（#13936） | 移动端不引入 wasi | ✅ 无影响 |
| wasi-filesystem 权限简化（#14010） | 无 `wasmtime-wasi` 依赖，无 preopen | ✅ 无影响（**与桌面不同，桌面改了 2 处代码**） |
| wasmtime-wasi-http wasip2/p3 统一（#13810/#13812） | 未依赖 | ✅ 无影响 |
| 需 Rust 1.95.0+（#13853） | 本机 1.98.1；CI `dtolnay/rust-toolchain@stable` | ✅ 满足（README 的 MSRV 声明 1.94 → 1.95 已同步 4 处） |
| 可变长 opcode 燃料成本（#13931） | 移动端有燃料看门狗（`FUEL_PER_CALL`，`component.rs` 每次调用前重置） | ✅ 语义不变（可选调优项，本次不动） |
| LinkerInstance reopen（#13908） | 未用 | ✅ 无影响 |
| async realloc 零化（#13949） | 移动端仍**同步 Store**，不涉及 | ✅ 无影响 |
| pooling allocator process_madvise（#13830） | Linux-only 性能项，移动端 Android | ✅ 无影响 |
| backtrace（`wasm_backtrace_max_frames`） | 移动端沿用 default features；`docs/knowledge/logging.md` 的「47.0.3 default features 已含 backtrace」表述随版本上抬 | ✅ 仅文档表述更新 |

**结论：移动端 47→48 零代码适配**（与桌面端的 2 处 `FsPerms` 改造形成对比——根因是移动端没有 WASI 面）。

### 2.3 改动清单

| 文件 | 改动 |
|---|---|
| `bedcode-mobile/src-tauri/Cargo.toml` | `wasmtime = "47"` → `"48"`，注释记录 ADR 0019 分叉沿革与 47 的 Android aarch64 JIT 修复延续性 |
| `bedcode-mobile/src-tauri/Cargo.lock` | 经 `cargo update -p wasmtime` 解析到 48.0.3（**禁手工编辑**） |
| `bedcode-mobile/src-tauri/src/plugin/wasm_runtime.rs` | 模块注释 2 处版本字样（bindgen 宏来源） |
| `bedcode-mobile/src-tauri/src/plugin/wasm_runtime/component.rs` | 模块注释：`bindgen!` 实际来源版本 `wasmtime-internal-wit-bindgen 47.0.3 / wit-parser 0.252` → `48.0.3 / 0.254`（**CRLF 文件，用 `open(newline='')` 改，行尾 1331/1331 保持**） |
| `bedcode-mobile/packages/plugin-sdk-mobile/rust/Cargo.toml` | wit-bindgen 锁注释（CRLF 文件，同上处置） |
| `AGENTS.md` §2 | 版本表行改为「双端 48（LTS）」+ 分叉关闭说明 |
| `docs/adr/0019-*.md` | 恢复「两端锁死」表述，补版本沿革表 + 锁的粒度（声明范围强约束 / lock patch 不强制相等，理由：`.cwasm` 写在各自设备 cache 目录、从不跨端复用）+ MSRV 1.95 |
| `docs/knowledge/wasip3-toolchain.md` | 「移动端不变（wasmtime 47 …）」→ 48 + 构建链仍 unknown-unknown |
| `README.md` / `README_en.md` / 两端 README（4 个） | wasmtime 徽标 47→48、MSRV 1.94→1.95（桌面 README 的 47 是 2026-09-18 桌面升级遗留的文档债，本次一并纠正） |
| `CHANGELOG.md` | 新增条目 |

### 2.4 执行记录（2026-09-26）

- 声明 `wasmtime = "48"`，lock 解析 **48.0.3**（桌面 48.0.2；ADR 0019 锁的是**声明范围**，patch 差异不产生产物兼容问题，理由写进 ADR）
- **零代码适配**：`cargo check --lib` 干净（0 error / 0 warning 增量）
- 全量 `cargo test`（移动端 `src-tauri`）：**347 lib + 1 + 17 + 7 + 14 + 1 集成 + 1 doc = 全绿，0 失败**（其中 wasm_runtime 29 项覆盖：组件往返、燃料 trap、ResourceLimiter 拒绝、AOT 缓存命中、abi 协商、SDK 宏产物加载并 activate）
- 未做：Android 真机运行验证（JIT/缓存稳定性是运行时属性，见 §6 风险）

---

## 3. Phase A 不做的事

- 不改移动端构建链（仍 `wasm32-unknown-unknown` + componentize）
- 不启用 CM_ASYNC / 不引入 `wasmtime-wasi`
- 不动移动端 WIT、ABI 版本、插件协议（跨端协议零变化）
- 不升 `wit-component` / SDK 版本 / npm 发布

---

## 4. Phase B：移动端 wasip3 评估（待决策）

### 4.1 现状事实

| 项 | 桌面 | 移动 |
| --- | --- | --- |
| 运行时 | wasmtime 48 + `wasmtime-wasi`(p3) | wasmtime 48，**无 wasmtime-wasi** |
| 引擎 | `wasm_component_model_async(true)`（票 02） | 纯同步 |
| linker | `p2::add_to_linker_async` + `p3::add_to_linker` | 仅 12 组 `host-*` bindgen 接口 |
| 产物 target | `wasm32-wasip3`（cdylib 直出 Component，免 componentize） | `wasm32-unknown-unknown` + `componentize`（`wit-component =0.256.0`） |
| bindgen | `exports: { default: async }` | 同步导出 |
| 移动插件对 wasi 的实际使用 | 有（wasi:clocks 等，see `packages/plugin-wasip3-test`） | **零**（3 个插件 `rg wasi` 无命中；无时钟/随机数/stdio/fs，全走 `host-*`） |

### 4.2 探针实证（本次实测，非推测）

独立探针 crate（`/tmp/w3probe`，wasmtime 48.0.3 + `wasmtime-wasi{p3}`）**编译通过**下列接线——与桌面票 02 完全同形：

```rust
config.wasm_component_model_async(true);           // 引擎级 CM_ASYNC
p3::add_to_linker(&mut linker);                     // wasi 0.3（func_wrap_async 注册）
let mut store = Store::new(engine, state);          // **仍是同步 Store**
linker.instantiate_async(&mut store, &component)    // 实例化走 async 入口
```

**关键发现（降低移动端成本）**：桌面 A0-3 计划里的 `Store::new_async` **最终没有采用**——桌面保持同步 `Store::new`，只在实例化/调用两个入口走 async（`component.rs` 票 02 注释 + probe 报告）。因此移动端 Phase B 的真实改动面是「**引擎配置 + 一个 linker 调用 + WasiView impl + 10 个导出调用点包一层 `block_on_async`**」，而不是整条 async 迁移。

`WasiView::ctx` 在 48 返回 `WasiCtxView<'_>`（结构体，含 `ctx` + `table` 两个字段），移动端 `WasmPluginState` 需加 `WasiCtx` + `ResourceTable` 两字段并实现该 trait（桌面 `runtime.rs:189-196` 是现成模板）。

### 4.3 收益评估

| 收益 | 评价 |
| --- | --- |
| 构建链简化（免 componentize 步骤 + 一份 wit-component 工具依赖） | 中等收益，但要动**已发布 SDK 的 CLI**（§4.5） |
| 插件拿到 `wasi:clocks` / `wasi:random` / `wasi:cli`（stdio） | **当前 3 个移动插件零需求**（无时钟靠 `host-config` 代理，见 `plugins/file-transfer/rust/src/transfer_store.rs:8`）。属于「未来插件作者的体验改善」，非当下收益 |
| 与桌面构建链统一（一个 target、两份文档、一套 nightly pin） | 中等收益，降低长期分叉维护成本 |
| wasip3 的真 TLS 语义（`target_thread_local=true`） | **风险项**而非收益：插件若用 thread_local 存跨线程状态会读空（桌面踩过，ws/sdk/system 夹具四测全挂，见 wasip3-toolchain.md §6） |

### 4.4 代价与风险

1. **宿主体量**：`wasmtime-wasi`(p3) 把 wasi 实现（cap-std / system-interface / wit-*）拉进移动端二进制；桌面已有，移动端是新增 → APK 体积增量**未实测**（需 Android 构建，24G 可用磁盘下成本高）。
2. **SDK 破坏**：`@binblink/bedcode-plugin-sdk-mobile` 的 CLI（npm 已发布）三处硬编码 `wasm32-unknown-unknown` + componentize；改 target 等于要求第三方插件作者也装 pinned nightly（`nightly-2026-09-16`）才能构建。属**对外契约变更**。
3. **测试基建**：移动端测试内嵌 `cargo build --target wasm32-unknown-unknown` 建夹具（`component.rs` 的 `build_test_component` / `build_real_plugin_component`），要同步注入 `RUSTUP_TOOLCHAIN`；且 AGENTS §3 已记载「夹具构建必须走 rustup shim 的 cargo」这条坑，移动端会首次继承。
4. **CI**：`.github/workflows/test.yml` 的 `rust-mobile` job 要加 pinned nightly 安装 + target（桌面 job 已有先例）。
5. **无需 p2 兼容垫片**：移动端既有 unknown-unknown 组件不 import 任何 wasi，故只需 p3 linker（桌面因有 wasip2 插件才必须加 `p2::add_to_linker_async`）。**存量插件产物无需强制重建**——这是移动端比桌面便宜的一点。
6. **宿主调用 10 处 + bindgen 改 async**：`abi.version` / `activate` / `deactivate` / `invoke_command` / `on_terminal_input` / `on_terminal_output` / `on_startup` / `on_shutdown` / `on_message_binary` / `manifest`。同步→async 包裹经既有 `block_on_async`（重入/三线程路径已测），风险可控但需逐处 + 回归。

### 4.5 票拆分（对齐桌面 票 01/02/03 经验）

| 票 | 内容 | 前置 |
| --- | --- | --- |
| B-1 工具链 | 移动端 `wasm-apps/plugins` 等价物（`bedcode-mobile/plugins/*`）构建脚本 + `scripts/dev-run.js` + SDK CLI `bin/cli.js`（三处产物路径）改 `wasm32-wasip3` + `RUSTUP_TOOLCHAIN` 注入；`plugin-dev-mobile.md` / README 文档同步 | 用户决策 |
| B-2 宿主 async 化 | 引擎 `CM_ASYNC` + `wasmtime-wasi{p3}` + `WasiView for WasmPluginState` + `bindgen!{exports:{default:async}}` + 10 处调用点 `block_on_async`；**保持 unknown-unknown 组件零回归**（本票不改产物） | B-1 |
| B-3 产物全量切换 + 集成验证 | 3 个移动插件 wasip3 产物替换 `resources/plugins/mobile`、CI nightly 步骤、夹具构建器切 target、Android 真机回归（cargo test 全绿 + gradlew） | B-2 |
| B-4 SDK 版本与发布 | `plugin-sdk-mobile` 的 npm + crates 版本上抬、`sdk-v*` tag 发布（`docs/knowledge/sdk-publish.md`）；或明确「旧 CLI 产物继续被宿主加载」并写入文档 | B-3 + 用户决策 |

### 4.6 待用户决策点

1. **是否现在做 Phase B**（建议：若移动端近期无新插件作者接入需求，可只停在 Phase A——wasip3 收益目前全是「未来的」）；
2. **是否允许破坏已发布 SDK 的构建链**（B-1 必然破坏：第三方插件作者需装 pinned nightly）；
3. **Android 真机回归的排期**（cargo test 覆盖不到 JIT/缓存运行时行为——47 修的正是 Android aarch64 SIGILL；本次 48 升级同理需真机过一遍才敢发版）。

---

## 5. 验证（Phase A 完成定义）

| 项 | 命令 | 结果 |
| --- | --- | --- |
| 编译 | `cd bedcode-mobile/src-tauri && cargo check --lib` | ✅ 干净，零代码适配 |
| 单元 + 集成测试 | `cd bedcode-mobile/src-tauri && cargo test` | ✅ 347 + 1 + 17 + 7 + 14 + 1 + 1 全绿，0 失败 |
| wasmtime 专项 | `cargo test --lib wasm_runtime` | ✅ 29/29（含组件加载 / 燃料 trap / limiter / AOT / abi 协商 / SDK 宏产物） |
| 前端 | 未改前端 | 跳过（无前端文件变更） |
| 格式 | `cargo fmt --check` | 见 §7 执行记录 |
| 文档一致性 | AGENTS §2 / ADR 0019 / wasip3-toolchain.md / 4 个 README / CHANGELOG | ✅ 同步 |
| Phase B 探针 | `/tmp/w3probe`（wasmtime 48 + wasi p3 接线编译） | ✅ 通过 |

---

## 6. 风险与回退

- **Android 运行时未验证**：wasmtime 47 修的是 Android aarch64 JIT/缓存 SIGILL（46 时代），本次跨 minor 到 48 虽无相关代码面改动（移动端无 wasi、无 preopen、JIT 路径同构），但**编译与 cargo test 都覆盖不到 JIT/codegen 的真机行为**。发版前需真机跑一遍插件加载 + 长时间会话（建议列入 §4.6 决策点 3）。
- **patch 不对齐**（桌面 48.0.2 / 移动 48.0.3）：已论证无产物兼容风险（`.cwasm` 各端本地 cache）；ADR 0019 已把「锁声明范围、不锁 patch」写成显式条款。
- **回退**：`bedcode-mobile/src-tauri/Cargo.toml` 与 `Cargo.lock` 两处改回 `"47"` + `cargo update -p wasmtime` 即可（本次无其他行为改动，回退面 = 2 个文件 + 注释/文档）。注意 §11：工作区有在途改动时**禁止整文件 `git checkout`**，用 edit 精确还原本次内容。
- **clippy 非门禁的两条既有告警**：`component.rs:625` `clone_on_copy`（`TypedFunc` 在 47/48 **都是** `Copy`，与本次升级无关）、`:627` `useless_format`——均为既有代码，未顺手改（最小改动原则）。

---

## 7. 后续

- **Phase B B-1~B-4**：待用户决策（§4.6）
- **Android 真机回归**：Phase A 发版前必做（§6）
- **ADR 0019**：已恢复锁死表述并补沿革；若 Phase B 落地，追加「构建链 target 不受本 ADR 约束」条款（已写入）
- **stable 1.99 切换点**（桌面 `wasip3-toolchain.md` §1 记录的 nightly pin 移除条件）不影响移动端当前形态；若 Phase B 落地则移动端需同批处理

## 8. 执行记录补充（2026-09-26）

- Phase A 全量验证如 §5；`cargo fmt` 未引入格式差异（见提交前自查输出）
- 磁盘：`bedcode-mobile/src-tauri/target` 12.68 GB（阈值 15 GB，未触发 `cargo clean`；升级后 wasmtime 48 依赖图重编译后仍在此量级）
- 未做 Phase B 的任何代码改动（仅 `/tmp` 探针，未污染仓库）
