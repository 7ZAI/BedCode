# 09: 文档与交付义务

**What to build:** 本特性的术语、边界判据与变更记录全部落到仓库的单一事实源，后续维护者不必翻会话记录就能判断「宿主替插件决定授权策略」是否被允许。

**Blocked by:** 08

**Status:** done（2026-09-28）

- [x] `CONTEXT.md` 新增「授权策略」「授权记录」两条词条 + 与既有「目录授权」的关系说明（**既有定义一字不动**）
- [x] `docs/adr/0022-...` 补一段「授权策略 = 安全闸门，非业务默认值」
- [x] WIT「不 bump 的语义变更」段登记本次（沿 v22 总线 topic 先例）
- [x] `CHANGELOG.md` + `CHANGELOG_zh.md` 条目
- [x] `bedcode-desktop/docs/code-map.md` 的 security 段更新（补策略层与授权记录面）
- [x] 防回接锁：卸载清空 / 停用保留（spec §8.3）——它与「不兼容存量用户」是两件事，容易被后人「顺手统一」掉
- [x] spec §12.2 的全部变异自检清单逐条跑过并记录结果

## 实现记录（2026-09-28）

**文档落点**

| 内容 | 位置 |
|---|---|
| 两条新词条 + 关系说明 | `CONTEXT.md` 新增「### 授权策略 (Authorization Policy)」分组（在「插件宿主能力」之后）。**既有「目录授权」词条一字未动**，关系写在新分组里 |
| 边界判据（8 条）+ 双端偏离 + 防回接 | `docs/adr/0022-...` 新增「## 授权策略 = 安全闸门，不是业务默认值（2026-09-28 桌面端，ABI 不变）」节 + 修订记录一条 |
| 不 bump 登记（第三例） | `bedcode-desktop/packages/plugin-sdk-desktop/rust/wit/bedcode.wit` 的「不 bump 版本号的语义变更」段 |
| 变更记录 | `CHANGELOG.md` / `CHANGELOG_zh.md` 的 `### Features` / `### 功能` 顶部各一条（覆盖票 01–09 全部内容） |
| 代码地图 | `bedcode-desktop/docs/code-map.md` 的 `security/` 段新增「授权策略 / 授权记录（2026-09-28）」子条目，四个文件各自职责 + 生命周期 + 界面落点 |

**ADR 里最关键的三句**（否则下一个人会重新怀疑这件事是否越线）：

1. 策略**只决定是否询问**，不决定权限位是否生效、不放松任何硬闸门 ⇒ 属 §5.1.3
   「安全闸门」，不是 B5「业务默认值」。
2. **manifest 不得声明档位**（加载期显性报错）：策略由用户设；让插件在 manifest 里
   声明自己「该被信任到什么程度」是同一个越线问题的镜像。
3. **「忽略权限」是错误措辞**（spec §4.1 已否决）：任何档位都放行不了 manifest 声明门、
   SSRF/重定向、规范化失败、配额、属主隔离。安全界面宁可低估，不可高估。

**防回接锁（新增代码）**

- `AuthPolicyStore::purge_plugin`（清两表，返回被清行数并 `info!` 留痕）——
  装在 `PluginHost::uninstall_plugin`，紧邻既有的「撤销审批记录」，**不跟着
  `storage.clear_all` 走**（那是插件私有存储，这两张表在主库）。
- 锁：`manager::host::tests::lifecycle_test::auth_records_survive_deactivate_and_are_purged_on_uninstall`
  ——停用后记录与策略都在（否则「停了再开」= 全部重新弹窗）；卸载后两者皆空
  （重装即全新授权）。变异：卸载去掉 purge ⇒ 转红（实测 1 项）。

## spec §12.2 变异自检全表（本次逐条实测，含本会话重跑的 fs 侧）

| # | 变异 | 落点 | 杀死（实测） |
|---|---|---|---|
| 1 | `always_ask` 改回读记录 | network `authorize_outbound` 去掉 `uses_records` 闸门 | **2**：`always_ask_prompts_again_despite_an_allow_record`、`both_decision_faces_agree_on_every_tier` |
| 1' | 同上（fs 侧） | fs_auth `StrategyStep::Ask` 分支不再提前返回 | **3**：`always_ask_skips_legacy_fallback_too`、`always_ask_skips_new_table_records_and_asks_again`、`prompt_payload_carries_the_frozen_strategy` |
| 2 | `always_allow` 改成询问 | network 策略层把 `AutoAllow` 退化为 `Ask` | **4**（含票 05 的 `allow_record_releases_without_prompt`——两档语义一改，默认档的旧契约也一起塌） |
| 3 | deny 移到第一方层之后（fs） | fs_auth 把 `deny_hit` 判定挪到 `first_party_dir_matches` 之后 | **2**：`revoke_records_deny_that_wins_over_first_party_dir`、`removing_the_revoke_record_restores_the_first_party_exemption` |
| 3' | 策略层挪到 deny 之前（network） | network 把策略块移到 deny 判定之前 | **1**：`always_allow_never_overrides_a_deny_record` |
| 4 | legacy 短路条件反转 | fs_auth `Some(_)`（记录命中但 ops 不覆盖）也回退 legacy | **1**：`record_without_op_short_circuits_legacy_fallback` |
| 5 | SSRF 闸门放到策略之后 | `redirect_decision` 恒放行 | **2**：`granted_origin_does_not_open_public_to_private_redirects`（本票新增的链级用例）+ `redirect_decision_blocks_public_to_private` |
| — | 「总是询问」仍落 allow 记录 | network `lands_allow_record` 恒真 | **1**：`always_ask_allow_leaves_no_record_behind` |
| — | 卸载不清记录 | `uninstall_plugin` 去掉 purge 调用 | **1**：`auth_records_survive_deactivate_and_are_purged_on_uninstall` |
| — | 设置页隐藏第一方分区 | 删掉 `AuthorizationView` 的该分区 | **5**（票 08 的 5 条前端用例） |
| — | 两资源共用一个选中态 | 策略控件把 resource 写死 `fs` | **2**（票 06 的 2 条独立性用例） |

**为 #5 补的链级用例**：`host_api::http::tests::granted_origin_does_not_open_public_to_private_redirects`
——原先只有 `redirect_decision` 的纯函数断言，spec §12.2 明令「每条用例必须驱动真实判定链」。
新用例起一个 302 → `169.254.169.254` 的夹具服务器（首跳用 `http://localhost:<port>`：
`is_private_target` 按 host 能否解析成 IP 判私网，`localhost` 判为公网，正是公网首跳 + 私网
跳转目标这一 SSRF 形态的可达替身），在**用户记录命中**与**始终允许档**两种放行下各跑一次，
两次都断言拿到的是 302 本身（跳转未被跟随）。

**#lesson（补链级用例时的两个坑）**
- 夹具的 302 响应头要能在 `while let accept()` 循环里复用 → 得 `Arc<String>`，否则
  `use of moved value: head`（第一次就撞上了）。
- 「私网目标一律被拒」是**错的**期望：局域网文件传输直连私网是本仓既有合法行为
  （`client_for` 为私网专设直连客户端）。SSRF 闸门真正拦的是**公网 → 私网重定向**。
  写用例前先读 `is_private_target` / `redirect_decision` 的真实语义，否则会写出一条
  恒真或恒假的断言。
