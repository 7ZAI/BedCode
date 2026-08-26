# 01 — 插件骨架与白名单打通

**What to build:** 用户安装磁盘清理插件并首次启用时，弹出目录授权弹窗；同意后插件激活成功，面板显示 `{home}/AppData/Local` 预打开自检结果。这是整条链路的 tracer bullet：manifest 白名单声明 → WASM 组件激活自检 → 最小前端面板。拒绝授权或预打开不可见时给出明确错误与「停用再启用」引导。

**Blocked by:** None — can start immediately

**Status:** ready-for-agent

- [ ] manifest 声明 `wasiPreopenDirs` 仅一条 `${home}/AppData/Local`，permissions 按需最小（storage / fs 授权域）
- [ ] 首次启用触发 fs_auth 授权弹窗；同意后 activate 成功，拒绝/超时进入 Error 状态且重新启用可重试
- [ ] 面板展示自检结果；预打开不可见时提示「停用再启用」引导（对齐 ai-chatbox 先例语义）
- [ ] 插件 `cargo test` 与桌面端 `npm run test:run` 绿
