# wasmtime 版本两端锁死，升级必须同步

桌面端与移动端均声明 `wasmtime = "48"`，版本必须两端一致、升级时一次性同步完成。约束来源：47 修复了 Android aarch64 平台 JIT/缓存稳定性问题（46 曾现 SIGILL 崩溃，见各端 `Cargo.toml` 注释，该修复在 48 线上延续）；两端 `.cwasm` AOT 产物按 Engine 版本序列化。配套 binding 组合（wit-bindgen）随组件迁移定版（双端 0.60.0），升级时按组合一并锁定。

## 版本沿革（偏离与恢复）

| 时点 | 桌面 | 移动 | 说明 |
| --- | --- | --- | --- |
| ~2026-09-13 | 47 | 47 | 双端锁死生效 |
| 2026-09-18 | **48.0.2** | 47 | 桌面先行升级（LTS 48，47 非 LTS 支持期将尽），**显式偏离**本 ADR，管控手段：移动端不加载桌面产物、不改 SDK 绑定与组件编码工具链。规格：`.scratch/2026-09-18-wasmtime-48-upgrade/spec.md` |
| 2026-09-26 | 48.0.2 | **48.0.3** | 移动端补齐，**分叉关闭、双端重新对齐**。规格：`.scratch/2026-09-26-mobile-wasmtime-48-wasip3/spec.md` |

## 锁的粒度

- **声明范围**（两端 `Cargo.toml` 的 `"48"`）必须一致——这是本 ADR 的强约束。
- **锁文件解析出的 patch**（当前 48.0.2 / 48.0.3）由各自 `cargo update` 决定，不强制相等：两端 `Cargo.lock` 相互独立，`.cwasm` AOT 缓存写在**各自设备的宿主 cache 目录**、从不跨端复用（同设备上桌面/移动也各自独立），故 patch 差异不产生产物兼容问题。
- MSRV 随之上抬到 **Rust 1.95**（wasmtime 48 的 `rust-version`；47 为 1.94）。

## wasip3 与本 ADR 的关系

构建链 target（`wasm32-wasip3` vs `wasm32-unknown-unknown`）**不受本 ADR 约束**：组件二进制格式向前兼容，桌面 48 加载 unknown-unknown 组件、移动端 48 加载桌面产物均已验证可行。移动端是否跟进 wasip3 属独立议题（评估见 `.scratch/2026-09-26-mobile-wasmtime-48-wasip3/spec.md` §4）。
