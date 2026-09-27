# 06: 网络侧两档策略

**What to build:** 文件与网络两侧的策略档位齐全：网络也可设为「总是询问」（每次新地址都问）与「始终允许」（不问直接放行）。

**Blocked by:** 04, 05

**Status:** done（2026-09-28）

- [x] `always_ask` / `always_allow` 在网络侧与文件侧语义一致
- [x] 同 origin 合并规则在两档下都生效
- [x] 设置页两资源的策略控件**独立设置、独立展示**
- [x] 总览页风险排序把任一资源为 `always_allow` 的应用置顶

## 测试纪律

- [x] 变异自检：网络侧任一档位退回默认语义，必须杀死 ≥1 项（实测 4 个 Rust 变异 + 1 个前端变异，见下）

## 实现记录（2026-09-28）

**落点**

| 内容 | 位置 |
|---|---|
| 网络侧三档接线（删掉票 05 的「非默认档显性报错」守卫） | `src-tauri/src/wasm_core/security/network_auth.rs`（`authorize_outbound` / `authorize_outbound_quiet` 第 2 步） |
| 策略求值（**两个判定面共用一个 `read_strategy`**） | 同上 `read_strategy`（内部转调 `security::strategy::evaluate`，与 fs 侧同源） |
| 「总是询问」档的落账口径 | `OriginPrompt.lands_allow_record` + `respond` 的 `AllowOnce if !lands_allow_record` 分支 |
| 「始终允许」档留痕 | `land_auto_allow`（落 origin，`source='always_allow'`） |
| 容量丢弃计数接线（票 05 遗留缺口） | `NetworkAuthChecker::set_monitor` → `manager/host.rs` 与 `manager/host/tests/scaffold.rs` |
| 档位提示文案修正（见下「口径修正」） | `src/locales/{zh-CN,en}/settings.ts` `strategyControl.hint.always_ask` |
| 弹窗文案随档位变（**弹窗解释与实际行为必须同源**） | `network_auth.rs` 事件载荷加 `remembers` 布尔 → `NetworkAuthDialog.vue` 换说明句 → `desktop.plugin.netAuthScopeOnce`（双 locale） |

**三处必须成对落地的语义裁定**

1. **策略层与 deny 记录的相对位置**：`deny` 判定仍在**策略层之前**（spec §6.1 第 1 步）。
   票 05 时代网络侧只有默认档，这层顺序无从体现；档位一多，「始终允许」就成了一条
   绕开硬拒绝记录的现成路径（票 05 那条 `non_default_network_strategy_fails_visible`
   用例正是这条缺口的产物，现已被真实档位测试取代）。
2. **「总是询问」档不落 allow 记录**（fs 侧「不提供记住按钮」的网络对偶）：该档不读
   记录，落一条永远不会被命中的 allow 记录，只会在管理界面显示一条假的「用户已授权」。
   `DenyAlways` **仍落** deny 记录——deny 在更靠前的第 1 步生效，任何档位都尊重它。
   `lands_allow_record` 与 `step` 拆成两个字段正是因为 `Step::Ask` 有两种来由
   （「总是询问」主动跳过 vs 档位读失败 fail-safe 退化成询问，后者**要**落账）。
3. **档位读失败 = 询问 + 落账**，不是报错也不是放行：与 `fs_auth::decide_without_dialog`
   同向（放行方向才是危险的一侧）。两个判定面共用同一实现，不存在双答案。

**留痕粒度 = origin**（不是 path）：询问粒度本就是 origin（票 05 定的），落更细的
path 会出现「界面写 `/v1`、库里是整站」的形状不一致；落更粗不存在（整站就是该粒度）。

**口径修正（i18n）**：`strategyControl.hint.always_ask` 原写「每次访问**未记录的**目标
都会询问」，但该档恰恰跳过全部 allow 记录——已授权过的目标也照问。票 06 是该文案第
一次对用户可见（网络侧档位此时才真正接线），故一并纠正为「每次访问都询问（已授权过
的目标也照问）」/「Ask on every access, even for targets you already granted」。

**弹窗文案必须随档位变（票 06 实施中发现的第三处口径）**：`NetworkAuthDialog` 原本
无脑写「同意后，该地址以后不再询问」。本票裁定「总是询问」档下允许**不落账**之后，
这句话在该档下就是**骗用户**（点了允许，下次（合并窗口过后）还会再问）。修法：事件
载荷带 `remembers`（= 弹出时档位是否读记录，`OriginPrompt.lands_allow_record` 原样透出），
弹窗据此换 `netAuthScopeOnce`（「只放行这一批请求」）；字段缺失按 `true` 处理（宿主
早于该字段时不凭空改变旧行为）。**fs 侧同款**（`PendingRequest::offers_remember`），
两侧同源于「用户看到的按钮与实际落账必须一致」这一条。

**测试（12 条新增，全走真实判定链）**

`always_ask_prompts_again_despite_an_allow_record`（C2 正例）·
`always_ask_still_honors_deny_records_without_prompting`（C1 反例）·
`always_ask_allow_leaves_no_record_behind`（落账口径）·
`always_ask_keeps_the_deny_the_user_ever_gave`（deny 边界）·
`always_ask_still_merges_one_prompt_per_origin_batch`（C9 在该档仍成立）·
`always_allow_releases_unrecorded_origin_and_records_it_as_unconfirmed`（C3 正例，
URL 带 query 并断言落库 target 不含 query）·
`always_allow_never_overrides_a_deny_record`（C8 策略侧）·
`always_allow_keeps_allowing_when_the_record_cap_is_reached`（C3 边界：放行方向不变
+ core-monitor 计数，顺带锁住 `set_monitor` 接线）·
`both_decision_faces_agree_on_every_tier`（6 组档位 × 是否有记录的矩阵，两个判定面同源）·
`strategy_read_failure_falls_back_to_asking_and_still_records`（fail-safe 退化）·
前端 `AuthorizationView 两类资源的档位互相独立`（2 条：写库只带该资源、选中态各自独立、
「已是当前档位」的免写判定逐资源进行）· `NetworkAuthDialog` 两条（`remembers=false`
换说明句 / 缺字段回落旧文案）。

**变异自检实测（`security::network_auth` 过滤跑，26→35 项基线）**

| 变异 | 杀死 |
|---|---|
| M1 `always_ask` 改回读记录（去掉 `uses_records` 闸门） | 2 项（`always_ask_prompts_again_despite_an_allow_record`、矩阵用例） |
| M2 `always_allow` 退化成 `always_ask` | 4 项（含票 05 的 `allow_record_releases_without_prompt` —— 说明两档语义一改，默认档的旧契约也一起塌） |
| M3 `lands_allow_record` 恒置真 | 1 项（`always_ask_allow_leaves_no_record_behind`） |
| M4 策略层挪到 deny 判定**之前** | 1 项（`always_allow_never_overrides_a_deny_record`） |
| 前端：策略控件把 resource 写死 `fs` | 2 项（新增的两条独立性用例） |
| 前端：弹窗恒用「以后不再询问」文案（忽略 `remembers`） | 1 项（`remembers=false` 换说明句那条） |

**#lesson（本次踩到的）**：变异脚本里 `open('network_auth.rs','w')` 用了相对路径，
而循环的 cwd 是 `src-tauri` 而非 `security/`——脚本**静默新建了一个错位文件**，
四个变异「全绿」全是假的（跑的是未变异的源码）。脚本末尾补了「落地即与备份比对、
相同则非零退出」的断言后才暴露出问题。凡是批量变异脚本，写文件一律用绝对路径 +
落地自检。
