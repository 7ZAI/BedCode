# 票 04 · 功能统一签名落地（minor bump + 双端同步 + 产物重建）

**来源**：`.scratch/2026-10-13-domain-owned-contracts/spec.md` §2 R2/R5、§5、§7。

**Blocked by**：01（合并案冻结）、02、03（契约与实现落点稳定后改签名）。

**Status**：**resolved**（2026-10-13；实施记录见文末 Comments）。

## What to build

按 01 票冻结的合并案逐项落地「功能一致 → 统一成一个函数」（R2），例如（以 01 终稿为准）：

1. **签名统一**：如 `pick-folders` 取代 `pick-folder`（含两端宿主实现、SDK guest 调用面、权限门口径）；如 notify 统一进共享契约（含桌面不支持选项的处置）。
2. **契约版本 minor bump**：`contracts/bedcode-plugin-package.rs` 单点 `0.1.0 → 0.2.0`（ADR 0051 D4：签名变 = minor），双端同步（ADR 0051 D3：file-transfer 共享产物要求双端对齐）；`node scripts/compose-wit.mjs --all` 重拼。
3. **宿主与 SDK 同步**：内核 core_binding / 能力域 crate / 端宿主的 `impl Host`、SDK `rust/src`（wasm.rs 等）的 guest 绑定与调用点全量跟进。
4. **产物重建**：桌面插件产物 + 移动 3 个 app（`--rust-only`）+ file-transfer 双端共享产物；旧产物不得留兼容路径。
5. **fail-visible 三形态证据**：① 旧产物实例化期被 linker 按包版本拒绝；② `stale_artifact_rebuild_hint` 点名目标版本与重建指引；③ 新旧契约面在编译/运行期的差异有针对性用例。
6. **台账**：`CHANGELOG.md` + `CHANGELOG_zh.md` 双语条目（统一项 + bump 记录）。

## 验收标准

- [ ] 01 冻结的每个合并案落地，无「两端变体」残留（同一功能只有一个函数签名）
- [ ] `pnpm run check:wit` 绿；双端 package 版本一致（共享产物对齐）
- [ ] 双端 `cargo check` / 相关 `cargo test` 绿；SDK 编译绿
- [ ] 全部产物重建完成，旧产物拒绝证据入票
- [ ] CHANGELOG 双语同步

## 门禁

- 针对性：双端受影响 crate 的 `cargo test` 过滤 + `check:wit` + 产物构建记录
- 涉及 file-transfer 共享产物时 `cross-end-tests` 必跑（AGENTS §10）
- 命令字眼以 `docs/commands.md` 为准；大编译前 `df -h`

## 注意

- **破坏面必须诚实分类**：禁止为少 bump 把签名变记作追加（ADR 0051 D4 明文禁止）。
- 不顺手新增能力（补移动实现面的范围以 01 裁决为界；超出即停下问用户）。

## Comments

### 2026-10-13 · 实施完成（C1 通知统一 + C2 目录选择统一 + minor bump + 产物全量重建）

**契约面（真源改动）**
- `contracts/bedcode-plugin-package.rs`：`PACKAGE_VERSION` / `PACKAGE` `0.1.0 → 0.2.0`（单点真源；11 个 WIT 源文件 package 行同步、双端重拼 `node scripts/compose-wit.mjs --all`）。
- **C1**：新建 `packages/bedcode-notify-engine`（轻量契约房：`Cargo.toml` + `src/lib.rs` 文档 + `wit/notify.wit`，world `cap-notify`），共享 `host-notify.notify(title, body, options-json)`（严格解析 options、平台不支持项静默忽略）；双端 `compose.json` 增 `caps.notify`。桌面 `cap-desktop.wit` 删 `host-events-desktop` 整 interface；移动 `cap-mobile.wit` 的 `host-notify` 5 函数拆为共享 `notify`（出本文件）+ `host-notify-mobile` 4 函数（cap-mobile）。
- **C2**：core `host-platform.pick-folder` → `pick-folders`（`string[]` JSON，空数组=取消）；桌面 `host-platform-desktop` 删 `pick-folders`（剩 3 函数、权限位清空）。

**实现面（按端落点）**
- 内核：`host_api/platform.rs::platform_pick_folders`（对话框 `pick_folders` + `fs:pick` 门 + `authorize_picked` 同链；补权限门单测）；`core_binding.rs` 的 `host_platform::Host::pick_folders`。
- 桌面宿主：删除 `plugin/events.rs`（模块 + mod 声明），新增 `plugin/notify.rs`（路径 B 模块 `notify` / `host-notify` / 无权限门 / Tauri `plugin:notify` payload 逐字保留 / options 严格解析+静默忽略）；`plugin/platform.rs` 摘除 pick 链（权限位清空）；`plugin/bindings.rs` cfg(test) 锁更新（caps/包版本清单/路径 B 面 10 项）。
- 移动宿主：`plugin/notify.rs` 拆两个 `impl`（共享 `host_notify` + 扩展 `host_notify_mobile`），同模块同权限位；`plugin/desktop_compat.rs` 删 `pick_folders` 桩方法。
- SDK：桌面 `host/notify.rs`（`HostNotify` + `NotifyOptions`）、`HostEvents` 去 notify、`HostPlatform` 合入 `platform_pick_folders`（wasm_host 走 core `host_platform::pick_folders`）；移动 `HostNotify`（notify）+ `HostNotifyMobile`（4 函数）、`HostPlatform` 加 `platform_pick_folders`。
- 应用：file-transfer-core `PlatformPort` 只要 `platform_pick_folders`（必需）+ `platform_reveal_in_dir`（默认 unsupported）；file-transfer `peer.rs`（`pick_download_dir` / `mount_local_mobile` 取首项）、`adapters{,_mobile}.rs`、测试 MockHost；agent-hub `import.rs` / `usage/sources.rs` 取首项；**移动 file-transfer `plugin.json` 补 `fs:pick`**（audit §E.1 疑点收口）。
- 版本字面量扫尾：桌面/移动宿主模块常量、能力域 crate 的 `MODULE_INTERFACES` 与 `discovery-engine/routing.rs` 的 `EXPORTS`（`host-mdns.*@0.2.0`）、SDK 注释全部随 bump 更新；`wit_package_version_lock.rs` 源清单增 `notify.wit`（11 个）。

**验收证据**（详见 `evidence/rebuild-verification.md` 与 `evidence/stale-artifact-rejection.txt`）
- fail-visible ①②：重建前的 0.1.0 产物在移动宿主加载被 linker 点名 `bedcode:plugin/host-storage@0.1.0` + `stale_artifact_rebuild_hint` 指引；③ 全量重建后 `shared_artifact_probe` 4/4 绿。
- 产物：桌面 4 插件 + 移动 3 插件 + wasm-core 夹具 8/8 全重建成功。
- 门禁：`check:wit` 绿；wasm-core `--lib`（platform 7/0 + 两把回接锁）；桌面 `cargo test --lib` 157/157 + `wit_package_version_lock` 5/5 + `http_e2e` 1/1 · `ws_e2e` 6/6 · `system_component_test` 10/10 · `owner_e2e` 5/5 · `pty_wiring` 6/6 · `capabilities_lock` 9/9 · `sdk_e2e` 3/3 · `component_e2e` 2/2；移动 `mobile_wit_composition_lock` 3/3 + `mobile_wasm_core_purity_lock` 7/7 + `shared_artifact_probe` 4/4；file-transfer 应用 83/83（mount_local 5/0）；file-transfer 共享核 76/76；`discovery-engine` routing 4/4。
- **已知基线红（非本票引入）**：`capability_crates_no_product_ids::every_registered_crate_is_present_and_scan_coverage_is_complete` 在 HEAD 即红（`bedcode-headless-host-probe` / `bedcode-host-api-core` 两 crate 未登记，属并行线登记面）；本票只补登 `bedcode-notify-engine`，未替他线做登记决定。

**遗留（T-05）**：三把新锁（core 纯净 / 域契约组合 / 能力域端中立）+ 全仓「17 个」口径与 `wit_package_version_lock` 注释（「pty / http / ws 会被双端 compose 拷贝」已随 T-02/T-04 失真）+ code-map / ADR 0045/0049 注记 + 全量门禁收尾。
