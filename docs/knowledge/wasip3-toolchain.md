# wasip3 工具链（wasm32-wasip3 target）—— 桌面插件编译链决策与操作手册

Status: done（票 01：工具链落地；移动端评估见 §7）
Date: 2026-09-19
关联: `docs/adr/0019-wasmtime-version-locked-across-ends.md`（双端锁版）、
`docs/knowledge/wasmtime-guide.md`（wasmtime 升级）、`docs/knowledge/build-process.md`（构建命令与产物落点）

## 1. 背景与决策

认证中心插件（auth-center）与后续所有**桌面端**插件使用 **wasm32-wasip3（WASI 0.3）**
编译路径；移动端 2026-09-26 起同样运行 wasmtime 48，但构建链**仍为 `wasm32-unknown-unknown` + componentize**（wasip3 评估与待决策项见 §7）。

**为什么固定 nightly（决策）**：stable 1.98.1 无 wasm32-wasip3 预编译产物
（tier 2 low-tier，需 LLVM 23 + rustup 更新或源码构建）。自 2026-09-12 起的
nightly 已含 rust-std（证据：rust-lang.github.io/rustup-components-history/wasm32-wasip3.html），
spike（2026-09-19，nightly 1.100.0）验证编译链可用、零代码改动。

**固定版本**：`nightly-2026-09-16`（rustc 1.100.0-nightly, 215a8af4b）。
单一事实来源在 `scripts/wasip3-toolchain.sh`（`WASIP3_NIGHTLY`，可环境变量覆盖）。

**切换点（stable 1.99 发布后）**：nightly 自 2026-09-12 起 wasm32-wasip3 std
present，预计随 **stable 1.99（约 2026-10 中）** 合入。届时：
`rustup update stable && rustup target add wasm32-wasip3` → `scripts/wasip3-toolchain.sh`
去掉 nightly pin（`WASIP3_NIGHTLY` 改为 stable 语义）→ 命令字眼同步 AGENTS.md §3。
**在切换到 stable 之前，任何插件构建/测试都必须经该脚本指定的 pinned nightly。**

## 2. 关键事实（spike + 本票验证）

1. **wasip3 target 的 cdylib 直接输出 Component 组件**（magic `\0asm` + `0d 00 01 00`，
   core module 对应 `01 00 00 00`）——**免 componentize/wit-component 步骤**，构建链简化。
2. **零代码改动**：既有 4 个桌面插件（file-transfer / ai-chatbox / agent-hub / auto-task）
   均以 wasip3 target 直接编译通过（构建命令同形，仅换 target 与工具链）。
3. **import 面**：`bedcode:plugin/host-log`（宿主接口）+ 全套 `wasi:cli@0.3.0` /
   `wasi:clocks@0.3.0`；export 8 个 `bedcode:plugin` 接口与 unknown-unknown 同构
   （wasmtime 48.0.2 解析验证通过）。
4. **实例化边界**：产物 import wasi 0.3 接口，实例化需宿主 **p3 async linker（A0-3，
   票 02）**；当前 p2 sync 宿主**不可加载**——wasip3 产物仅作编译链验证，
   **不得替换 `resources/plugins/` 现行产物**（unknown-unknown / wasip2）。

## 3. 安装（镜像加速，可复现）

```bash
# 安装 pinned nightly + wasm32-wasip3 target（幂等）；
# 默认 USTC 镜像，可用 RUSTUP_DIST_SERVER / RUSTUP_UPDATE_ROOT 覆盖
scripts/wasip3-toolchain.sh install

# 校验已就绪（打印 rustc 版本 + target 列表）
scripts/wasip3-toolchain.sh verify
```

镜像：`RUSTUP_DIST_SERVER=https://mirrors.ustc.edu.cn/rust-static`（rsproxy.cn /
TUNA / 阿里云备选，官方源大文件下载极慢且断点续传有限）。脚本只在调用方
rustup 命令上注入镜像 env，不污染仓库其它构建。

## 4. 构建命令

```bash
# 最小 fixture（wasip3 编译链 + 产物 Component magic 校验）
scripts/wasip3-toolchain.sh fixture

# 存量插件 wasip3 零代码改动健康基线（4 个插件全部编译 + magic 校验）
scripts/wasip3-toolchain.sh health

# 手动构建任意桌面插件到 wasip3：
RUSTUP_TOOLCHAIN=nightly-2026-09-16 cargo build \
  --target wasm32-wasip3 --release --no-default-features --features wasm \
  --manifest-path bedcode-desktop/wasm-apps/<id>/rust/Cargo.toml
```

**产物落点（2026-09-26 起）**：夹具与 wasm 应用各自收敛到**单一共享 target 目录**，
不再写各自 crate 根的 `target/`（本仓库无根 workspace，per-crate target 会把相同的
依赖图重复编译 N 遍；实测 11 个夹具 6.0G + 4 个应用 5.8G）：

| 类别 | 共享目录 | 路径真源 |
| --- | --- | --- |
| 测试夹具（9 个 crate） | `bedcode-desktop/target/fixtures/` | `src-tauri/.../runtime/fixture_target.rs`（宿主测试经 `CARGO_TARGET_DIR` 注入）+ `packages/.cargo/config.toml`（手工 / 工具链探针） |
| wasm 应用（4 个） | `bedcode-desktop/target/wasm-apps/` | `scripts/plugin-wasm-config.mjs` 的 `WASM_TARGET_DIR`（`build.js` 显式传 `--target-dir`）+ `wasm-apps/.cargo/config.toml` |

因此上面两条手动命令的产物分别落在
`bedcode-desktop/target/fixtures/wasm32-wasip3/release/…` 与
`bedcode-desktop/target/wasm-apps/wasm32-wasip3/release/…`；
`wasip3-toolchain.sh fixture` / `health` 两个子命令也已指向共享目录（脚本内注释与上表互相指认）。
两个目录**刻意不合并**：夹具有 `[profile.release] opt-level="s"/lto=true` 而应用无
`[profile.*]`，profile 参与产物指纹，同目录会产出两份依赖产物。
治理方案与决策记录：`docs/knowledge/build-process.md`「Target 目录管理」节。

fixture 工程：`bedcode-desktop/packages/plugin-wasip3-test/`（`com.bedcode.wasip3-test`，
`wasip3-test.read-clock` 命令走 `wasi:clocks` import 作为时钟可读性静态证明；
票 02 将扩展 `wasi:random` async `get-random-bytes` 闭环）。

## 5. 健康基线（2026-09-19 实测）

| 插件 | wasip3 编译 | 产物（Component） |
| --- | --- | --- |
| file-transfer | ✅ 零代码改动 | 860K |
| ai-chatbox | ✅ 零代码改动 | 716K |
| agent-hub | ✅ 零代码改动 | 1.2M |
| auto-task | ✅ 零代码改动 | 1004K |

## 6. 边界与后续

- **宿主 async 化（A0-3）与实例化验证**：票 02 已完成（门禁通过）——CM_ASYNC +
  instantiate_async/call_async + p2 async adapter + p3 linker，cargo test 全绿。
- **构建链全量 wasip3（票 03 已完成）**：`resources/plugins/` 4 个桌面插件产物 +
  宿主测试 7 个 fixture 全部 wasip3 Component；CI（test.yml / release.yml 桌面
  job）新增 pinned nightly 安装步骤 + targets 收窄 `wasm32-wasip2`；
  mobile/移动端与 wasip2 preopen fixture（plugin-wasi-test）维持现状。
- **wasip3 的 thread_local 是真 TLS**（`target_thread_local`=true）：按宿主调用
  线程隔离——插件状态若存 thread_local，跨线程（投递线程写 / 查询线程读）会读空，
  须用实例级 static Mutex（实证：ws/sdk/system fixture 跨线程 TLS 四测全挂，改后全绿）。
- **CI 现状**：dtolnay stable（宿主）+ 单独步骤安装 `nightly-2026-09-16` +
  wasm32-wasip3 target（插件构建注入 RUSTUP_TOOLCHAIN）——stable 1.99 发布后
  移除 nightly pin。
- **测试命令**：本链相关插件/Rust 验证一律 `cargo test`（config 指定 manifest）/
  `pnpm run test:run` / 根目录 `pnpm exec eslint .`（AGENTS.md §3 字眼）。

---

## 7. 移动端 wasip3 评估（Phase B，待决策）

> 整理自 2026-09-26 移动端 wasmtime 48 + wasip3 评估专项 spec §4（2026-09-27 迁入）。
> Phase A（移动端 wasmtime 47→48，双端重新对齐）已于 2026-09-26 落地全绿；本节是
> **Phase B（wasip3）的现状事实、探针实证、收益/代价与待决策点**，待用户拍板再实施。

### 7.1 现状事实（桌面 vs 移动）

| 项 | 桌面 | 移动 |
| --- | --- | --- |
| 运行时 | wasmtime 48 + `wasmtime-wasi`(p3) | wasmtime 48，**无 wasmtime-wasi** |
| 引擎 | `wasm_component_model_async(true)` | 纯同步 |
| linker | `p2::add_to_linker_async` + `p3::add_to_linker` | 仅 12 组 `host-*` bindgen 接口 |
| 产物 target | `wasm32-wasip3`（cdylib 直出 Component，免 componentize） | `wasm32-unknown-unknown` + `componentize`（`wit-component =0.256.0`） |
| bindgen | `exports: { default: async }` | 同步导出 |
| 移动插件对 wasi 的实际使用 | 有（wasi:clocks 等，见 `packages/plugin-wasip3-test`） | **零**（3 个插件 `rg wasi` 无命中；无时钟/随机数/stdio/fs，全走 `host-*`） |

### 7.2 探针实证（2026-09-26 实测，非推测）

独立探针 crate（wasmtime 48.0.3 + `wasmtime-wasi{p3}`）**编译通过**下列接线——与桌面
完全同形：

```rust
config.wasm_component_model_async(true);           // 引擎级 CM_ASYNC
p3::add_to_linker(&mut linker);                     // wasi 0.3（func_wrap_async 注册）
let mut store = Store::new(engine, state);          // **仍是同步 Store**
linker.instantiate_async(&mut store, &component)    // 实例化走 async 入口
```

**关键发现（降低移动端成本）**：桌面 A0-3 计划里的 `Store::new_async` **最终没有采用**——
桌面保持同步 `Store::new`，只在实例化/调用两个入口走 async。因此移动端 Phase B 的真实改动面是
「**引擎配置 + 一个 linker 调用 + WasiView impl + 10 个导出调用点包一层 `block_on_async`**」，
而不是整条 async 迁移。`WasiView::ctx` 在 48 返回 `WasiCtxView`（结构体，含 `ctx` + `table`
两个字段），移动端需加 `WasiCtx` + `ResourceTable` 两字段并实现该 trait。

### 7.3 收益 / 代价 / 风险

| 收益 | 评价 |
| --- | --- |
| 构建链简化（免 componentize + 一份 wit-component 工具依赖） | 中等，但要动**已发布 SDK 的 CLI**（§7.4 B-1） |
| 插件拿到 `wasi:clocks` / `wasi:random` / `wasi:cli`（stdio） | **当前 3 个移动插件零需求**（无时钟靠 `host-config` 代理）。属「未来插件作者的体验改善」，非当下收益 |
| 与桌面构建链统一（一个 target、两份文档、一套 nightly pin） | 中等，降低长期分叉维护成本 |
| wasip3 的真 TLS 语义（`target_thread_local=true`） | **风险项**：插件若用 thread_local 存跨线程状态会读空（桌面踩过，见 §6） |

代价与风险：
1. **宿主体量**：`wasmtime-wasi`(p3) 把 wasi 实现（cap-std / system-interface / wit-*）拉进移动端
   二进制 → APK 体积增量**未实测**（需 Android 构建，磁盘成本高）。
2. **SDK 破坏**：`@binblink/bedcode-plugin-sdk-mobile` 的 CLI（npm 已发布）三处硬编码
   `wasm32-unknown-unknown` + componentize；改 target 等于要求第三方插件作者也装 pinned nightly
   （`nightly-2026-09-16`）才能构建。属**对外契约变更**。
3. **测试基建**：移动端测试内嵌 `cargo build --target wasm32-unknown-unknown` 建夹具，要同步注入
   `RUSTUP_TOOLCHAIN`；且 AGENTS §3 已记载「夹具构建必须走 rustup shim 的 cargo」这条坑。
4. **CI**：`test.yml` 的 `rust-mobile` job 要加 pinned nightly 安装 + target（桌面已有先例）。
5. **无需 p2 兼容垫片**：移动端既有 unknown-unknown 组件不 import 任何 wasi，只需 p3 linker
   （桌面因有 wasip2 插件才必须加 `p2::add_to_linker_async`）。**存量插件产物无需强制重建**。
6. **宿主调用 10 处 + bindgen 改 async**：`abi.version` / `activate` / `deactivate` / `invoke_command` /
   `on_terminal_input` / `on_terminal_output` / `on_startup` / `on_shutdown` / `on_message_binary` /
   `manifest`。同步→async 包裹经既有 `block_on_async`（重入/三线程路径已测），风险可控但需逐处 + 回归。

### 7.4 票拆分（对齐桌面经验）

| 票 | 内容 | 前置 |
| --- | --- | --- |
| B-1 工具链 | 移动端 `bedcode-mobile/plugins/*` 构建脚本 + `scripts/dev-run.js` + SDK CLI `bin/cli.js`（三处产物路径）改 `wasm32-wasip3` + `RUSTUP_TOOLCHAIN` 注入；文档同步 | 用户决策 |
| B-2 宿主 async 化 | 引擎 `CM_ASYNC` + `wasmtime-wasi{p3}` + `WasiView for WasmPluginState` + `bindgen!{exports:{default:async}}` + 10 处调用点 `block_on_async`；**保持 unknown-unknown 组件零回归**（本票不改产物） | B-1 |
| B-3 产物全量切换 + 集成验证 | 3 个移动插件 wasip3 产物替换、CI nightly 步骤、夹具构建器切 target、Android 真机回归 | B-2 |
| B-4 SDK 版本与发布 | `plugin-sdk-mobile` 的 npm + crates 版本上抬、`sdk-v*` tag 发布（`docs/knowledge/sdk-publish.md`）；或明确「旧 CLI 产物继续被宿主加载」并写入文档 | B-3 + 用户决策 |

### 7.5 待用户决策点

1. **是否现在做 Phase B**（建议：若移动端近期无新插件作者接入需求，可只停在 Phase A——wasip3
   收益目前全是「未来的」）；
2. **是否允许破坏已发布 SDK 的构建链**（B-1 必然破坏：第三方插件作者需装 pinned nightly）；
3. **Android 真机回归的排期**（cargo test 覆盖不到 JIT/缓存运行时行为——47 修的正是 Android
   aarch64 SIGILL；本次 48 升级同理需真机过一遍才敢发版）。

### 7.6 风险与回退（Phase A 已落地部分）

- **Android 运行时未验证**：wasmtime 47→48 跨 minor 无相关代码面改动（移动端无 wasi、无 preopen、
  JIT 路径同构），但 cargo test 覆盖不到真机 JIT/codegen 行为，发版前需真机跑插件加载 + 长会话。
- **patch 不对齐**（桌面 48.0.2 / 移动 48.0.3）：已论证无产物兼容风险（`.cwasm` 各端本地 cache）；
  ADR 0019 已把「锁声明范围、不锁 patch」写成显式条款。
- **回退**：`bedcode-mobile/src-tauri/Cargo.toml` 与 `Cargo.lock` 两处改回 `"47"` +
  `cargo update -p wasmtime` 即可（Phase A 无其他行为改动，回退面 = 2 个文件 + 注释/文档）。