# 04 — 前端模块加载结果写入 tracing 诊断

**What to build:** 前端 TS 模块加载成败进入落盘日志：宿主内部诊断通道把启动期每个插件前端模块的导入/激活结果写入 tracing，使 `runtime.*.log` 能反映插件加载的后端半程之外的完整图景。仅日志，不入状态机。

设计依据见同目录 `../spec.md` §3.7（P2）。注意这是宿主自身诊断命令，不是插件协议，不违反「不加特殊上报 ABI」约束。

**Blocked by:** 02 — 前端消费新状态：降级徽章与加载门禁（已完成，本 issue 在其之上实现）

**Status:** done（2026-08-26）

- [x] api_bridge 增加 host 内部诊断命令：入参插件 id + 结果 + 明细，仅写 tracing（info/error），不改任何状态 → `plugin_frontend_load_report(plugin_id, stage, ok, detail)`
- [x] 前端加载器在导入成功、激活成功、失败三条路径各上报一次；诊断命令失败不阻塞加载流程 → loader 四条加载路径（启动 rust-ts / 启动 ts-only / 手动激活 / 热重载）均接线 `reportLoadDiagnostic`（invoke 失败静默吞掉）；失败上报带 stage 标注发生在导入还是激活步骤
- [x] 桌面端 dev 运行验证：`runtime.*.log` 中可见每个 rust-ts/ts-only 插件的前端加载结果行 → 实测 auto-task（ts-only）与 ai-chatbox（rust-ts）各有 `frontend module load ok plugin_id=… stage=import/activate` 两行；跳过/未激活插件零上报
- [x] `npm run test:run` 与 workspace `cargo test` 全绿 → vitest 58 文件 / 518 测试全绿（gating 测试新增诊断断言 + 诊断容错用例）；`cargo test --lib` 522 绿（api_bridge 新增恒 Ok 单测）

**备注：**
- 验证时发现 `scripts/dev-run.js` 的 WASM 缺失预检路径陈旧（检查 `resources/plugins/desktop/<name>/` 而产物目录已是 plugin-id 形态 `com.bedcode.*`），与本 issue 无关，绕过 wrapper 直接 `npx tauri dev` 完成 dev 运行验证
