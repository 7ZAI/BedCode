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

- [ ] 双端 `cargo test` 全绿（桌面 nightly 项明列，非本票引入的基线红单独注明）
- [ ] 两端 `pnpm run test:run` 全绿；根 eslint 0 error
- [ ] `check:wit` 绿；wasm-apps + fixtures 重建完成（wasmHash 注入）
- [ ] fail-visible 重建指引验证记录（旧产物被拒 + 指引文案）
- [ ] 交付说明逐条「跑了/没跑 + 原因」（AGENTS.md §10 手工验证项清单）
- [ ] CHANGELOG / ADR / code-map / spec 实施记录与终态一致

## 门禁

- CI 同判据：`lint.yml`（eslint 0 error）+ `test.yml`（两端 cargo test + vitest）
- 交叉端测试（`cross-end-tests`）：本任务未动跨端协议/认证/会话面——与本任务无涉，写明理由跳过
- husky pre-commit（doc-tracking + 暂存前端 eslint）

## 注意

- 集成测试编写 + 全量套件回归集中在收口执行（AGENTS.md §10 两段式）。
- 若重建暴露版本化后的行为不一致（非预期），停下问用户（spec §8「不顺手改语义」）。
- 命令字眼以 `docs/commands.md` 为准。
