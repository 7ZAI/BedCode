# 07: 应用详情页「授权记录」区块

**What to build:** 在应用中心的 wasm-app 详情页能看到该应用已授权的记录——用户已授权 / 免询问自动放行 / 内置免询问 / 硬拒绝四个分区，全部可逐条撤销。现有「权限」区块改名为「申请的权限」以区分。

**Blocked by:** 01

**Status:** done（2026-09-28）

- [x] 详情页新增「授权记录」区块，四个分区齐全
- [x] 免询问自动放行项带「未经确认」标记
- [x] 四分区均可逐条撤销（撤销 = 删 allow + 落 deny，spec §8.4；内置免询问 = 落 deny，spec §7）
- [x] 现有「权限」区块改名「**申请的权限**」（manifest 声明位）；两者保持**并列**，不合并不嵌套
  - 两者是**正交事实**（一个静态声明、一个运行期落账），合并会暗示存在映射关系，而实际一个权限位可对应零条或几十条记录
- [x] 三档策略控件**不出现在详情页**（只在设置页）——避免两处口径漂移
- [x] 详情页与设置页**共用同一读模型** `plugin_auth_overview(pluginId)`，不各写一套查询
- [x] i18n 双语同步；改 UI 前已加载 `frontend-styles` skill
- [x] 组件测试：四分区渲染 + 撤销按钮触发正确命令

## 实现记录（2026-09-28）

**落点**

| 内容 | 位置 |
|---|---|
| 四分区纯函数 + 第一方项展示形态 | `bedcode-desktop/src/utils/authPolicy.ts`（`userGrantedRecords` / `autoAllowedRecords` / `deniedRecords` / `firstPartyDirsOf` / `firstPartyLabel`） |
| 详情页「授权记录」折叠区块 | `src/views/PluginDetailView.vue`（权限区块标题改 `requestedPermissions`；新区块用 `AuthSection` + `AuthRecordRow` 组合，badge = 记录数 + 第一方项数） |
| 分区薄壳组件 | `src/components/AuthSection.vue`（标题 + 空态 + 计数徽标，无业务判断） |
| 记录行组件 | `src/components/AuthRecordRow.vue`（target + ops + source 徽标 + 未经确认标记 + 撤销/移除按钮；与设置页展开行同口径） |
| 宿主 `~/` 展开 | `src-tauri/src/wasm_core/security/auth_policy.rs::resolve_revoke_target`（fs 资源且 target 以 `~/` 开头 → home 绝对路径；home 不可得显性报错不落账） |
| i18n | `settings.authorization.sections.*`（四分区标题/未经确认/空态/第一方形态，两页共用）；`desktop.plugin.section.authRecords` + `requestedPermissions`；删除无引用的 `section.permissions` 旧 key |
| 组件测试 | `src/__tests__/views/desktop/PluginDetailViewAuthRecords.test.ts`（8 项） |
| 分区纯函数测试 | `src/__tests__/utils/authPolicy.test.ts`（新增 4 项） |
| 宿主 `~/` 展开测试 | `auth_policy.rs` tests：`revoke_expands_tilde_home_prefix_for_fs` + `revoke_does_not_expand_tilde_for_network` |

**关键实现事实（grilling 已核实）**

1. **撤销内置免询问（home 形态）的 target 链路**：前端拿不到 `$HOME`，撤销 `~/.agents` 时传 `~/` 前缀给 `plugin_auth_revoke`；宿主 `revoke` 展开成绝对路径再落 deny——判定链 `record_signals` 用 `strip_prefix` 匹配规范绝对路径，不展开直接落账的 `~/` 字符串永远匹配不上（静默撤销无效 = §8 禁止的降级）。home 不可得时显性报错、不落账。
2. **project-segment 形态（`.claude` 任意项目段）不提供撤销按钮**：段名是规则不是路径，落不成可复用的授权记录（spec §7 注释原话），撤销语义归票 08。详情页照常展示（`<project>/.claude` 形态 + 形态徽标）。
3. **三档策略控件不在详情页**：职责切分（详情页 = 看 + 撤销；设置页 = 设策略 + 全量管理，spec §9.2）。本票零策略写入，页面无策略控件。
4. **徽标计数口径**：授权记录区块 badge = 记录总数（allow + deny）+ 内置免询问项数——与统计条「权限数」（manifest 声明位）分属两个正交事实。
5. **未知来源的 allow 记录归「用户已授权」**：`sourceKeySuffix` 返回 null 时按保守口径进用户已授权分区，徽标显示原文——不误标成「免询问自动放行」（溯源标错比标丑严重）。

**验证**

- 前端定向：`PluginDetailViewAuthRecords.test.ts` 8 passed（四分区渲染 / 未经确认标记 / revoke / removeDeny / 第一方撤销 `~/` target / 失败消费层 / 单次拉取）+ `authPolicy.test.ts` 16 passed（新增 4 项分区测试）+ AuthorizationView 两文件 19 passed 无回归
- 前端全量：`pnpm run test:run` 109 passed / 2 failed——2 失败均在 `FsAuthDialog.test.ts`，属工作区在途改动（02/03 票操作集/三态按钮改造未完成，`git diff` 156 行非本票），已确认与本票无关
- eslint：本次改动文件 0 error（测试文件按既有 ignore 惯例跳过）
- Rust：`cargo build --lib` 通过（`resolve_revoke_target` 编译正确）；**`cargo test --lib auth_policy` 无法运行**——宿主 test 目标被工作区既有红阻塞（`fs_auth.rs` 中间态 `allowed/remember` 未定义变量，属 03/04 票在途重构），待该中间态修复后需补跑 `cargo test --lib auth_policy`（含本票 2 条新用例）
- 变异自检（unit-test-discipline）：`autoAllowedRecords` 条件反转变异实测杀死 2 项；分区/排序/firstPartyLabel 六种变异推演均杀死 ≥1 项

**边界与后续（票 08 承接）**

- 内置免询问 **project-segment** 形态的撤销（段名 deny 语义 + 拦截测试）→ 票 08
- 设置页 `AuthorizationView` 是否同样展示内置免询问分区 → 票 08（「设置页与详情页两处都展示该分区」）
- Rust 全量回归（宿主 + wasm 应用）→ 待 03/04 票 fs_auth 中间态落地后统一跑
