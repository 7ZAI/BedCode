# 01 — wit-bindgen 0.60 × wasmtime 47 探路 spike

**What to build:** 决定移动端组件化工具链的版本锁定结论。用 wit-bindgen 0.60.0 生成一份最小组件契约（2 个宿主 import + 1 个插件 export），经过组件编码后在移动端宿主（wasmtime 47，与桌面端锁死）上实例化并成功调用一次命令，证明 0.60 生成的组件与 47 宿主兼容。若 0.60 与 47 不兼容（生成代码 API 不匹配 / 实例化报错），则固定回退方案 wit-bindgen 0.41（桌面端已投产组合）并把结论写回 spec（§5 R1 关闭），锁定后其结论成为 02/04 的依赖版本输入。

**Blocked by:** None — can start immediately

**Status:** done — 2025-08-14 结论：**采用 wit-bindgen 0.60.0，不回退**（见下「结论」）

- [x] 最小 WIT（2 import + 1 export）在移动端 SDK 侧用 wit-bindgen 0.60 生成 guest 绑定并编译通过（wasm32-unknown-unknown）
- [x] 同一份 WIT 在宿主侧生成绑定并接线
- [x] 组件编码后产物魔法字节为 `0d 00 01 00`，在 wasmtime 47 引擎上实例化成功、命令调用返回预期值
- [x] 无论成败，结论（采用 0.60 或回退 0.41 + 失败根因）记录到 spec（S0 小节 + §5 风险表 R1），作为 02/04 的版本输入

---

## 结论（2025-08-14）

**wit-bindgen 0.60.0 × wasmtime 47 兼容 → 版本锁定 0.60.0，R1 风险关闭。** 已同步写回 spec（§S0 小节、§5 R1、§8 待定行）。

### 实证内容

spike 位于 `.scratch/mobile-wasmtime-component-migration/spike/`（独立 workspace：`guest/` + `host/` + `wit/spike.wit`，见 README）。复现：`cargo run --release -p spike-host`。全部断言通过：

1. **guest 编译**：wit-bindgen 0.60.0（macros）生成绑定，`wasm32-unknown-unknown` 编译通过；产物为 core module（`00 61 73 6d`），含 `component-type:wit-bindgen:0.60.0` 自定义段（440B）
2. **宿主接线**：宿主侧用 wasmtime 47 自带的 `wasmtime::component::bindgen!` 生成绑定（无需独立 wit-bindgen 依赖——`wasmtime-internal-component-macro` 47.0.3 内置，内部 wit-parser 0.252），`Host` trait impl + `add_to_linker` 接线模式与桌面端 `component.rs` 完全一致
3. **组件编码**：wit-component **0.256.0** 编码；产物字节形态 `00 61 73 6d 0d 00 01 00`（**模块段在组件头之前**——S3 字节检查按此 8 字节形态，spec 原文已如此写）
4. **实例化 + 调用**：`Component::from_binary` → `linker.instantiate`（含 fuel + ResourceLimiter，与生产同配置）→ 命令导出调用成功：
   - `invoke`：guest 内 host-storage 读（预写 `value-42`）→ host-log 埋点 → 写回 → 宿主断言全部命中
   - `probe`（bool/u64→u32 标量覆盖，§3.1 契约类型面）：`probe(true, 47) == Ok(47)`、`probe(false, 47) == Ok(0)`

### 对后续 tickets 的版本输入

| 项 | 值 | 依据 |
|----|-----|------|
| wit-bindgen（SDK guest 绑定） | **0.60.0**（不回退 0.41） | 本 spike 实证 |
| 宿主侧绑定来源 | wasmtime 47 自带 `component::bindgen!`，**不加**独立 wit-bindgen 依赖 | 本 spike 实证；与桌面端 `component.rs` 同模式 |
| 组件编码工具 | wit-component 0.256（S2 复制桌面 `tools/componentize` 时按此升级版本） | 本 spike 实证（桌面端 0.255 属 0.60 之前的组合） |
| 产物字节断言 | `00 61 73 6d 0d 00 01 00`（8 字节形态） | 本 spike 实证（§5/S3 原文已按此写） |

### 过程中发现的两个 API 差异（S2 落地时注意）

1. **0.60 import 函数 string 参数为 `&str`**（不再是 0.41 的 `String`）：`host_storage::get("key")`、`host_log::info(&format!(...))`。SDK `host/` 桩与插件侧调用点签名以此为准；export 侧（`Guest` trait）参数仍为 `String`
2. **`add_to_linker` 需要显式类型标注**：`bedcode::spike::host_storage::add_to_linker::<State, D>(&mut linker, |s| s)`，其中 `type D = wasmtime::component::HasSelf<State>`（桌面端 component.rs 同样写法）

### 验证边界（诚实声明）

- spike WIT 为 §3.1 的类型子集：string / option<string> / result<T,string> / bool / u32 / u64 均已覆盖；`record`/`variant`/`resource` 不在契约内（未引入）
- 未验证：fuel trap、ResourceLimiter 拒绝、AOT `Component::serialize`（均属 S1 单测范畴，机制与 wasmtime 版本相关而非 wit-bindgen 版本）
- 宿主为 x86_64 Windows（开发机）；Android aarch64 引擎行为由 ADR 0019 锚定（两端同版本 47），运行期差异不在本 spike 范围