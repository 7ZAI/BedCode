# 05: 契约收口（锁更新 / 集成测试指向 / CHANGELOG 双语 / ADR 0037）

**What to build:** 见 spec §6（行内）与对应 spec 章节。

**Status:** done

**Type:** task

## Comments

- 2026-10-06 实施（接 02-04 合并执行后）：
  1. **hot_path_logging_lock.rs**：`LOCKED_SITES` 两条 `src/wasm_core/{bus,security/fs_auth}.rs`
     → `../packages/bedcode-wasm-core/src/{bus,security/fs_auth}.rs`（随整核迁移，C-001~C-003 绿）。
  2. **crate_boundary_lock.rs 收口为单向引用（lib → crate）**：`SPLIT_CRATES` 登记表
     **上提 `bedcode_wasm_core::crate_boundary_lock`**（单一事源），lib 侧 `pub(crate) use`
     再导出 + `server_lib_src_roots()` 薄委托；登记表新增 `bedcode-wasm-core` 自身
     （拆分产物清单含整核本体）+ `ALLOWED_DOWNWARD_EDGES` / `REQUIRED_DOWNWARD_EDGES`
     各 8 条（base / crypto-engine / host-kit / server-core / 四能力域）。
     断言①~⑤ 全绿（8 passed）。
  3. **新增结构锁 `tests/wasm_core_whole_crate_lock.rs`**（3 用例：目录无实现文件 +
     lib.rs 垫片是 `pub use` 非 `mod` + 扫描器防空转自检）——复用 enums.rs 反双份纪律。
  4. **集成测试指向核对**：4 个集成测试（broadcast_shutdown / pty_session_chain /
     ws_auth_rules / http_auth_biometric）全绿；wasm_bridge_bench/support.rs 两条
     WASIP3/fixture_target 文档注释改指 crate 内路径；`rg "src/wasm_core"` 在 lib 侧
     仅剩新结构锁自身的路径描述（验收达标）。
  5. **CHANGELOG.md + CHANGELOG_zh.md 双语条目**（ADR 0037 摘要：M1-M11 / 单端口 /
     注册表单入口 / 验证数字）。
  6. **ADR 0037 落档**（`docs/adr/0037-wasm-core-whole-crate.md`：D1-D11 全表 /
     M1-M11 清单 / fail-visible 三形态 / 不回归承诺 / out of scope）；
     ADR 0035 头注补充「自然后继 = ADR 0037」十字引用。
  7. **文档**：`bedcode-desktop/docs/code-map.md`（packages 树 + src-tauri 树 +
     §2 整核导引 + §3 能力表 + §5/§6/§8 落点 + 快速导航 + 防回接锁索引全部改指 crate）、
     `docs/knowledge/build-process.md`（target/host-kits 桶增 bedcode-wasm-core +
     验证段落）、`docs/commands.md`（crate 级 cargo test 示例）。

## Blocked by
- 02-04（已完成）

## 验证
- crate_boundary 8 passed（含新登记表边）；wasm_core_whole_crate_lock 3 passed；
  hot_path_logging_lock 3 passed；capabilities_lock 7 passed；empty_dir_lock 4 passed；
  退役面锁（retired_* 9 条，crate 侧经新登记表 server_lib_src_roots）全绿；
  4 个集成测试全绿
