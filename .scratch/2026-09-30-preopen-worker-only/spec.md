# wasiPreopenDirs 仅限 worker 类别：主 wasm-app 文件访问一律走宿主 host-fs

> 立项：2026-09-30 · 用户裁定
> 相关：ADR 0032（worker = L3.b，`lifecycle: ephemeral`，本期只预留类型）；
> `docs/knowledge/wasip3-toolchain.md`（preopen 在 wasip3 上不工作，2026-09-30 实测）

## 1. 用户裁定（原始表述）

> 桌面端 wasm-app 不再兼容 preopen wasi 的目录授权，而是全部使用宿主自己的授权机制。
> 以后 preopen 这类只会留给 worker store 类型的 wasm 使用，主 wasm-app 不再需要使用
> wasi 的系统文件访问功能。

**裁定确认（2026-09-30）**：「worker store 类型的 wasm」= ADR 0032 L3.b 的 worker
（`lifecycle: ephemeral`，即用即弃）。preopen 为 worker 保留。主 wasm-app（L3 业务应用）
不使用 WASI preopen 目录授权。

## 2. 现状核对（2026-09-30 实查）

- **生产 wasm-app 零 preopen 使用者**：4 个应用（terminal-session / file-transfer /
  ai-chatbox / agent-hub）的 `plugin.json` 无一份声明 `wasiPreopenDirs`。
- **ai-chatbox 已迁 host-fs**：`rust/src/store.rs` 头部注释与 `lib.rs::activate` 实锤——
  wasip3 下 WASI 0.3 filesystem 是 async import，而插件导出是 sync-lifted，会 trap
  `CannotBlockSyncTask`（wasmtime-environ `fact/trampoline.rs`），故改用 `host-fs`
  （sync import）；数据根 `{HomeDir}/.bedcode/ai-chatbox`，activate 时经 `fs_request_auth`
  集中授权一次、同意后宿主持久化。**README 仍写 preopen 流程，已过时**（文档债）。
- **宿主机制完整但无生产消费者**：`component.rs::build_wasi_ctx` /
  `resolve_preopen_dirs` / `expand_preopen_declarations`（仅 wasip2 上真正可用；
  wasip3 实测 trap 于 `filesystem_method_descriptor_open_at`）、`preauth.rs` 步骤 1.5
  （preopen 声明并入启用前弹窗）、`activation.rs` 漂移重建、`fs_auth.rs::is_granted`。
- **SDK**：`types.rs::WasiPreopenDir` + `wasi_preopen_dirs`、`types.ts`（注释「仅
  wasm32-wasip2 插件」已陈旧）、`manifest-validate.js` 形态校验。
- **测试**：`runtime/tests/wasi_e2e.rs`（3 例，wasip2 夹具 `plugin-wasi-test`）——
  preopen 能力存在的唯一证明；`host/tests/runtime_preauth_test.rs` 两例直接设
  `manifest.wasi_preopen_dirs` 测 preauth 并入；`component.rs` resolve 单测 2 例。
- **worker 现状**：`lifecycle: ephemeral` 在构建期（manifest-validate.js）与加载期
  （`validation.rs::validate_lifecycle`）**双侧显性拒绝**（ADR 0032 §6 清单未完成）。

## 3. 裁定落地的口径

| 对象 | 口径 |
| --- | --- |
| 主 wasm-app（L3 业务应用 / L3.a 业务插件） | **不用** WASI preopen 目录授权；文件访问只走宿主 `host-fs` 授权机制（`fs:read/fs:write` 权限 + `fs_request_auth` / preauth 目录授权） |
| worker 类别（L3.b，`lifecycle: ephemeral`） | preopen 为其**保留**（本次不实现 worker；机制层保留为预留能力） |
| 当前可达性 | worker 未实现（双侧拒绝）→ `wasiPreopenDirs` 对**一切现有 manifest 不可达** |

**双侧显性拒绝（fail-visible，§8 三形态之③）**：非 worker 声明 `wasiPreopenDirs` →
构建期（manifest-validate.js）与加载期（validation.rs）显性报错，**不静默忽略**——
静默忽略会让「声明了却没人读它」的目录配置一路活到分发链。

## 4. 保留而非删除（预留能力的守门）

以下**全部保留**，只改注释口径为 worker-only：
`build_wasi_ctx` / `resolve_preopen_dirs` / `expand_preopen_declarations`、
preauth 步骤 1.5 并入、activation 漂移重建、`WasiPreopenDir` 类型与解析、
`plugin-wasi-test` + `wasi_e2e.rs`（机制层测试，在策略闸门之下，不走 manifest 校验路径）、
`runtime_preauth_test.rs` 两例（直接设 manifest 字段测机制）。

**worker 启用时需补**（写进 ADR 0034 §6）：wasip3 上 preopen 的装配问题未解决
（p3 filesystem 预打开目录能力未建进去）——worker 启用专项需一并解决（或 worker 锁定
wasip2 target），本次不展开。

## 5. 票据

- **票 01 加载期闸门**（`validation.rs`）：`wasi_preopen_dirs` 非空且 `lifecycle !=
  ephemeral` → 显性拒绝，文案点名 host-fs 替代与 ADR 0034。加回归用例
  （persistent + preopen → 拒；缺省 + preopen → 拒；ephemeral + preopen → 由既有
  lifecycle 闸门先拒）。
- **票 02 构建期闸门**（`manifest-validate.js`）：`wasiPreopenDirs` 声明且
  `lifecycle !== 'ephemeral'` → 构建期报错。既有 wasiPreopenDirs 形态用例的 helper
  改为显式 `lifecycle: 'ephemeral'`（保持测形态语义），新增类别闸门反例。
- **票 03 ADR 0034 + 注释/文档口径反转**：
  - 新 ADR `docs/adr/0034-*.md`（本裁定持久记录）；
  - 注释：`types.rs` / `types.ts` / `component.rs` / `preauth.rs` / `activation.rs` /
    `runtime.rs`（build_wasi_test_component）/ `usePluginManager.ts` /
    `runtime_preauth_test.rs` 两例注释；
  - 文档：ai-chatbox `README.md`（激活流程 + 数据目录段改 host-fs 口径）、
    `bedcode-desktop/docs/code-map.md`（plugin-wasi-test 两处）、
    `docs/knowledge/wasip3-toolchain.md` §6 行。
- **票 04 验证**：针对性单测（§3 两段式）→ 全量回归（宿主 cargo test、前端
  pnpm run test:run、eslint）+ §10 自检清单。

## 6. 不改的（最小改动原则）

- `WasiPreopenDir` / `wasi_preopen_dirs` 类型与解析、`build_wasi_ctx` 等机制代码
  **不删除不重构**（worker 预留）；
- `wasi_e2e.rs` / `plugin-wasi-test` / `component.rs` resolve 单测 / preauth 两例
  **不动逻辑**；
- 移动端不涉及（无 WASI 面，ADR 0022 双端偏离条款登记）。
