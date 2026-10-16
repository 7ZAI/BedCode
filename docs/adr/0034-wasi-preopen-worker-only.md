# wasm 应用不使用 WASI preopen，文件访问统一走宿主 host-fs

## 状态

**已裁定（2026-09-30，用户）**——主 wasm-app 不再兼容 preopen wasi 的目录授权，
全部使用宿主自己的授权机制；preopen 只留给 worker 类别（ADR 0032 L3.b，
`lifecycle: ephemeral`）使用。双侧显性拒绝（构建期 + 加载期）已随本 ADR 落地。
spec：`.scratch/2026-09-30-preopen-worker-only/spec.md`。

本 ADR 修订 ADR 0022 的「宿主能力边界」相关表述：WASI preopen 不再是业务应用可用的
文件访问通道。

## 背景

1. **wasip3 上 preopen 实测不工作**（2026-09-30，推翻静态推断）：宿主
   `runtime/component.rs::build_wasi_ctx` 返回共享 `WasiCtx`、p2/p3 两套 linker 都注册
   ——很容易推断「preopen 与 target 无关、wasip3 等价」；实测 p3 linker 虽接上（import
   解析成功、进到 host 实现），但**预打开目录的能力没建到 p3 filesystem 接口上**，
   guest 一 open 就 trap（`filesystem_method_descriptor_open_at`）。`plugin-wasi-test`
   因此固定 wasip2 独立 crate。
2. **wasip3 下 WASI filesystem 对业务应用本就不可用**（更深一层）：WASI 0.3 的
   filesystem 方法是 async func，而插件导出（`activate` / `invoke_command`…）是
   sync-lifted——wasmtime 进入 sync 导出时清掉 task 的 `may_block` 标志，guest 一旦
   等待 async import 即 trap `CannotBlockSyncTask`。这是**结构性的**，不是装配 bug。
3. **ai-chatbox（唯一 preopen 使用者）已迁 host-fs**：数据根 `{HomeDir}/.bedcode/
   ai-chatbox` 改为宿主绝对路径，activate 时经 `fs_request_auth` 集中授权一次、同意后
   宿主持久化；Rust 端 `std::fs` 直读直写改为 `host-fs` 原语转发。README 未同步（文档债）。
4. **生产 wasm-app 已零 preopen 声明**：4 个应用的 `plugin.json` 无一份声明
   `wasiPreopenDirs`；但宿主机制（`build_wasi_ctx` / `resolve_preopen_dirs` /
   preauth 并入 / 激活漂移重建）与 SDK 文档仍把它当活能力——名实不符。

## 决定

### 主 wasm-app：不用 preopen，文件访问只走宿主 host-fs 授权机制

- 业务应用（L3 业务应用 / L3.a 业务插件）的文件访问一律经 `host-fs` 原语：
  manifest `permissions` 声明 `fs:read` / `fs:write`，目录授权走宿主
  `fs_request_auth` / preauth 弹窗，授权记录持久化（宿主自身授权机制）。
- WASI 系统文件访问（preopen / guest `std::fs` 直连）对业务应用**不是能力**，是
  已关闭的通道。

### preopen 保留给 worker 类别（ADR 0032 L3.b，`lifecycle: ephemeral`）

- worker（即用即弃，本期只预留类型）是唯一允许声明 `wasiPreopenDirs` 的类别——
  preopen 作为它的**预留能力**保留，本次不实现 worker、不展开 preopen 装配。
- 机制层**全部保留**（不删除不重构）：`build_wasi_ctx` / `resolve_preopen_dirs` /
  `expand_preopen_declarations`、preauth 步骤 1.5 并入、activation 漂移重建、
  `WasiPreopenDir` 类型与解析、`plugin-wasi-test` + `wasi_e2e.rs`（机制守门测试，
  在策略闸门之下）、`runtime_preauth_test.rs` 两例（直接设 manifest 字段测机制）。

### 双侧显性拒绝（fail-visible，§8 形态之③）

| 侧 | 落点 | 判据 |
| --- | --- | --- |
| 构建期 | `packages/plugin-sdk-desktop/bin/manifest-validate.js` | `wasiPreopenDirs` 声明且 `lifecycle !== "ephemeral"` → 报错，文案指路 host-fs |
| 加载期 | `wasm_core/manager/validation.rs::validate_preopen_category` | `wasi_preopen_dirs` 非空且 `lifecycle != ephemeral` → 显性拒绝，文案点名 host-fs + ADR 0034 |

**不静默忽略**：静默忽略会让「声明了却没人读它」的目录配置一路活到分发链（作者以为
preopen 生效了，实际没有任何一行代码读它）。worker 未实现期间（ADR 0032 §6 双侧拒绝）
`ephemeral` 本身被 `validate_lifecycle` 拦截，故 `wasiPreopenDirs` 当前对**一切 manifest
不可达**——类别闸门是取值域层面的落死，worker 启用（ephemeral 放行）后 preopen 即恢复
为 worker 专属能力。

## worker 启用时需补（防「预留」变永久悬空的谎言）

- [ ] **wasip3 preopen 装配问题**（本 ADR 的唯一技术悬项）：p3 filesystem 的预打开
      目录能力未建进 p3 linker——worker 启用专项需一并解决（查 wasmtime 48 的 p3
      `add_to_linker` 是否需要额外的 dir/stream 装配），或 worker 锁定 wasip2 target
- [ ] 其余 ADR 0032 §6 L3.b 启用清单（调度方、store 传参协议、权限模型、配额）——
      本 ADR 不重复，照抄引用

## 归属裁决（ADR 0022 §5.1.2 三问）

1. **离宿主能实现吗？** 权限闸门（构建期 + 加载期）+ 机制保留只能宿主做 → 放宿主
2. **携带产品语义吗？** 类别归属是机制学（哪个类别能声明哪项能力），不含产品概念；
   文案只指路机制不解释业务 → 不命中 B1–B6
3. 都不命中 → 放宿主，落点：manifest 校验闸门（安全闸门类薄壳，§5.1.3 允许）

## Consequences

**正面**

- 文件访问授权收敛为**单一机制**（host-fs + fs_auth），沙箱口径简化为「无 preopen
  = WASI 无文件入口」；
- 文档与实现一致（ai-chatbox README、SDK 注释、code-map 同步反转）；
- 双侧显性拒绝把「类别归属」变成可执行门禁，防回接锁有测试载体。

**代价 / 风险**

- worker 启用时需先解决 wasip3 preopen 装配（见上），否则 worker 只能继续走 host-fs；
- `wasi_e2e.rs` 依赖本机 wasm32-wasip2 target（rustup target add），CI 需保留该 target
  安装步骤（现状已如此，无新增）。

**双端偏离**：移动端无 WASI 面（无 preopen、无 wasi import），本 ADR 不适用；
移动端相关判断以 ADR 0018/0019 与 `docs/knowledge/mobile-desktop-auth.md` 为准。

## 修订记录

- **2026-09-30**：立项。用户裁定「主 wasm-app 不再兼容 preopen wasi 目录授权，全部使用
  宿主自身授权机制；preopen 只留给 worker 类别」。落地双侧显性拒绝 + 文档口径反转；
  确认「worker store 类型的 wasm」= ADR 0032 L3.b worker（`lifecycle: ephemeral`）。
