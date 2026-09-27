# 05：统一集成测试执行 + 全量回归（收尾）

**Type:** task
**Spec:** `../spec.md`（§5 验收标准）；契约单一事实源 `docs/adr/0030-error-envelope-and-user-prompt-boundary.md`
**Blocked by:** 01, 02, 03, 04（全部）
**Status:** done（2026-09-27）

**What to build:** 按用户指令（2026-09-27）：票 01–04 **只编写不执行**的集成测试在本票**统一执行**；同时按 AGENTS §3/§10 跑全量回归（Rust 全量 + 前端全量 + lint），任一红即修复到绿。产出「界面零技术详情」最终状态的验收证据：集成测试清单结果 + 全量套件结果 + grep 复核。

**Acceptance:**

- [x] 票 01–04 编写的全部集成测试在本票统一执行（含宿主 wasm 闭环 fixture 用例；按 AGENTS §3 用 rustup shim 的 cargo，禁止绕过 shim；测试后检查并关闭残留后台进程/端口）
- [x] Rust 全量：`cargo test` 通过（桌面端 `bedcode-desktop/src-tauri`）
- [x] 前端全量：`pnpm run test:run` 通过（对应端；禁止 `pnpm run test` watch 挂起）
- [x] `pnpm exec eslint .` 0 error（warning 不计入门禁）；`cargo fmt` / `cargo clippy` 自查
- [x] i18n：新增键 zh + en 同步存在；无 `{error}` 残留
- [x] 契约回归防线全绿（信封序列化契约 / i18n 扫描 / toast 消费层断言）
- [x] 收尾证据：集成测试清单与结果、全量通过截图/输出、`src/` grep 零技术原文直显；`lens_diagnostics` 无 blocker
- [x] 测试后进程清理：无 mock server / vitest worker / gradle daemon 等残留进程占用端口或 CPU

## 交付说明（2026-09-27，票 05）

### A. 集成测试统一执行（票 01–04 只写不跑的全部启用）

| 文件 | 来源票 | 启用方式 | 结果 |
| --- | --- | --- | --- |
| `src/__tests__/integration/error-envelope.test.ts` | 01 | `describe.skip` → `describe` | 2/2 ✓ |
| `src/__tests__/integration/plugin-manage-error-envelope.test.ts` | 02 | 同上 | 4/4 ✓ |
| `src/__tests__/integration/plugin-error-envelope.test.ts` | 03 | 同上 | 7/7 ✓ |
| `src/__tests__/integration/update-checker-error-envelope.test.ts` | 04 | 同上 | 4/4 ✓ |
| `src-tauri/tests/error_envelope_integration.rs` | 01 | 去 `#[ignore]` | 3/3 ✓ |

启用后既有常规套件内的 `plugin-flow.test.ts`（6 例）belongs 同一契约族，一并绿。

### B. 执行中修复（集成测试暴露的真实缺陷 / 契约偏差）

1. **`plugin/loader.ts` loadInline 吞错假成功（真实缺陷，spec §5「不抛异常吞错」违反项）**：
   后端 `plugin_activate` rejection 被 loadInline catch 吞掉（只做诊断 + mark_error），
   toggle 路径继续显示「启用成功」假成功 toast。修法：诊断/状态登记后 `throw e` 原样上抛，
   由 unified 消费层（showUserError + 重试按钮）展示友好提示。同步给两处调用点兜底：
   `router/index.ts` 导航守卫 try/catch（失败只记日志不中断路由）、`PluginWindowHostView`
   已有 catch 兼容。补丁后 plugin-manage 4 例 + plugin-flow 6 例绿。
2. **`composables/useUpdateChecker.ts` downloadAndInstall 未处理 rejection（真实缺陷）**：
   `await check()` 在 try 外，网络/签名检查失败会抛成 **unhandled rejection** 且原始错误
   逃逸（全链路集成测试第 3 例实测复现：前一用例的 rejection 残留在 mock 队列里变成
   下一用例的失败）。修法：check() 移入 try，统一 failed 态 + 通用文案 + logger 落盘。
3. **集成测试断言修正（契约不变，澄清实现形状）**：
   - `showUserError` 日志 arg2 是归一化 `UserError`（camelCase `requestId`）而非原始信封
     snake_case `request_id`——arg1 消息串恒带 `code=… request_id=…`，两文件断言按此锁定。
   - plugin:notify 的 useToast.info 实参为 `(message, { duration, position })` 两参。
   - plugin-manage「停用失败」用例改 `vi.spyOn(pluginLoader, 'deactivate')` 模拟后端
     rejection 边界（loader 的 `this.plugins` 为私有 map，测试无法直接播种已加载实例；
     真实链路 Activated 插件启动即入 map，deactivate 的后端调用与上抛行为不变）。
   - update-checker 测试：AboutSection **不自动触发**检查（「检查更新」按钮在 SettingsView
     工具栏），改为经 `useUpdateChecker.checkForUpdate()` 驱动 + DOM 失败段断言；并在
     beforeEach `mockReset` 全部 mock（clearAllMocks 不清 once 队列，跨用例残留）。
4. **`link_crypto.rs` 既有 flaky 测试修复（全量并发红、单跑绿）**：
   `registration_into_fresh_chain_is_named_and_idempotent_per_instance` 断言「全局链不含
   本过滤器」，但并行执行时兄弟用例（sync_registration / update_config 系列，持
   SNAPSHOT_LOCK）可能已注册进全局链 → 绝对断言误红。改持同一锁 + 全局链前后差分断言
   （register_into 只作用于自建链，不得改动全局链）。与错误信封无关，属全量回归红修复。

### C. 全量回归结果

- **Rust（桌面 `bedcode-desktop/src-tauri`，rustup shim cargo，`~/.cargo/bin/cargo`）**：
  `cargo test` → **950 passed / 0 failed**，覆盖 11 个 test target（lib 936 + error_envelope
  integration 3 + server_integration / pty_session_chain / http_auth_biometric / link_crypto_http /
  broadcast_shutdown / build_manifest_smoke / ws_auth_rules / wasm_bridge_bench / doc-tests）。
  构建前按 AGENTS §3 target >15GB 规则 `cargo clean`（20.4GiB），重建成 3.4G。
- **前端宿主 + SDK + wasm-apps（终端会话 / agent-hub，根配置 happy-dom）**：
  `pnpm run test:run -- --maxWorkers=2` → **97 files / 975 tests 全绿，0 skipped**
  （对比票 04：792 passed + 17 skipped——本票启用 4 个 describe.skip 集成文件后 0 skipped）。
  全量单命令偶发 `ERR_IPC_CHANNEL_CLOSED`（已知 vitest worker 批量崩溃，memory 在册），
  用 `--maxWorkers=2` 稳定复跑绿。
- **ai-chatbox（独立 vitest，node 环境纯逻辑测试）**：7 files / 144 passed。
- **file-transfer**：无独立测试文件（其 format 测试在宿主 `src/__tests__/plugins/file-transfer/`，
  已随宿主 97 files 覆盖）。
- **Lint**：`pnpm exec eslint .` → **0 errors**（120 warnings 为既有噪音，不计入门禁）；
  `cargo fmt --check` 我的文件（error_envelope_integration.rs）clean（diff 全为并发 agent
  在途文件：db/database.rs、pty/pty_process.rs 等，未触碰）；`cargo clippy` 无新增项。
- **i18n**：全部 6 组 locale 对（宿主 3 + 插件 3）键集合 python 对比 zh/en 完全同步；
  `{error}` 仅剩注释提及（无活动值）；`errorInterpolationGuard` 28 tests 绿。

### D. 契约回归三道防线（全绿，随全量套件运行）

1. 信封序列化契约：`system/error.rs` mod tests（字段白名单 / 无 detail / request_id 存在 /
   UserFacing 透传 / EventEnvelope 同形）→ 随 lib 936 绿。
2. i18n `{error}` 扫描：`src/__tests__/locales/errorInterpolationGuard.test.ts` 28/28。
3. toast 消费层永不渲染：`utils/userError.test.ts` 30 例（parseInvokeError 形状矩阵、
   showUserError 无码渲染断言）→ 随宿主 975 绿。

### E. grep 复核（「界面零技术详情」最终证据）

- locale 值 `{error}` 插值：**仅注释提及，零活动值**（zh/en desktop.ts 注释留档退役原因）。
- 模板裸渲染 `{{ state.error }}` / `{{ e.message }}` / `{{ error.message }}`：**零命中**。
  （唯一保留 `Input.vue`/`TextInput.vue` 的 `error` prop = 表单校验位，无动态绑定，票 04 留档。）
- `e.message` 直显：宿主 `src/` 与 wasm-apps 四应用 vue 模板零命中。
- AppError 信封序列化契约测试 + 事件载荷信封测试均在实际执行中覆盖「detail 不进 payload」。

### F. lens_diagnostics 与遗留说明

- `lens_diagnostics mode=all` 无 blocker：5 个测试文件仅有 TS「inferred settings」推断噪音
  （`@/` alias 2307 / `vi.mocked().mock` 2339 —— 与全仓既有测试文件同款，非 tsconfig 覆盖下
  的真实错误）；存在的 1 个 code-quality issue 为并发 agent 在途文件
  （agent-hub useUsage.ts 的 knip unlisted vue = 既有系统噪音，vue 是 package.json 已声明
  依赖，非 CI 门禁，memory 在册）。
- 测试后进程清理：`ps`/`ss` 复核无残留 vitest / cargo / gradle / mock server，无测试占用端口。

### G. 边界说明（与 spec 的偏差，记录）

- 本票实际改动面含 3 个宿主实现文件（loader/router/useUpdateChecker）——均为集成测试执行
  暴露的真实缺陷（假成功吞错 / unhandled rejection），属「任一红即修复到绿」的必改项，
  非顺手重构（每处均有票 05 注释留档）。
- 移动端零改动（终端选区修复并发 agent 在途文件未触碰）；移动端 Rust/前端全量未被本票
  改动影响，故不重复执行（AGENTS §10「改了 Rust/前端」的触发条件不满足）。