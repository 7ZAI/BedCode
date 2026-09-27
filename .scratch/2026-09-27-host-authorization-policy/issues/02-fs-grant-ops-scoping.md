# 02: 文件授权按操作拆分（旧记录零感知 + 可撤销）

**What to build:** 用户授权某目录的**读**之后，插件对该目录的**写**会再次弹窗询问。旧版已授权的目录读写仍照常放行（存量用户零感知）。设置页能列出该应用的文件授权目录并逐条撤销。

**Blocked by:** 01

**Status:** done（2026-09-28）

- [x] 判定规则按 spec §5.2 的 resolve()：records 命中优先（ops 不含此操作即拒），未命中才回退 legacy flat（视作 read+write）
- [x] 正例：授权 read 后对同目录 write 再弹一次
- [x] 反例：某子树在 records 中 `ops=["read"]` 时 write 被拒——**即使 legacy 有同前缀**（短路条件）
- [x] 三个落账入口（`check` / `check_batch` / `authorize_picked`）写新表，`ops` 取本次请求的操作集
- [x] `remember=false` 的一次性放行**不落账**
- [x] `fs_granted_paths` 退化为**只读**回退，不再被写入
- [x] 撤销命令：删除该目标 allow 记录 + 落一条 `effect='deny'` 记录（spec §8.4）
- [x] 设置页文件分区列出目录记录 + 逐条撤销
- [x] 弹窗文案区分读 / 写（i18n 双语）

## 测试纪律（`unit-test-discipline` 强制）

- [x] 变异自检：把 legacy 短路条件反转（集合非空时仍回退）必须杀死 ≥1 项 → **实测杀死 1 项**（`record_without_op_short_circuits_legacy_fallback`）
- [x] 测试**必须驱动真实判定链**，不能只测纯函数。上一批 fs_auth 的教训：只断言纯函数时，「落账粒度改回文件粒度」这个变异 17 项全绿（杀死失败）

## 实现记录（2026-09-28）

**判定链（`wasm_core/security/fs_auth.rs`）**

- 新增操作集 `FsOps`（bitflag：`READ` / `WRITE` / `READ_WRITE`）：库值、日志、弹窗 payload 三处口径共用（`to_wire` / `from_wire` / `as_wire_str`）；`FsOp` 补 `as_str`。
- 免弹窗判定收成**单点** `decide_without_dialog(plugin_id, canonical, needed)`，返回三态
  `Denied`（硬拒绝记录命中）/ `Allowed(layer)` / `Ask`，`check` / `check_batch` / `is_granted` 三条入口共用：
  1. `effect='deny'` 命中（target 是祖先或自身）→ `Denied`（不弹窗、优先于第一方层）；
  2. 第一方集成目录 → `Allowed(FirstPartyDir)`；
  3. 记录命中（allow 行操作集**并集**）**覆盖** `needed` → `Allowed(RecordGrant)`；命中但不覆盖 → `Ask`
     且**短路旧记录**（spec §5.2 第 2 步）；
  4. 无记录命中 → 旧记录（`fs_granted_paths`，视作读写都授权）→ `Allowed(LegacyGrant)`；否则 `Ask`。
- 记录读取一次成对算 deny 命中 + allow 操作集并集（`record_signals`，单次查表）；读库失败**按未命中**处理并 error 日志（不降级放行）。
- 落账：`respond` 按 `PendingRequest.ops` 写新表（`AuthRecordSource::User`），目标 = **规范化路径** + 粒度规则（Exact → 自身；Directory → 目录本身 / 文件取父目录，复用 `directory_scope_target`）。
- `save_granted_path` → **`#[cfg(test)] seed_legacy_granted_path`**：旧表自本票起只剩只读回退（`legacy_prefix_matches`），生产无法再写入（编译期保证）。
- 无弹窗面按各自能力集传参：框架阶段 2 = 本次操作；fs 任务单元 = 写单元要写授权、其余读；WASI 预打开 = 按声明档位（只读档要读、读写档要读 + 写）——否则「授权读」会换来一个可写 preopen。
- 批量入口口径：`check_batch(paths, ops)`（preauth 与 `host-fs.request-auth` 传 `READ_WRITE`，与改造前「无操作维度 = 读写等价」逐字一致）；`authorize_picked` 传 `READ_WRITE`（用户点头的是「这个目录可以用」）。
- 命令面：`plugin_auth_revoke`（删 allow + 落 deny）与 `plugin_auth_remove_record`（只删 deny）——均宿主面凭证；`plugin_fs_auth_respond` / `plugin_auth_overview` 的凭证门抽成 `require_host_surface` 单点。

**真源（`wasm_core/security/auth_policy.rs`）**

- 新增判定读面 `records_for_match(plugin_id, resource)` 与写面 `grant` / `deny` / `revoke` / `remove_deny`；
  `grant` 按 (应用, 资源, 目标) 去重、操作集取并集、来源保留 `user`（不被后续自动放行降级）；`deny` 幂等。
- `AuthRecordSource` 枚举 + `AUTH_EFFECT_{ALLOW,DENY}` 常量（库值单点）；`AuthResource::parse`（未知值显性报错）。
- `PluginStorage::db()` 共享句柄访问器：`FsAuthChecker::new` 据此构造真源，20+ 处构造点零改签名。

**前端**

- `utils/authPolicy.ts`：`recordsOf` / `sourceKeySuffix` / `effectKeySuffix` / `opsKeySuffix`（未知值返回 null，界面回落原文——溯源标错比标丑严重）。
- `views/AuthorizationView.vue`：行可展开（`aria-expanded`）→ 文件目录记录清单（目标 + 效果 + 操作集 + 来源 + 动作），已授权在前、硬拒绝在后；「取消授权」= `plugin_auth_revoke`，「移除拒绝」= `plugin_auth_remove_record`，成功后 `toast.success` + 重新拉取读模型；失败走 `showUserError`（不弹成功提示、不刷新）。
- `components/FsAuthDialog.vue` + i18n：`fsAuthReadWrite`（读 / 写 / 读写三态）、「记住」与目录范围文案带本次操作集（zh / en 同步）。

**验证**

- 变异自检：短路反转 → 杀 1 项；删操作集覆盖检查 → 杀 3 项（`read_grant_asks_again_for_write` / `batch_entry_uses_request_ops` / `record_without_op_short_circuits_legacy_fallback`）。
- Rust 定向：`fs_auth` 29 passed / 0 failed；`auth_policy` 13、`framework` 17、`preauth` 10、`wasi_e2e` 3、`task_e2e` 8、`host_api::fs` 24 全绿；`cargo check --all-targets` 0 error。
- 前端：授权助手 + 授权页 + 授权弹窗 30 passed；全量见当日记录；eslint 0 error。
- **并行会话提示**：同一 worktree 另有会话在实施网络侧（票 05/06，`security/network_auth.rs` 等），期间出现过其 in-flight 编译错误；本票定向测试与全目标编译均在对方编译通过后复跑确认。
