# 10 — EgressConsentDialog.vue + 懒触发 + 设置页授权查看/撤销 + i18n

**What to build:** 新组件 `src/components/EgressConsentDialog.vue`：授权弹窗（域名/路径/来源插件或调用方展示 + 确认/拒绝 + 「不再询问」勾选，§9 D7 会话级默认 + 可选持久）。请求时懒触发（D8）：ticket 03/06 的 egress 判定返回 NeedConsent → Rust emit `egress_consent_request`（ticket 02 桥）→ 本组件渲染 → invoke 结果回 Rust 裁决（oneshot 超时兜底已 Rust 侧）。设置页新增授权记录查看/撤销入口。i18n 双语言（zh-CN/en，key 同步）。

**Spec:** §5.6 机制要点 4/5、§9 D7/D8、spec §4 前端段

**Blocked by:** 02, 07

**Status:** ready-for-agent

## 前置要求（强制）

- **UI/样式改动必须先加载 `frontend-styles` skill**（AGENTS.md §6 强约束），参考 file-transfer ConsentDialog 先例（`ft-consent-*` 类族）。
- **UI 设计决策（弹窗交互）强制前置查询 `ui-ux-pro-max` skill**（frontend-styles 的强制配对）。

## 关键实现事实（handoff §2/§3.3 已核实）

- 弹窗桥：Rust emit `egress_consent_request`（含域名/路径/来源、request_id）→ 前端渲染 → invoke 结果回 Rust 裁决（oneshot channel，超时兜底视为拒绝）——ticket 02 已定 Rust 侧，本 ticket 接前端侧。
- 来源展示：宿主调用（useUpdateChecker 等）/ 插件（ai-chatbox 等）区分显示。
- 授权记忆：会话级（默认）+ 持久（「不再询问」勾选）→ 存 Rust 持久层（ticket 02），设置页查看/撤销调 Rust 命令。
- i18n：两端 `src/locales/{zh-CN,en}/` key 同步新增；用户可见文案一律 i18n，禁止硬编码（AGENTS.md §6）。

## 实现清单

- [ ] `EgressConsentDialog.vue`：域名/路径/来源 + 确认/拒绝 + 「不再询问」勾选；frontend-styles 规范
- [ ] 监听 `egress_consent_request` → 懒触发弹窗 → 结果 invoke 回 Rust
- [ ] 设置页授权记录查看/撤销入口（调 Rust 记忆管理命令）
- [ ] i18n key 双语言同步（zh-CN + en + schema 若有）
- [ ] 组件测试（EgressConsentDialog：渲染/确认/拒绝/不再询问/超时兜底）

## 验证

- vitest 组件用例全绿；eslint 0 error
- 真机/模拟：未声明 URL 请求 → 弹窗 → 确认放行 / 拒绝拒绝（验收 5）

## Comments

## Comments
- 2026-09-12 完成：`EgressConsentDialog.vue`（Teleport + center-modal 弹窗族，沿用 FsAuthDialog 结构：图标+标题+来源描述+URL 展示+「不再询问」Toggle+拒绝/允许；FIFO 队列处理并发请求，30s 同值计时器到点自动收起——Rust 已按拒绝结算）；App.vue 全局挂载；`EgressSettingsView.vue`（设置→安全→网络访问授权子页：`egress_list_grants` 展示 host+path+时间、ConfirmDialog danger 确认后 `egress_revoke_grants`）；路由 `mobile-settings-egress`；SettingsView 安全组新增入口（地球图标与弹窗同源）；i18n：`mobile.egress.*`（弹窗 9 key）+ `settings.egress.*`（设置页 11 key）zh/en 同步。
- ui-ux-pro-max 检索（`--domain ux` "confirmation dialogs / heading clarity / confirmation messages"）：无专属 consent 条目，结论为沿用代码库既有 FsAuthDialog 授权弹窗族（High 级显式确认 + Heading Clarity 均已满足），未引入新色板/字体。
- 测试：`src/__tests__/components/EgressConsentDialog.test.ts` 10 用例全绿（渲染/允许/拒绝/背板=拒绝/不再询问 persist=true/队列推进/30s 超时收起/request_id 去重）；mock listen+invoke 模式照 integration helpers。
- 超时语义：JS 计时器到点只推进队列不 invoke（Rust 30s 超时已清 pending，回执仅 warn）；重复 request_id 事件去重防悬挂。
