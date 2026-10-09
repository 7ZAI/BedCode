# 票 01 · POC：WIT 分片 + 组合等价 + 无桌面依赖编译实证

Status: **todo**（spec `.scratch/2026-10-09-wasm-core-single-crate/spec.md` 票序第一票）
依赖：无
原则：**零生产改动**——全部验证在 `/tmp/wit-slice-poc/` 临时 crate 与只读检查里完成，不触碰工作区在途改动（开工前 `git status` 已有多任务在途文件）。

## 1. 要证伪/证实的四个命题

| # | 命题 | 不成立则 |
| --- | --- | --- |
| **P1** | 能力域 crate 可以 bindgen「自持的 WIT 分片（`world cap-pty`）」，生成的 `Host` trait 与 `add_to_linker` 对等于「bindgen 整份端 WIT（`world plugin`）」 | D4 不成立 ⇒ 能力域永远绑死端 WIT ⇒ 改用「核心 WIT 由 wasm-core 单独维护、能力域仍绑端 WIT」的退化形态（差异不只来自组合） |
| **P2** | 端 world 用 `include core; include cap-pty; …` 组合后，其 import/export 集合与 HEAD 扁平 `world plugin` **逐项等价** | D2 不成立 ⇒ 同 package 拼装方案作废，须走跨 package + `deps/`（ABI 破坏、产物重建） |
| **P3** | `packages/bedcode-wasm-core` 在 `--no-default-features`（无桌面能力域）下可编译，且 `cargo tree` 不含 `portable-pty` / `actix-web` / `webkit2gtk` / `dbus` | D5 不成立 ⇒ 单一 crate 在移动侧编不过 ⇒ 须先把平台依赖彻底外迁（新增前置票） |
| **P4** | 拼装脚本可从「wasm-core `wit/core.wit` + 能力域 `wit/*.wit`」幂等生成端 `wit/` 目录 | D1 不成立 ⇒ 生成物方案作废，改手写 + 漂移锁 |

## 2. 步骤

### 2.0 前置

```bash
df -h                      # 磁盘常紧张；wasmtime 编译需要余量
which wasm-tools || echo "无 CLI ⇒ P2 用 wit-parser 临时 crate 替代"
```

### 2.1 P1 · 分片 bindgen 对等（临时 crate，零生产改动）

```bash
mkdir -p /tmp/wit-slice-poc && cd /tmp/wit-slice-poc
```

建一个最小 crate（`Cargo.toml` 依赖 `wasmtime = { version = "48", features = ["component-model"] }`、`wit-bindgen = "=0.60.0"`），`src/lib.rs` 含两次 `bindgen!`：

```rust
// A：现状形态 —— 绑整份端 WIT（只读引用，不改工作区文件）
wasmtime::component::bindgen!({
    path: "<repo>/bedcode-desktop/packages/plugin-sdk-desktop/rust/wit/bedcode.wit",
    world: "plugin",
    exports: { default: async },
});

// B：目标形态 —— 绑自持分片（从 A 的 host-pty interface 段原样复制到 poc/wit/pty.wit，
//    追加 `package bedcode:plugin;` 与 `world cap-pty { import host-pty; }`）
wasmtime::component::bindgen!({
    path: "wit/pty.wit",
    world: "cap-pty",
});
```

判据（`cargo test` 内断言，不用肉眼看）：
1. 两次 bindgen 编译通过；
2. `host_pty::Host` 的**方法名集合**在 A / B 中一致（用 `std::any` 无法反射 ⇒ 改为**编译期探针**：分别写 `impl A_host_pty::Host for S` 与 `impl B_host_pty::Host for S`，两者方法签名逐项同形即编译通过 —— 签名不同会编译红，红即证伪）；
3. 两个 `host_pty::add_to_linker::<S, D>` 均可被调用，且**分别**注册到两个 Linker 时不冲突（验证「只注册自己那一个 interface」的分片装配面成立）。

### 2.2 P2 · 组合等价（临时目录，零生产改动）

在 `/tmp/wit-slice-poc/wit/` 里手搭一份「组合版」桌面 WIT：

- `core.wit`：`package bedcode:plugin;` + HEAD `bedcode.wit` 的**全部内容**，仅把 `world plugin` 改名 `world core`
- `pty.wit`：`package bedcode:plugin;` + `interface host-pty`（自包含，无跨 interface `use`）+ `world cap-pty { import host-pty; }`
- `bedcode.wit`（组合版）：`world plugin { include core; include cap-pty; }`

导出两端 world 的 import/export 集合并 diff：

- 有 `wasm-tools` CLI：`wasm-tools component wit <dir-or-file> --json`（两处各跑一次，`jq` 排序后 `diff`）
- 无 CLI：临时 crate 依赖 `wit-parser = "0.256"`（wasmtime 48 同版本），`Resolve::push_dir` + 遍历 `world.imports / world.exports`，打印排序后的 `interface@package` 列表并 diff

**判据**：组合版 `world plugin` 与 HEAD `world plugin` 的 import/export 列表 **diff 为空**。

### 2.3 P3 · wasm-core 无桌面依赖编译（只读检查）

```bash
cd <repo>/packages/bedcode-wasm-core
cargo check --no-default-features            # 先 host target
cargo tree --no-default-features | rg -n "portable-pty|actix-web|webkit2gtk|dbus|bedcode-plugin-api" || echo "归零"
```

- **判据**：`cargo check` 通过（或记录首红点名：`webkit2gtk` / `dbus` 的 `cfg(target_os = "linux")` 在 Android 同 `target_os` 的撞车点）；`cargo tree` 中桌面能力域与桌面 SDK 归零（对齐 `packages/bedcode-headless-host-probe` 既有判据）。
- Android target 实证（需 NDK）：POC 阶段**可选**，`cargo check --no-default-features --target aarch64-linux-android`；跑不了就在门禁里写「没跑 + 原因」。

### 2.4 P4 · 拼装脚本原型

在 `/tmp/wit-slice-poc/` 写 `compose.mjs`（或 shell 原型），输入 = 端清单（域名列表），输出 = 端 `wit/` 目录（core + 各 cap + 端 world 文件）。

判据：
1. 连续跑两次输出**零 diff**（幂等）；
2. 输出目录与分片真源**逐字一致**（漂移锁原型：逐文件 sha256 比对）；
3. 输出喂给 2.2 的等价对比 ⇒ 与 HEAD 等价。

## 3. 门禁（POC 收尾）

| 项 | 要求 |
| --- | --- |
| P1 | 临时 crate `cargo test` 绿（两次 bindgen + 双 impl 探针 + 双 add_to_linker） |
| P2 | 组合前后 world import/export 列表 diff 为空 |
| P3 | `cargo check --no-default-features` 通过 + `cargo tree` 归零（或在途首红点名） |
| P4 | 脚本幂等 + sha256 逐字一致 |
| 回归 | **零生产改动** ⇒ 无回归面；`git status` 不应出现本票新增/修改的生产文件 |

## 4. 收尾产出（写入 spec / 票文档）

1. P1–P4 的**实际运行结果**（贴命令与输出，跑不了写原因与风险）
2. 四个命题的成立/不成立结论 + 证据路径（临时 crate 保留路径）
3. 若 P2 不成立：跨 package 方案的代价重估（import 改名 → ABI bump + 产物重建清单）
4. 若 P3 不成立：平台依赖外迁的前置票清单（点名 `webkit2gtk` / `dbus` / `cfg(target_os)` 撞车点）
5. spec §3 D3 的「POC 后复评」结论：交集切片（拆 interface，ABI bump） vs 只放 11 个全等（零拆分）

## 5. 风险与回退

- **磁盘 / 编译时长**：wasmtime 48 首次编译慢；`CARGO_INCREMENTAL=0` 控占用；跑前 `df -h`，必要时清 `target/debug/incremental`（`cargo clean` 不需审批）。
- **回退**：POC 全在 `/tmp`，`rm -rf /tmp/wit-slice-poc` 即完全回退；生产文件零改动。
- **不与在途任务抢文件**：本票不修改 `bedcode-desktop/src-tauri`、`packages/bedcode-*`、`plugin-sdk-*` 任何文件（P3 是只读 `cargo check`，会写 `target/` 但不改源码）。

## 6. 实施记录

### P2 · 组合等价 —— **已证（2026-10-09）**

构造（脚本 `/tmp/wit-slice-poc/build_poc_wit.py`，零生产改动）：

- `wit-head/bedcode.wit`：HEAD 原样复制（`package bedcode:plugin;`，21 import / 5 export 扁平 world）
- `wit-combined/core.wit`：HEAD 全量 − `interface host-pty` 段（第 692–732 行，41 行）− `import host-pty;` 行，`world plugin` 改名 `world core`
- `wit-combined/pty.wit`：`package bedcode:plugin;` + `interface host-pty` 原段 + `world cap-pty { import host-pty; }`
- `wit-combined/bedcode.wit`：`world plugin { include core; include cap-pty; }`

对比（`/tmp/wit-slice-poc/parser`，`wit-parser 0.254.2`，`Resolve::push_dir` 遍历 `world.imports/exports`）：

```text
HEAD     items = 26
COMBINED items = 26
--- DIFF ---            （空）
EQUIVALENT = true
```

**结论**：同 package 分片 + `include` 组合，**world 成员逐项等价**，`host-pty` 的 import 名仍为 `bedcode:plugin/host-pty` ⇒ 分片本身**不引入 ABI 破坏**（破坏只来自 D3 的 interface 拆分，与分片机制无关）。

副产物事实：`host-pty` interface 定义自包含（无跨 interface `use`）⇒ 能力域分片无需 `deps/` 目录。

### P3 · 无桌面依赖编译 —— **编译绿，但依赖图红（2026-10-09）**

```text
cd packages/bedcode-wasm-core
cargo check --no-default-features --offline   →  Finished in 14.47s（6 warnings，0 error）
cargo tree --no-default-features | grep -cE 'bedcode-plugin-api|bedcode-server-*|bedcode-pty-engine|bedcode-discovery-engine|portable-pty|actix-web|webkit2gtk|dbus'  →  28
```

命中清单（节选）：`bedcode-plugin-api`（桌面 SDK）、`bedcode-{pty-engine,server-http,server-websocket,server-peer-net,discovery-engine}`、`portable-pty`、`actix-web`、`webkit2gtk` / `webkit2gtk-sys`、`dbus` / `libdbus-sys`。

**根因**：`packages/bedcode-wasm-core/Cargo.toml` 里五个能力域依赖是**必选**且被强制 `features = ["desktop-host"]`；`bedcode-plugin-api`（桌面 SDK）必选；`webkit2gtk` / `dbus` 位于 `[target.'cfg(target_os = "linux")'.dependencies]` 且必选（Android 同 `target_os` ⇒ 必然撞）。

**结论**：P3 命题「无桌面依赖」**当前不成立**（编译能过只是因为 host 是 linux）。⇒ **新增票 02「wasm-core 依赖图脱桌面」** 作为后续一切（票 03/06）的硬前置；判据 = `cargo check` 绿 **且** `cargo tree` 命中归零（对齐 `packages/bedcode-headless-host-probe` 既有零 wasmtime / 零 SDK 判据）。Android target 实证（`--target aarch64-linux-android`，需 NDK）纳入票 02。

### P4 · 拼装脚本原型 —— **已证（2026-10-09）**

`/tmp/wit-slice-poc/compose.py --manifest manifest.json --out <dir>`：

```text
两次输出的 sha256 清单 diff            → 空（幂等 OK）
out1/core.wit   vs 真源 core.wit       → 空（逐字一致）
out1/cap-pty.wit vs 真源 pty.wit       → 空（逐字一致）
生成的端 world                          → world plugin { include core; include cap-pty; }
```

### P1 · 分片 bindgen 对等 —— **已证（2026-10-09）**

探针 crate：`.scratch/2026-10-09-wasm-core-single-crate/_tools/bindgen-poc/`（复用 `target/host-kits` 桶，4.9s 编译）

```text
P1-a 分片版 Host trait 签名 == 整份 world 版（编译通过即证）
P1-b add_to_linker 首次=Ok；重复注册 is_err = true
       重复注册错误：map entry `spawn` defined twice
```

- **P1-a 判据**：`impl Host for ProbeHost` 的 6 个方法签名**逐项复制自生产代码** `packages/bedcode-pty-engine/src/plugin_binding.rs:185-226`（那份 impl 正是对**整份 world 绑定**的实现，能通过编译 ⇒ 其签名即整份版签名）。对**分片版** trait 写同样签名能通过编译 ⇒ 两版签名一致。rustc 全程无 E0046 / E0053（缺方法 / 签名不符），只有 `add_to_linker` 的类型推断问题。
- **P1-b 判据**：`host_pty::add_to_linker::<ProbeHost, HasSelf<ProbeHost>>(linker, |s| s)` 首次 `Ok`；同一 interface 注册两次 → `Err("map entry 'spawn' defined twice")` ⇒ **复现 ADR 0035 D5 的 `defined twice` 风险**（分片装配面成立，越权装配会被抓住）。
- **踩坑记录**：wasmtime 48 的 `HasSelf` 有 1 个泛型参数（`wasmtime-48.0.5/src/runtime/component/has_data.rs:301`），POC 必须写 `HasSelf<T>`；生产代码 `pty-engine/plugin_binding.rs:106` 裸写 `HasSelf` 是依赖本 crate 内的别名/import。另：直接调 `add_to_linker`（不写显式泛型）会 E0283（`HostWithStore` / `HasData::Data<'a>` 推断歧义）。

### POC 四命题总结

| 命题 | 结论 | 影响 |
| --- | --- | --- |
| P1 分片 bindgen 对等 | ✅ | D4 成立：能力域可 bindgen 自持分片 |
| P2 组合等价 | ✅ | D2 成立：同 package + `include` 组合，零 ABI 破坏 |
| P3 无桌面依赖 | ❌（编译绿、依赖图红，28 处） | D5 前置不成立 ⇒ **新增票 02** |
| P4 拼装脚本 | ✅ | D1 成立：幂等 + 逐字一致 |
