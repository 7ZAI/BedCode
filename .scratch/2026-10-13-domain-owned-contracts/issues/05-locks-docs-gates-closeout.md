# 票 05 · 锁 / 文档 / CHANGELOG + 全量门禁收口

**来源**：`.scratch/2026-10-13-domain-owned-contracts/spec.md` §6、§8、§9。

**Blocked by**：02、03、04（终态稳定后一次收口）。

**Status**：**resolved**（2026-10-13；实施记录与全量门禁结果见文末 Comments）。

## What to build

1. **新增防回接锁**（落桌面/移动宿主 tests，按各端既有锁风格）：
   - **core 机制纯净锁**：`core.wit` 只允许 8 import + 4 export 名单；任何业务能力 interface 定义回流即红（点名 interface）。
   - **域契约组合锁**：compose caps 的域映射断言（mdns/peer/http/ws → 域 crate 文件；端扩展 → 端 wit-src），含「移动 caps 不含端扩展」判据。
   - **能力域端中立锁**：能力域 crate 默认形态零端 WIT / 零端专属 impl（扩展 `common-host` 面判据）。
2. **既有锁更新与注释修正**：
   - `wit_package_version_lock.rs`：源文件清单/注释更新（纠正「pty / http / ws 会被双端 compose 拷贝」——实际移动只组合 cap-mobile；core_binding 语义随 core 收窄更新）。
   - `mobile_wit_composition_lock` / `mobile_wasm_core_purity_lock`：口径与新 core 面同步（默认导出的 4 域实现位置注释）。
   - 桌面 `bindings.rs` cfg(test)：白名单双向锁在新 caps 世界下复核（含端扩展路径 B 模块）。
3. **文档收口**：
   - `AGENTS.md` §5.4：核心 WIT 描述（「17 个双端全等 interface」→ 新口径；core 收录判据指向 ADR 0052）。
   - `docs/adr/0045`（D3 修订指引）/ `0049`（D2 保留 + 计数口径）加修订注记（不改历史结论正文，只加指向 0052 的 note）。
   - 两端 `docs/code-map.md`：WIT 真源/分片/组合面、防回接锁索引。
   - 全仓「17 个」口径与相关注释逐条核对修正（`rg "17 个.*interface"`）。
4. **CHANGELOG 双语**：归位 + 收窄 + 合并的完整条目。
5. **全量门禁（AGENTS §10，一次收尾）**：双端 `cargo test` 全量；两端 `pnpm run test:run`；根 `pnpm exec eslint .` 0 error；`pnpm run check:wit`；`cross-end-tests`（涉及共享产物/协议面时）；`cargo fmt` / `clippy` 自查；未接入自动化的手工项逐项「跑了/没跑 + 原因」（wasm 应用完整构建含 wasmHash、真机/浏览器核验等）。

## 验收标准

- [x] 新锁三把落地并做过变异自检（改坏即红、还原即绿）——见 Comments（变异自检 3/3）
- [x] 全仓无「17 个」陈旧口径；注释与代码逐条对得上（AGENTS §6）——历史 ADR 正文保留并加 0052 注记
- [x] 双端全量测试 + 前端全量 + eslint + check:wit 全绿，结果贴票——四例基线红已逐项证据化（非本票引入）
- [x] CHANGELOG 双语同步；ADR 0045/0049 注记指向 0052

## 门禁

- 全量结果必须**实际运行**并贴出；跑不了的写明原因与风险（AGENTS §10）
- 锁清单同步登记进两端 code-map 的防回接锁索引（「新增退役面时同步补锁与索引」）

## Comments

### 2026-10-13 · 收口完成（三把新锁 + 文档口径全扫 + 全量门禁）

**新锁三把（`bedcode-desktop/src-tauri/tests/domain_contract_structure_locks.rs`，三判据）**
- ① `core_wit_stays_mechanism_only`：`core.wit` interface 集 == 8 import + 4 export 白名单；`world core` import/export 集逐项钉死（`events-binary` 不在 world）。
- ② `compose_caps_map_to_the_right_contract_houses`：双端 caps 值逐条对照域契约房 / 端点 wit-src；移动 caps 不含任何端扩展 / 桌面独有键（含 `pty`）；每个 cap 真源必须声明 `world cap-<key>`。
- ③ `capability_domains_stay_end_neutral`：域 crate 的 `bindgen!` / `impl bedcode::plugin::host_*` 必须紧邻 `#[cfg(feature = "common-host")]`、域分片 interface 仅限共享白名单、生产代码零端扩展词汇。
- **变异自检 3/3**（改坏即红、点名地点，还原 sha 一致后绿）：① core.wit 注入 `host-evil` → 红点名 `["host-evil"]`；② 移动 caps 注入 `pty` → 红点名；③ mdns.wit 注入 `host-fs-mobile` → 红点名。

**既有锁/注释更新**
- `wit_package_version_lock.rs`：WIT 源清单 10→11（补 `notify.wit`）、生成物下限计数与注释同步、域分片去向口径更新；补齐 `routing.rs` EXPORTS 的 `@0.2.0`（该格式是 `bedcode:plugin/host-mdns.browse@0.2.0`，与 `bedcode:plugin@…` 前缀不同——bump 排查时容易漏）。
- 内核 `contract.rs` 的版本字面量断言改为由 `PACKAGE_VERSION` 派生（同类漏改的根治法）；`capability.rs` / `discovery-engine/routing.rs` 注释同步。
- `mobile_wasm_core_purity_lock.rs` 判据 1 口径更新（notify 为共享域契约的移动实现 + `host-notify-mobile`，不再是「移动独有域」）。
- `empty_dir_lock.rs`：源码根约定 `bedcode-mobile/plugins/*/src` 是 2026-10-08 目录改名前的旧路径（新路径 `wasm-apps/*/src`）→ 修正；连带清理两处空的遗留目录 `bedcode-mobile/src/__tests__/{config,views}`。

**文档收口**
- `AGENTS.md` §5.4：core 口径改「机制必需 8 import + 4 export，ADR 0052 D1」，caps 列举补齐 mdns/peer/http/ws/notify，移动独有域列表改为含 `host-notify-mobile`。
- `docs/commands.md` §5.6：真源/分片/组合面/源文件计数（10→11）与 bump 动作同步。
- `docs/knowledge/plugin-development-checklist.md`：core 口径 + `pick-folder` 退役 + `fs:pick` 行 + 2026-10-13 变更注记。
- ADR 0045（状态行 + D3 加 0052 取代注记，D1/D2/D4 保留）、ADR 0049 D2 计数口径注记。
- 两端 `docs/code-map.md`：WIT 真源/分片/组合面与防回接锁索引更新（新锁三把登记；桌面宿主扩展表改 `{fs,platform}` + 共享 `notify`；移动 WIT 生成物段重写）。
- 全仓「17 全等」口径清零（仅 ADR 历史正文与 CHANGELOG 历史条目保留，均已带指向 0052 的注记/本为历史）。

**全量门禁（实际运行）**
- `pnpm run check:wit`：双端全 ok。
- wasm-core `cargo test --lib`：**577/577**。
- 桌面宿主 `cargo test --no-fail-fast`：**35 个测试二进制绿**（含 lib 157/157、http_e2e/ws_e2e/system_component_test/owner_e2e/pty_wiring/capabilities_lock/sdk_e2e/component_e2e 等）；仅 2 个二进制红：`capability_crates_no_product_ids`（1 例）与 `capability_crates_unit_tests_only`（2 例）——均为 `bedcode-host-api-core` / `bedcode-headless-host-probe` 未登记/带 crate 根 `tests/` 的**既有基线红**（`git show HEAD` 同判据 + 目录在 HEAD 在场，属并行线登记面），本票只补登 `bedcode-notify-engine`，未替他线做处置决定。
- 移动宿主 `cargo test --no-fail-fast`：**全绿**（lib 226 + 全部集成，含 `shared_artifact_probe` 4/4 · composition 3/3 · purity 7/7）。
- 能力域 crate：`discovery-engine --features common-host` 35/35（routing 4/4）；`file-transfer-core` 76/76；wasm 应用 file-transfer 83/83（mount_local 5/5）。
- SDK：移动 `cargo test --features wasm` 全绿；桌面 `cargo test --features wasm` 158 passed / 4 failed（4 例均为 `wire/key.rs` 遗留格式，HEAD 同判据同红——基线红，与本改造无关）。
- 前端：桌面 `pnpm run test:run` 116/117 文件（唯一红 = `terminal-session` `terminalPreview` resize 重试 flake，HEAD 既有，见 T-03 记录同款）；移动 78/78 文件 / 895 用例全绿。
- 根 `pnpm exec eslint .`：**0 error**（97 warnings，既有）。
- `cross-end-tests`：**当前不编译**（并行线在途）：`tests/common/desktop_ctx.rs` 仍按旧 `PluginHost::new` 6 参 / `AppContext::db()` tokio Mutex 调用，且移动侧 `session::http` / `terminal_link` 已随移动重构退役——这些 API 变更来自 domain-owned 之外的在途线（host-facts 单形态 + 移动重构），本票未触碰其源。共享产物面已由 `shared_artifact_probe`（同字节 + 桌面构建产物在移动宿主实例化）代替覆盖；跨端协议面本改造零改动。
- 手工项：wasm 应用完整构建（含 wasmHash 注入）已跑（`plugins:build` 双端）；真机 / 浏览器核验未跑（无设备，且本改造无前端呈现面变更）。

**附：基线红清单（非本票引入，均已证据化）**
1. `capability_crates_no_product_ids` / `capability_crates_unit_tests_only` ← `bedcode-host-api-core`（crate 根 `tests/boundary_lock.rs` 在 HEAD 在场）+ 两 crate 未登记。
2. 桌面 SDK `wire/key.rs` 4 例遗留格式（HEAD 同红）。
3. 桌面 vitest `terminalPreview` resize 重试 flake（T-03 已记录、HEAD 同款）。
4. `cross-end-tests` 编译红（host-facts / 移动重构在途 API，见上）。
