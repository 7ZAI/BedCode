# 03 — 静态注册插件 builtin 常驻语义修复（移动端）

**What to build:** 消除「日志称 Static plugin loaded、实际永不激活」的自相矛盾（桌面端 issue 03 的同源问题）。

**Status:** N/A — 移动端不适用

设计依据见同目录 `../spec.md` §1.4、§3.4：

> 移动端**无 inventory 静态注册**（`builtin_manifests()` 返回空 Vec，内置插件走 APK assets 解压 + 正常激活流程）。故桌面 spec §1.4 / §3.4 的静态插件矛盾在移动端不适用，**无需处理**。

**阻塞范围确认**：

- `bedcode-mobile/src-tauri/src/plugin/manager.rs` / `wasm_runtime.rs` / `component.rs` **均无 `inventory` 依赖**（与桌面 host.rs 区别点）
- `ApkAsset` / `FrontendOnly` 走 `activate()` phase 0 审批门禁放行（`manager.rs:514`）+ 正常激活路径，与用户安装插件的差异仅在**信任域**而非激活语义
- 移动端无 `Static plugin loaded` 日志——`loader.rs:239-244` 输出是 `Scanned N dir(s), loaded M plugin(s)`，按发现计

**结论**：本 issue 在移动端无改造项，留 N/A 占位以保持与桌面端 4 张 issue 的对位结构（不污染后续审计链）。
