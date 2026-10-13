# T-04 产物重建与门禁验证证据（2026-10-13）

契约包 `0.1.0 → 0.2.0` 后全量产物重建与针对性门禁实测记录。

## 产物重建

| 产物 | 命令 | 结果 |
| --- | --- | --- |
| 桌面 4 插件（agent-hub / ai-chatbox / file-transfer / terminal-session） | `cd bedcode-desktop && pnpm run plugins:build` | 全部成功，`resources/plugins/desktop/*` 时间戳更新（file-transfer wasm 997349 B，wasmHash 重注入） |
| 移动 3 插件（ai-chatbox / file-transfer（共享产物源在桌面 app） / terminal-session） | `cd bedcode-mobile && pnpm run plugins:build` | 全部成功，`resources/plugins/mobile/*` 更新 |
| 桌面 wasm-core 夹具（SDK 7 feature + system-test） | `cd packages/bedcode-wasm-core && cargo test --lib fixture` | 8/8 绿（重建落 `bedcode-desktop/target/fixtures/`） |

file-transfer 双端共享产物字节一致由 `shared_artifact_probe::both_ends_ship_byte_identical_artifact` 断言（4/4 绿）。

## 旧产物拒绝（fail-visible ①②）→ 重建后复验

见 `stale-artifact-rejection.txt`：重建前旧 0.1.0 产物在移动宿主实例化期被 linker 拒绝
（点名 `bedcode:plugin/host-storage@0.1.0` + `stale_artifact_rebuild_hint` 文案）。
重建后同一用例复跑：

```
cd bedcode-mobile/src-tauri && cargo test --test shared_artifact_probe
test result: ok. 4 passed; 0 failed; 0 ignored
```

## 针对性门禁（T-04）

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| WIT 漂移锁 | `pnpm run check:wit` | 双端 11 + 8 生成物全 ok |
| 内核交集 + 防回接锁 | `packages/bedcode-wasm-core`：`cargo test --lib platform` / `sliced_interfaces` / `binding_surface` | 7/0、1/0、1/0 |
| 契约版本锁 | `bedcode-desktop/src-tauri`：`cargo test --test wit_package_version_lock` | 5/5 绿（含 `notify.wit` 真源、routing EXPORTS 0.2.0） |
| 桌面宿主 lib 单元 | `cargo test --lib` | 157/0 绿 |
| 桌面宿主 e2e / 白名单 | `http_e2e` 1/0 · `ws_e2e` 6/0 · `system_component_test` 10/0 · `owner_e2e` 5/0 · `pty_wiring` 6/0 · `capabilities_lock` 9/0 · `sdk_e2e` 3/0 · `component_e2e` 2/0 | 全绿 |
| 移动组合/纯净锁 | `mobile_wit_composition_lock` 3/0 · `mobile_wasm_core_purity_lock` 7/0 | 全绿 |
| 能力域 crate | `packages/bedcode-discovery-engine`（routing 4/0）· `bedcode-file-transfer-core` 76/0 | 全绿 |
| wasm 应用 | file-transfer `cargo test` 83/0（含 mount_local 5/0）| 全绿 |

## 已知基线红（非本票引入）

- `bedcode-desktop/src-tauri/tests/capability_crates_no_product_ids.rs::every_registered_crate_is_present_and_scan_coverage_is_complete`
  在本票改动前即红：`packages/bedcode-headless-host-probe` / `packages/bedcode-host-api-core`
  两个 tracked crate 未登记进 `SCANNED_CRATES` / `PENDING_SCAN_CRATES`（`git show HEAD:...` 同样不含）。
  属并行线（runtime-single-form / 能力域脱绑）的登记面，本票只补登 `bedcode-notify-engine`。
