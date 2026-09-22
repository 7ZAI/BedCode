# 15: 插件构建链权限映射表跟演 v23 + 加载期护栏（`plugins:build` 曾整链不可用）

**What to build:** `packages/plugin-sdk-desktop/bin/manifest-gen.js` 的 Rust 权限映射表跟演当前权限
模型（v23 退役 `session:config`、票 02 拆出 `database:main`），并加一道「映射 ⊄ 词汇 = 加载即失败」
的护栏，使同类漂移不再以「构建链报未知权限」的形式才被发现。

**Blocked by:** 无。

**Status:** done（2026-09-22，票 13 实施中发现并当日修复）

## 现状与根因（2026-09-22 实测）

票 13 收尾时按 AGENTS §3 重建插件产物，标准入口直接失败：

```bash
cd bedcode-desktop
node scripts/plugin-build.js --plugin com.bedcode.terminal-session
# [manifest-gen] permissions + session:config（Rust host 调用）
# [plugin-build] manifest ✗ 未知权限: session:config（真源 SDK rust/src/permission.rs）
# [plugin-build] plugin.json 校验失败: 1 个错误 → exit 1
```

根因：`bin/manifest-gen.js` 的 `RUST_PERMISSION_RULES` 是**手写的宿主方法名 → 权限位**表，未跟两批变更：

1. **v23（`fc760448e`）**：`session_config_upsert` / `session_config_delete` 从 ABI 删除，权限位
   `session:config` 同步退役；读取面 `session_config_list` / `session_config_get` 保留为**一次性
   legacy 迁移通道**、改挂 `session:read`。旧表三项全映射到已退役的 `session:config`。
   插件 `plugins/terminal-session/rust/src/config/ops.rs:41,59` 仍在合法调用这两个读取原语
   （迁移 + 宿主降级轨）⇒ 构建链给插件注入非法权限 ⇒ `validateManifest` 判死。
2. **票 02**（同表另一处潜伏项）：主库 SQL 面拆出高危位 `database:main`，但表里
   `db_execute|db_query` 仍映射 `storage` ⇒ 插件「声明成功、运行时被拒」（`storage` 过校验但过不了
   host 权限门）。当前生产插件**零消费者**（无人调主库 `db_*`），故属潜伏缺陷。

副作用取证：`plugin-build.js` 在失败前已执行 `generateManifest`，**改写了源 `plugin.json`**
（注入 `session:config`）——修复前的临时绕过（`plugins/terminal-session/scripts/build.js --rust-only`）
不敢走标准链，且需 `git checkout --` 还原该文件。

## 修法

`README` 不变量：真源是 `rust/src/permission.rs`（生成 `bin/permission-vocabulary.json` / 前端
`permission.vocabulary.ts`），**手抄消费方必须跟演**。本次两处跟演 + 一道护栏：

- 会话读取面：`session_config_list` / `session_config_get` 并入 `session:read` 规则；
  `session_config_upsert` / `session_config_delete` 项**删除**（ABI 已无此函数，留着只会注入死权限）。
- 主库 SQL 面：`db_(execute|query)\w*` → `database:main`（负向后视 `(?<![_\w])` 排除
  `plugin_db_execute` / `plugin_db_query` 私有库形态）；私有 KV / 私有库仍映射 `storage`。
- **加载期自检**：遍历 `FRONTEND_PERMISSION_RULES + RUST_PERMISSION_RULES`，`perm` 不在
  `permissionVocabulary().permissions` 内即 `throw`（错误文案点名权限位 + 规则正则 + 真源路径 +
  需重跑 `gen:permissions`）。手抄表与真源解耦是这类漂移的结构性成因，护栏把它从「构建链报未知权限」
  提前到「加载即失败」。

## 验收与证据（2026-09-22）

- [x] 映射表零漂移：脚本比对映射产出的 11 个权限位全部 ∈ 词汇表（此前 `session:config` 是唯一越界项）
- [x] 标准构建链恢复：`node scripts/plugin-build.js --plugin com.bedcode.terminal-session` **exit 0**
      （`[plugin-build] manifest 校验通过` → rust wasm + 前端构建完成）；源 `plugin.json` 不再被改写
      （`git status` 无插件目录条目）；产物 `plugin.json` 与源**逐字一致**
- [x] **护栏变异自检**：把 `plugin_db` 规则临时改成 `perm: 'storage:bogus'` → 加载即抛
      `manifest-gen: 权限映射表含词汇表外权限 'storage:bogus'（规则 …）` → 已还原（无残留）
- [x] 宿主回归：`cargo test` 全 target 绿（lib **1134 passed / 0 failed**（对侧 `355f8d893` 清 wsl 死码
      后为 1134）+ 8 个集成 target 全 ok；`[skip]` 计数 0）；`cargo check --lib --tests` 0 error
- [x] 无生产行为变更：`database:main` 映射修正对现役插件零影响（无 `db_*` 消费者；主库面仍受 SQLite
      authorizer 表名白名单仲裁，与票 02 一致）

## 遗留 / 非目标

- 不把 `RUST_PERMISSION_RULES` 改成从 `permission-vocabulary.json` 派生：现生成物只含**前端** API 方法名
  （`apiMap` 形如 `session.list`），不含宿主 Rust 方法名；要派生得先扩生成器。已记为可选后续（护栏已覆盖
  「注入非法位」这一最坏后果）。
- 本票不动 `AGENTS.md` 的权限同步点表述以外的东西；宿主 `host_impl` 权限门、词汇漂移锁（票 01）不在范围。

## Comments

- 2026-09-22 立项并当日修复（票 13 实施记录 §6 发现后，用户拍板「修复新发现」）。
- 复用纪律：改动插件产物前先核 `git diff`——`plugin-build.js` 会写源 `plugin.json`（generateManifest），
  这是构建链的已知副作用，出问题先还原再排查。
