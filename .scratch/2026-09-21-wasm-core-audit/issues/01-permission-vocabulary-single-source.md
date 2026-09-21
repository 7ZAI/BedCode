# 01: prefactor——权限词汇单源与漂移锁扩面

**What to build:** 权限词汇只有**一个真源**（SDK Rust `permission.rs` 的 `PERMISSION_*` + `VALID_PERMISSIONS`），打包 CLI 与前端合法集由它**生成**而非手抄；漂移锁从 2 个权限扩到**全部权限域**（集合相等断言，非包含断言）；生产 manifest 里的死词汇清零。本票是 02-08 的前置：后续任何「拆权限位 / 加权限位」都要自动被锁住，否则修一处漂一处。

**Blocked by:** 无（prefactor）

**Status:** done（2026-09-21 实施完成，见「实施记录」；`ui:pageToolbar` 等展示文案缺口与两条既有红已登记）

## 实测基线（本票要清零的漂移）

| 源 | 条数 | 相对 SDK 的缺项 |
| --- | --- | --- |
| `bedcode-desktop/packages/plugin-sdk-desktop/rust/src/permission.rs:81-110` | 28 | 缺 `ui:pageToolbar`、`ui:fileHandler`（声明后被 `:225-231` 静默过滤） |
| `.../plugin-sdk-desktop/bin/cli.js:410-432` | 22 | 缺 `fs:read` `fs:write` `mdns` `auth` `process:run` `timer:schedule` `app:cli` `ui:dialog` |
| `bedcode-desktop/src/plugin/permission.ts:9-35` | 24 | 缺 `fs:read` `fs:write` `mdns` `auth` `process:run` `timer:schedule` `app:cli` |
| 生产 manifest（`plugins/file-transfer` 等） | — | `bus` `fileservice` `transfer` 三处都不认 = 装饰词汇 |

## 验收

- [x] SDK Rust `VALID_PERMISSIONS` 为唯一真源；CLI 与前端两份列表由生成物产生（生成脚本随 SDK 包，`pnpm run build` 或 `cargo` 侧任一入口可重跑），并在文件头注明「生成物，勿手改」
- [x] 漂移锁测试断言**集合相等**：SDK 集 == 前端集 == CLI 集 == 「host_impl 实际用到的权限集」（第四项从 `check_permission` 调用点提取，可用测试内静态清单 + 注释锚点）
- [x] 每个权限域至少一条「未授予即拒绝」的门禁用例存在（照 `host_impl/tests/pty.rs:87` 与 `host_impl/session.rs:1077` 的先例形态补齐 ws / task / fs / network:http / peer / mdns / timer / database）
- [x] `ui:pageToolbar` `ui:fileHandler` 二选一处置：进 SDK 真源或从前端与 CLI 删除（禁止继续「声明了被静默过滤」）
- [x] 生产 manifest 的 `bus` `fileservice` `transfer` 清零：改成对应实际域（`bus` → 若总线无需权限则在文档中明确它不是权限位；`fileservice`/`transfer` → `peer`/`fs`/`network:http` 实际所需位）
- [x] `bedcode-plugin validate`（`cli.js:487-490` 的未知权限 error）挂进插件 build 链或 CI，否则删掉该校验以免给人「有锁」的错觉
- [x] 门禁：`cargo test` + `pnpm run test:run`（`--pool=forks`）+ `pnpm exec eslint .` 0 error + SDK 侧 `cargo test`（**两处既有红与本票无关，见实施记录「门禁实测」**）
- [x] 移动端 SDK 同名文件的处理结果写进本票 Comments（ADR 0018 两端契约独立，不要求同步，但**必须显式记录分叉**，禁止静默）

## Comments

- 2026-09-21 立项：来源 `.scratch/2026-09-21-wasm-core-audit/spec.md` §5「扩展性：漂移实测」。漂移数据为脚本比对实测，非推断。

### 实施记录（2026-09-21，票 01 done）

**真源形态（比票面更进一步）**：SDK 加了一张反射表 `PERMISSION_VOCABULARY: &[(&str ident, &str value)]`，
标识符用 `stringify!` 取自常量本身；`VALID_PERMISSIONS` 改由该表 `const fn` 派生。
加一个权限位只改一行，且「源码里的 `PERMISSION_*` 引用」能被机械还原成权限串——
第四项集合因此**不需要手抄静态清单**（票面允许的兜底方案没用上）。

| 落点 | 内容 |
| --- | --- |
| 真源 | `packages/plugin-sdk-desktop/rust/src/permission.rs`：反射表 + 派生 `VALID_PERMISSIONS` + `PERMISSION_API_MAP`（均 `pub`），30 条词汇 |
| 生成器 | `rust/examples/gen_permission_vocabulary.rs`（`cargo run --example`，不随插件构建编译）；SDK `pnpm run gen:permissions` |
| 生成物① | `packages/plugin-sdk-desktop/bin/permission-vocabulary.json` → `bin/cli.js` 的 validate（经新抽出的 `bin/manifest-validate.js`） |
| 生成物② | `bedcode-desktop/src/plugin/permission.vocabulary.ts` → `src/plugin/permission.ts`（前端只剩三个函数，零清单） |
| 第四副本 | `bin/manifest-gen.js` 的 `REGISTER_PERMISSIONS` 由生成物 apiMap 的 `ui.register*` 派生（票面未点名，实测是同型手抄，一并收掉；A/B 比对四个生产插件产物逐字一致） |
| validate 挂链 | `scripts/plugin-build.js` 构建前 `validateManifest()` 非零即中断；与 CLI 同一套规则（`bin/manifest-validate.js`） |
| 门禁锁 | `src-tauri/src/plugin/permission.rs` 五条锁：三副本集合相等 / 生成物标注且前端不再手抄 / **每条词汇都有门禁落点** / 生产 + fixture manifest 无死词汇 / validate 确在构建链上 |
| 前端锁 | `src/__tests__/plugin/permissionVocabulary.test.ts` 五组：`context.ts` 全部 `requirePermission` 调用点唯一权限门 + 授予/不授予/他权限三态、`ui.registerPage` 归属、TS↔JSON 同表、死词汇不合法、permission.ts 不含字面量 |
| 门禁用例 | 补 `mdns` / `peer` / `database` 三域（此前零门禁用例）；每条都配**正例对照**（授予后报的是参数校验/无头上下文错误而非权限拒绝），防「恒拒绝假绿」。ws / task / fs / network:http / timer 实测已有 |
| 死词汇 | `plugins/file-transfer/plugin.json` 去 `bus` / `fileservice` / `transfer`；`bus` 在 code-map 的 core-bus 行明确「不是桌面权限位，访问控制归 topic 命名空间（票 05）」 |
| 文档 | AGENTS §7 同步点改口径（真源 + 生成物 + 重跑命令）；code-map `permission` 行与 core-bus 行 |

**`ui:pageToolbar` / `ui:fileHandler` 处置 = 进真源**：前端 `context.ts:211,218` 确有
`requirePermission` 落点，是真的贡献面权限位，删了会打断在用能力。同时补
`ui.registerPage → ui:sidebar`（此前**只存在于前端手抄表**，SDK 侧没有 → 宿主授权面缺位）。
SDK 词汇与前端合并后行为不变：23 个前端实际门禁名逐条比对映射一致（前端锁 L1 即此断言）。

**顺带清掉的一处**：`src/plugin/permission.ts` 的 `filterValidPermissions`（零调用者）删除——
它在前端复刻了「storage 恒授予」这条**授权语义**，是真源之外的第二处授予规则，
票 02 取消自动授予时它必然成为漏改点。

**变异自检**（票面 Testing Decisions 要求）：① 从生成物删 `ui.registerPage` → 前端锁 3 组转红；
② 反射表塞一条 `demo:dead`（无门禁落点）+ manifest 塞回 `fileservice` → Rust 锁
`every_permission_has_an_enforcement_point` / `permission_vocabulary_is_equal_across_all_three_copies`
/ `production_manifests_declare_only_known_vocabulary` 三条各自转红。均已还原。

**生成器幂等**：连跑两次 md5 一致；生成物已进 `.prettierignore`（`pnpm run format` 不得重排生成物）。

#### 门禁实测（2026-09-21 本机，forks 跑法）

- `cd bedcode-desktop/src-tauri && cargo test --lib` → **1072 passed / 0 failed**（5 次全量跑里 3 次全绿；
  另 2 次各出现 1 例**换着不同**的闭环用例失败：`ws_e2e::test_ws_endpoint_server_domain_roundtrip`、
  `task_e2e::test_task_submit_events_dispatched_and_status`，单跑均绿）。两条都是 5s 轮询真等
  （wasm + 宿主线程池 + WS 帧），失败点与本票改动面无接触（本票未动任何授权/grant 运行时路径），
  记为并发负载下的既有时序抖动，不在本票处置。
- `cargo test`（全 target）→ **五个集成测试 target 在本票之前就无法编译**：
  `ws_session_route` / `pty_session_chain` / `ws_auth_rules` / `http_auth_biometric` / `broadcast_shutdown`，
  报的都是已退役符号（`server::services::pairing_service`、`AppContextBuilder::pairing_service`、
  `utils::auth::QrTokenManager`、`SessionManager::from_database`、`restart_session`）。
  来源是「配对 / QR 宿主降级退役」与 host-session v21 收敛（删 `create`/`restart`）留下的断链
  （`git grep <符号> HEAD -- src-tauri/src` 全空，错误里也无一处涉及本票改动的符号），
  与本票零接触面。**另立清理项，未在本票擅自修复。**
  后续票 02 / 04 的 `cargo check --lib --tests` 复核同一组断链，未再增加。
- `pnpm exec vitest run --pool=forks --maxWorkers=2` → **75 files / 734 tests 全绿**。
  本票实施过程中该跑法曾报 2 files 3 tests 红（`plugins/session/src/__tests__/{plugin-contract,terminalSettingsSync}.test.ts`
  报 `composables/terminal/useTerminalSettingsSync.ts` 直调 Tauri invoke），当时这些文件是并发会话
  未提交的在途产物；对侧票 01c 提交（1161ab6f8）后复跑即全绿。本票未碰对侧任何文件。
- 根目录 `pnpm exec eslint .` → **0 error**（125 warning，按 AGENTS §10 不计入）。
- SDK 侧 `cargo test --lib`（`plugin-sdk-desktop/rust`）→ **91 passed / 0 failed**，含本票新增三条
  （派生一致性、贡献面权限不被静默过滤、apiMap 键必须是已知权限）。
- 生成器 `cargo build --example` + 重跑幂等；`rustfmt --check` 对新文件与本票改动的 LF 文件零 diff
  （两处 CRLF 文件不整文件 rustfmt，见项目记忆）。
- 测试后无残留进程（本票不 spawn mock server）。

#### 移动端分叉登记（ADR 0018，本票零改动）

- `bedcode-mobile/packages/plugin-sdk-mobile/rust/src/permission.rs` 未动：仍是**手抄**
  `VALID_PERMISSIONS`（19 条，含 `ui:navTab` / `ui:route` / `ui:back` / `system:open` / **`bus`**，
  无 `pty:*` / `ws:*` / `mdns` / `task:run` / `auth` / `session:config` 等桌面域）。
  移动端的 CLI（`plugin-sdk-mobile/bin/cli.js`）与前端（`bedcode-mobile/src/plugin/permission.ts`）
  同样各自手抄 → **移动端完整保留本票清零前的三副本漂移形态**，属该端待办，不在桌面票内顺带改。
- `bedcode-mobile/plugins/file-transfer/plugin.json` 仍声明 `fileservice` / `transfer`
  （两端都不认的装饰词汇）与 `bus`（移动端合法）→ 移动端的清零随该端开工。
- **分叉必须显式记录的三条**：① 桌面 `bus` 不是权限位、移动端是；
  ② 桌面已单源、移动端仍三副本（改移动端词汇时要记得桌面已不手抄，别再反向手抄回去）；
  ③ 票 02 取消「storage 自动授予」时，移动端 SDK 仍是自动授予（`:168` 注释与用例为证）——
  只改桌面即产生**授权语义分叉**，票 02 内必须逐条登记，禁止静默。

#### 本票未做、已登记的后置项

- **权限展示面覆盖 13/30**：`src/plugin/contributionKinds.ts` 的 `PERMISSION_META` + i18n
  `desktop.plugin.perm.*` 只讲得清 13 条，其余 17 条在插件详情页回退成 🔐 + `perm.unknown`。
  票 03 的审批弹层必须把权限清单讲给用户，**该文案补齐是票 03 的前置子项**（顺带可把
  `PERMISSION_META` 的 `titleKey`/`descKey` 模板收敛成 emoji 表 + 键派生，本票按「界面维持」不动 UI）。
- `check_api` / SDK `PERMISSION_API_MAP` 在桌面宿主侧**零生产消费者**（本票只把它升格为真源并生成两份，
  未新接消费者）；`pty.*` / `ws.*` / `peer.*` 等条目是审计名而非前端 context 方法名——
  是否给前端 API 面补一条 Rust 侧最终仲裁，属票 06 的身份绑定改造范围。
- `plugin-wasi-test/plugin.json` 的 `pluginType: "wasm"` 不在 CLI 允许集（`ts-only/rust/rust-ts`）；
  fixture 不进 `plugins:build` 链所以从不被校验，历史遗留，未在本票顺带改。

