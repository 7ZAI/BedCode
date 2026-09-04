# 04 — 前端模块加载结果写入 tracing 诊断（移动端）

**What to build:** 移动端前端 TS 模块加载成败进入落盘日志：宿主内部诊断通道把启动期每个插件前端模块的导入/激活结果写入 tracing，使 `runtime.*.log` 能反映插件加载的后端半程之外的完整图景。仅日志，不入状态机。

设计依据见同目录 `../spec.md` §3.6（P2）。注意这是宿主自身诊断命令，不是插件协议，不违反「不加特殊上报 ABI」约束。移动端相对桌面端的差异：

- 桌面端走 `api_bridge` 注入 `plugin_frontend_load_report`；移动端对应位置是 `bedcode-mobile/src-tauri/src/plugin/commands.rs` 的 Tauri command 注册表，模式一致但需评估移动端是否真的需要这一通道
- 移动端 `loader.ts` 现状：`console.log` / `console.warn` 标 7 处（启动数量、Scan retry、Rust managed by backend、not enabled、activating defer、loadFrontend 错误等），都未进宿主 tracing
- 移动端 plugin_mark_error 已有走 `tracing` 通道（`commands.rs:245-252`），可参考其注册模式

**Blocked by:** 02 — 前端消费新状态（已完成，本 issue 在其之上实现）

**Status:** pending（spec §3.6 显式标 P2「范围外可拆票」）

**实施建议**（与桌面端 issue 04 对位）：

- [ ] `commands.rs` 增加 host 内部诊断命令（不入公共 API）：入参 `plugin_id` + `stage`（import / activate / hot-reload）+ `ok` + `detail`；仅写 `tracing::info!` / `tracing::error!`，不改任何状态
- [ ] 前端 `loader.ts` 在导入成功、激活成功、失败三条路径各上报一次；`invoke` 失败静默吞掉，不阻塞加载流程
- [ ] 移动端 dev run 验证：`runtime.*.log` 中可见每个 rust-ts/ts-only 插件的前端加载结果行（需确认移动端 `runtime.log` 收集路径与桌面端一致；若收集在宿主 logcat 而非文件，需相应调整验证方式）
- [ ] `pnpm run test:run`（AGENTS.md 强制 pnpm）与 `cargo test` 全绿

**P2 优先级的实际权衡**：

- 移动端日志回收链路比桌面端弱（主要靠 logcat 而非 `runtime.*.log` 文件），诊断价值不及桌面端明显
- 桌面端 issue 04 的 `dev-run.js` 路径陈旧教训不适用于移动端（移动端构建工具链不同）
- 建议在桌面端 issue 04 落地完成、积累使用反馈后，再决定是否值得在移动端实施

**结论**：本 issue 留 pending 状态，与 spec §3.6 范围外拆票口径一致；不在本次验收范围。
