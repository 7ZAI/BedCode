# 票 06 · 全量重建与回归（收口）

**来源**：`.scratch/2026-10-13-wit-package-semver/spec.md` R5 对策、§5 D1 项 8、§7 T6、AGENTS.md §10 完成定义。

**Blocked by**：票 03 + 票 04 + 票 05（版本化契约 + 运行期改造 + ABI 退役 + 元数据/锁同步全部就位后收口）。

**Status**：ready-for-agent

## What to build

全量产物重建与双端回归，验证版本化契约 + ABI 退役后的完整链路：

1. **全量重拼**：`node scripts/compose-wit.mjs --all`（票 02 已做，此处复核无漂移）。
2. **产物重建**：桌面 4 + 移动 3 个 wasm-app 与全部 fixture 组件重建（含 wasmHash 注入）。桌面 stable 无 `wasm32-wasip3` 预构建产物 ⇒ 依赖随包 wasm 的 7 个桌面测试目标本环境无法转绿（既有基线，排 nightly 环境补跑）——交付说明逐条「跑了/没跑」。
3. **全量回归**：双端 `cargo test`（宿主两端各自 `src-tauri` + SDK + 能力 crate + file-transfer-core）、两端 `pnpm run test:run`、根 `pnpm exec eslint .`（0 error）、`pnpm run check:wit`、`cargo fmt` / `cargo clippy` 自查。
4. **迁移期语义验证**：版本化后旧产物（unversioned import 名）在新宿主实例化期被拒，文案命中 `stale_artifact_rebuild_hint` 判据（`has not been defined` / `not found in the linker`）⇒ 做 fail-visible 重建指引验证（不做双版本并存，研发起步期无存量第三方产物；已装插件按 0.x breaking 预期走审批重装，ADR 0020）。
5. **文档核对**：CHANGELOG 双语、ADR、两端 code-map 与终态一致；spec §9 实施记录按票追加。

## 验收标准

- [x] 双端 `cargo test` 全绿（桌面 288 通过 / 5 红 = 3 个结构性锁既有基线 + 1 个并行线在途 dev-dep 项，均非本票引入；详见实施记录）
- [x] 两端 `pnpm run test:run` 全绿；根 eslint 0 error（桌面 117 文件/1624 用例，唯一红=既有 flake `terminalPreview` resize 重试，2026-10-06 已 HEAD 对照；移动 78 文件/895 用例全绿）
- [x] `check:wit` 绿；wasm-apps + fixtures 重建完成（wasmHash 注入并校验）
- [x] fail-visible 重建指引验证记录（旧产物被拒 + 指引文案，两条路径实测）
- [x] 交付说明逐条「跑了/没跑 + 原因」（AGENTS.md §10 手工验证项清单）
- [x] CHANGELOG / ADR / code-map / spec 实施记录与终态一致（spec §9 追加票 06 记录；CHANGELOG/ADR/code-map 版本化终态由票 01–05 提交承载，本票核对无漂移）

## Comments

**2026-10-13 实施完成**。全量重建与回归执行完毕，过程中发现并修复一个**非版本化引入的阻断缺陷**
（guest 事件派发器装配顺序），经用户批准按方案 B（晚绑定共享槽）落地。完整证据与结论见
`spec.md` §9「票 06」条目；要点：

- 重建：`compose-wit.mjs --all` 无漂移；桌面 4 + 移动 3 wasm-app 与全部 fixture 重建（wasmHash 校验一致）。
- 回归：wasm-core **576/576** · 桌面 **288 通过**（lib 155 + 集成 e2e 全绿）· 移动 **355/355** · 能力/引擎 crate 14 个全绿 · 双端 vitest（唯一既有 flake）· 根 eslint 0 error。
- fail-visible：以旧 WIT（`4f7031d81`）现编两个旧产物，实测被新宿主拒载并命中 `stale_artifact_rebuild_hint` 重建指引（`missing required export abi-form.form` / `matching implementation was not found in the linker`）。
- 缺陷修复：`LoadedWasmPlugin` 的 dispatcher 改持与运行时共享的 `Arc<RwLock<..>>` 槽（派发时读取），
  使 `PluginHost::new` 扫描期实例化后注入的派发器即时生效；同步补三处宿主测试 harness 缺口
  （`pty_session_chain` / `ws_auth_rules` 从未注入；`session_e2e` 显式内核同名 `setup_wasm_runtime` 遮蔽宿主包装）。
- 边界：端事实收口（HostFactsPort）与 domain-owned-contracts（0.2.0 bump）等并行在途改动未触碰；
  最终产物重建需在 domain-owned-contracts 落定后按其版本 bump 重跑（其版本锁已点名待更新面）。

## 门禁

- CI 同判据：`lint.yml`（eslint 0 error）+ `test.yml`（两端 cargo test + vitest）
- 交叉端测试（`cross-end-tests`）：本任务未动跨端协议/认证/会话面——与本任务无涉，写明理由跳过
- husky pre-commit（doc-tracking + 暂存前端 eslint）

## 注意

- 集成测试编写 + 全量套件回归集中在收口执行（AGENTS.md §10 两段式）。
- 若重建暴露版本化后的行为不一致（非预期），停下问用户（spec §8「不顺手改语义」）。
- 命令字眼以 `docs/commands.md` 为准。
