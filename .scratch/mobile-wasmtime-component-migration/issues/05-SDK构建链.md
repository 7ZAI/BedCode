# 05 — SDK 构建链

**What to build:** 移动端插件构建产出组件产物：把组件编码工具（自研、等价 wasm-tools component new、幂等——复制自桌面端 SDK）接入移动端 SDK；`bedcode-plugin build` 的 rust 步骤改为 cargo 构建后追加编码步骤；产物仍落在宿主资源目录。直连产物=组件成为唯一合法形态。

**Blocked by:** 04 — SDK 组件绑定

**Status:** done（2025-08-14，随 04 一并交付，见 04 ticket）

- [ ] `bedcode-plugin build`（或等价 rust 构建入口）产出组件二进制（魔法字节 `0d 00 01 00`，编码工具幂等：重复执行不重复嵌套编码）
- [ ] 产物无 WASI import（不需要 adapter）；wasm32-unknown-unknown 目标不变
- [ ] 编码工具在移动端仓库内可独立构建与运行
- [ ] 对非组件输入（无组件元数据）报错信息明确