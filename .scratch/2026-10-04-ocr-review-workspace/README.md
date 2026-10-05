# 工作区全量 OCR 审核登记（2026-10-04：dev 在途 5 工作流 + 内核拆分在途）

**审核日期：** 2026-10-04
**审核工具：** alibaba/open-code-review（`ocr review --audience agent`，sensenova / deepseek-v4-flash，v1.12.x 行为）
**审核范围：** dev 分支工作区全部在途改动（68 修改 + 4 新增 + 若干重命名/未跟踪目录，约 +5686/−1929，CHANGELOG 已登记的 5 个工作流 + wasm-core 库化拆分在途 + terminal-session 插件目录化在途）
**审核方式：** 全量一次跑不完（72 文件超时 + 提供方 429 rpm 严重限流），故**按工作流拆 11 批**、每批在一次性 HEAD 克隆中**只拷入该工作流的文件**后 `ocr review`（workspace 模式 → 只看到目标差异）。session 列表见 §7，原始输出在 /tmp/ocr_*.txt。
**登记人/来源：** pi 会话（open-code-review skill），2026-10-04 午后

> **目的**：把本次审核的可操作发现登记为统一修复待办单一真源。**每条目初始状态 = `open`**，修复后改 `done` 并附 commit。
>
> ⚠️ **误报说明（重要）**：按工作流拆批时**互依文件落在 HEAD 版本**（types/i18n/宿主 style.css/ProviderApply.vue 未随被审文件一起拷入），导致 OCR 对同一批文件反复报「缺失的导出/类型/键/CSS」——已对**每一条**都在真实工作树复核，误报一律标 **🚫误报（clone 范围）**，修复时直接跳过，不要浪费时间。
>
> ⚠️ **未纳入本登记的工作**：批 11 之外还有大体积在途改动——`wasm_apps/terminal-session/rust/src/*` 目录化拆分（lib/ output/ jwt/ registry/ 等未跟踪目录 + 大量 rust 修改）、`wasm_core/*` 的 manager 全量改动（58 文件，属 `.scratch/2026-10-04-wasm-core-lib-split/` 票 04 的目录化过程）、`packages/bedcode-host-kit/`（新 crate）、`scripts/{split-rust-tests,audit-rust-tests,run-test-split}.mjs`。这些属**其他在途任务的执行中形态**，未按 §11 触碰；本登记只覆盖 CHANGELOG 5 工作流 + 内核拆分的 security 相关已审文件。

---

## 0. 汇总

| 批 | 工作流 | 文件数 | 评论 | 验证后有效 | 误报 | 状态 |
| --- | --- | --- | --- | --- | --- | --- |
| 1-2 | W4 移动终端链路（重连/心跳/可见性） | 13 | 3 | **3** | 0 | ✅ 3 条真实 bug |
| 3-4 | W1 桌面页面过渡（宿主核心 + wasm 视图） | 29 | 10+4+6 | **6** | 9 | ✅ 6 条有效 |
| 5-7 | W2 agent-hub 供应商应用（Rust 内核） | 24 | 4+10 | **10** | 4 | ✅ 10 条有效 |
| 5-7 | W2 agent-hub 前端 | 10 | 0+0+10 | **10**（与 Rust 批重合归并后见各条） | — | 归并 |
| 8 | W3 构建产物 target-dir | 6 | 2 | **2**（1 已处理） | 0 | ✅ |
| 9 | W5 TaskHistoryView（任务队列扩展） | 2 | 3 | 0 | **3** | 🚫 全误报 |
| 10 | W5b 内核拆分 security/http 相关 | 24 | 11 | **9** | 2 | ✅ |
| 11 | file-transfer 视图/样式 | 2 | 3 | 1 | **2** | ✅ 1 条有效 |
| **合计** | — | **~110 批覆盖** | **52** | **31 有效** | **20** | — |

**跨批高频主题：**

1. **拆批拷文件导致互依文件版本漂移 → 批量误报**（9+4+3+2 = 18 条）。教训：跨工作流拆批时，被审文件引用的类型/i18n/CSS 必须**与其版本一致地拷入**，否则 OCR 会把「HEAD 里没有」当成「代码库里没有」。
2. **W4 重连系统的 3 条全是真 bug 且互相咬合**：backoff 不重置 × 倒计时不递减 × reconnecting 不清理——与 CHANGELOG「已验证」口径不符，建议优先修。
3. **fail-visible 基调在 W2 落地良好**（零模型拒绝、多目标逐项回执、未知/空模型形状显性失败都有），剩余缺口集中在「条目类型校验」与「部分失败的可重试性」。
4. **CSS 兼容层**：`calc()` 乘法（`-1 * var(...)`）是新特性（Chromium 111+/Safari 16.4+），宿主页面过渡体系落 WebView2/WKWebView 老内核时整条声明会被丢弃——建议改减法写法，零成本。

---

## 1. 批 1-2 —— W4 移动终端链路（13 文件，3 条全真）

### Medium

- **[M-01] `bedcode-mobile/src-tauri/src/terminal_link.rs:1059-1060` — backoff 策略成功后从不重置** `link_io` 只在失败路径 `policy.start()`/`get_delay()`，链路恢复 `Live` 后从不调 `on_success()`/`reset()`。`ReconnectManager::retry_count` 在链路整个生命周期累积 → 指数序列爬到 30s 封顶后，之后每次断线（哪怕健康期再断）都直接 30s 起退，而非从 1s 重来。事件通道（`manager.rs:606` 调 `reconnect_policy.on_success()`）用的同一策略有 on_success 语义——终端链路与它声称收敛的单一事实源**行为分叉**。
  > 建议：链路回到 `Live`（`handle_control_text` 的 `subscribed` 分支）时调 `on_success()`，稳定重连后重置退避序列。
  > 状态：**done（2026-10-05 修复批次：subscribed 分支调 policy.on_success()；Rust 回归锁 subscribed_resets_backoff_sequence_after_failures）**

- **[M-02] `bedcode-mobile/src/views/TerminalView.vue:312-316` — 倒计时从不递减** `sync()` 每秒重读静态 `buffer.reconnectInMs` 并赋值同一 `ceil(reconnectInMs/1000)`——显示秒数恒定（如永远显示「30s 后重连」）。注释自称「倒计时在组件本地递减」，实际无任何递减逻辑。
  > 建议：记录 `reconnect_scheduled` 到达时的 wall-clock 时间戳，`remaining = reconnectInMs − (now − receivedAt)`。
  > 状态：**done（2026-10-05 修复批次：store 记录 reconnectReceivedAt + utils/reconnectCountdown.ts 纯函数倒计时；9 条行为契约测试）**

- **[M-03] `bedcode-mobile/src/stores/terminalBuffer.ts:321-327` — `reconnecting`/`reconnectInMs` 只在 `live` 分支清理** 退避中途命中 `stopped`/`session_missing`（strikes 耗尽）/`unsubscribed` 时两个字段永不清除；Rust 侧链路放弃后不再发事件 → 横幅在死会话/已停止会话上永久卡「正在重连/30s 后重连」。
  > 建议：`stopped`/`session_missing`/`unsubscribed` 分支同样清 `reconnecting`/`reconnectInMs`。
  > 状态：**done（2026-10-05 修复批次：stopped/session_missing/unsubscribed 分支清除 reconnecting/reconnectInMs；4 条新测试）**

---

## 2. 批 3-4 —— W1 桌面页面过渡（宿主核心 + wasm 视图）

### Medium

- **[D-01] `bedcode-desktop/wasm-apps/terminal-session/src/components/DeviceCenterView.vue:115` — 过渡容器残留 `space-y-6`** 重叠式 `page` 过渡期间容器内是**两个兄弟节点**（出场层 + 入场层），`space-y-6` 的 `> * + * { margin-top }` 会把之后进来的入场层往下顶 24px，出场层移除瞬间回弹 → 每次切 Tab 布局跳一下。**全仓唯一**把 space-y 写在 page-swap 容器上的视图（其余迁移视图 AgentHubView/FileTransferView/ChatView 都是裸 page-swap）。
  > 建议：删掉该 `space-y-6`（各 tab 内容自己已有 space-y-*）。
  > 状态：**done（2026-10-05 修复批次：删除 page-swap 容器 space-y-6）**

### Low

- **[D-02] `bedcode-desktop/src/style.css:742-745` — `calc(-1 * var(--page-swap-pad-y))` 用 CSS Values-4 乘法** `*` 运算符 Chromium 111+/Safari 16.4+ 才支持；Tauri 双端渲染引擎随平台变（WebView2 / WKWebView / WebKitGTK），不支持的内核会在**解析期整条丢弃**（不是部分应用）→ `top/left/width` 静默消失，内边距补偿失效，出场层偏移。
  > 建议：改减法写法：`top: calc(0px - var(--page-swap-pad-y))`、`left: calc(0px - var(--page-swap-pad-x))`、`width: calc(100% - var(--page-swap-pad-x) - var(--page-swap-pad-x))`。
  > 状态：**done（2026-10-05 修复批次：calc 乘法改减法 top/left/width）**（低成本，建议随 W1 修复一起改）

- **[D-03] `bedcode-desktop/src/style.css:735` — `will-change: opacity, transform` + `.page-swap{position:relative}` 制造 containing block** 过渡期间（160-220ms）页面成为 `position: fixed` 后代的 containing block；Tooltip/Modal/LoadingOverlay 未用 `<Teleport>`、fixed 表面内联渲染 → 路由切换瞬间 fixed UI 临时重锚到页面元素而非视口，肉眼跳变。
  > 建议：审计路由页内 fixed/absolute 元素给独立定位容器；`will-change` 只在真需要的 transform 上用。
  > 状态：**done（2026-10-05 修复批次：审计全仓 fixed 表面，唯一真违规 TerminalPreview renderer-override 弹窗补 Teleport to body；其余已 Teleport）**

- **[D-04] `bedcode-desktop/src/components/DesktopLayout.vue:23` — 出场层 `z-index:1` 整个 160ms 内仍可交互** 重叠窗口期点击落在正在淡出的旧页上（快速切路由/双击）。
  > 建议：`.page-leave-active` 加 `pointer-events: none`，入场层立即接收事件。
  > 状态：**done（2026-10-05 修复批次：.page-leave-active 加 pointer-events:none，全端生效）**

- **[D-05] `bedcode-desktop/wasm-apps/ai-chatbox/src/{ChatView.vue:5, ProviderConfigPage.vue:35, ChatView.vue:2}` — 重叠模式副作用簇** ① 出场分支带 `position:absolute` + 自然高度（无 bottom/height）：长表单/长会话淡出层超出容器可视高（ProviderConfigPage 被宿主 `main{overflow:hidden}` 裁；ChatView 聊天分支无 h-full → 长会话画到视口下方、短会话淡成小条露出底下入场页）；② 出场层在顶上且可交互，快速双击 provider 行可重触发 `startEdit` → 淡出中 `formKey` 重生成、ProviderForm 重挂载。
  > 建议：给出场层补高度/裁剪约束；`.page-leave-active` 加 `pointer-events:none`（与 D-04 同修）。
  > 状态：**done（2026-10-05 修复批次：.page-leave-active 加 max-height:100% + overflow:hidden（长内容不外溢）+ pointer-events:none；快速点击场景已由 D-04 覆盖）**（需实测长内容+快速点击两个场景再定修法）

- **[D-06] `bedcode-desktop/src/utils/pageTransition.ts:40-42` — 硬编码中文 throw + 启动期整死** 未知 `PAGE_TRANSITION_EFFECT` 抛的是一段绕过 vue-i18n 的中文字符串；且 `applyPageTransitionEffect` 在 `app.mount` 前同步执行，拼写错 → 整个启动被拒无 UI 兜底（文档化的取舍，但可考虑 console.error + 回退默认值，测试路径仍保留 throw）。
  > 建议：错误文案至少 ASCII（i18n 在启动前不可用是合理理由，但值得写注释）；启动路径 catch 回落默认效果。
  > 状态：**done（2026-10-05 修复批次：throw 改 ASCII + main.ts 启动 catch 回落默认效果）**

### 🚫 误报（clone 范围，跳过）

- `StatsTrend.vue:22` 「axisScale/formatAxisTick 未导出」——真实工作树 `format.ts:203/372` 有导出（批 5 克隆的 format.ts 是 HEAD 旧版）。
- `OverviewTab.vue:64` 「EnvInfo 缺 arch/osVersion/shell/python」——`types.ts:30-36` 全有；i18n `messages.ts:27-36` + en/zh 全有。
- `AgentHubView.vue:129` 「宿主缺 .page-swap/绝对定位/动效 token」——`src/style.css:710-724` 全有。
- `OverviewTab.vue:64` 「hub.env.arch 等 key 缺失」——同上，i18n 三文件全有。
- `styles.css:1246` 「.ah-pv-apply 死 CSS」——ProviderApply.vue 模板全用（ah-pv-back/note/results/meta-item/body/actions/head 一一对应）。
- `styles.css:1319` 「--ah-pv-apply-measure 无效值」——声明在 `.ah-pv-apply`（渲染中），后代继承生效。

---

## 3. 批 5-7 —— W2 agent-hub 供应商应用（Rust + 前端，24 文件）

### Medium

- **[A-01] `agent-hub/src/utils/format.ts:373-376` — 兜底分支刻度顺序颠倒** `i === count ? 1 : 0` 产出升序 `[0,0,0,1]`，正常分支与注释契约「自上而下的刻度（含 top 与 0）」是降序 `[top,…,0]` → 全零/非法数据时 Y 轴上下颠倒；且 `count<=0` 进兜底时 `step = 1/count` 是 `Infinity`。
  > 建议：`i === 0 ? 1 : 0` + `count > 0 ? 1/count : 1` + `Math.max(count,1)`。
  > 状态：**done（2026-10-05 修复批次：兜底刻度降序 + count≤0 防除零；4 条新测试）**
- **[A-02] `agent-hub/src/utils/format.ts:203-207` — 亚千刻度被 `formatTokens` 取整抹平** `formatTokens` 对 `< 1000` 直接 `String(Math.round(v))`；`axisScale` 合法产出 `0.6/0.4/0.2/0` → 刻度渲染成 `1/0/0/0`，坐标轴信息全丢。
  > 建议：`formatAxisTick` 对 `< 1000` 保留小数（仅剥尾 `.0` 后缀）；`v == null` 改严格判空。
  > 状态：**done（2026-10-05 修复批次：formatAxisTick 亚千刻度保留小数；3 条新测试）**
- **[A-03] `agent-hub/src/composables/useProviders.ts:178-181` — `fetchModels` 只查数组/非空、不查条目类型** 有 `Array.isArray` + `length>0`，但条目若是非字符串/空白，下游 `mergeModelIds` 的 `id.trim()` 直接 TypeError（`addAllFetched` 崩溃）；未 trim 的 id 与已 trim 列表比对 → chip 激活态误判。与「unrecognized shape 必须 fail-visible」契约相悖。
  > 建议：回执校验逐条断言 trim 后非空字符串。
  > 状态：**done（2026-10-05 修复批次：fetchModels 逐条目 trim + 非字符串整体拒绝；composable 5 条新测试）**
- **[A-04] `agent-hub/src/components/ProviderApply.vue:206-209` — 部分目标失败不可就地重试** 多目标部分失败（部分 ok、某目标 writeFailed/invalidEnvKey）时 `applied` 置真 → `v-if="!applied"` 隐藏表单+动作条，并弹通用「应用失败」；但**成功目标已写入**，失败目标没有就地重试入口，只能关面板重开。部分成功文案与「全失败」未区分。
  > 建议：区分「全部失败」（通用 toast + 保留失败行）与「部分成功」（不弹失败 toast、保留表单，仅重试失败目标）。
  > 状态：**done（2026-10-05 修复批次：部分成功不弹失败 toast、目标收敛失败项、表单保留就地重试；i18n partial 键 + 2 条新测试）**
- **[A-05] `agent-hub/src/components/ProvidersTab.vue:245-246` — 改 URL/切预设后候选模型残留** `fetchedModels` 只在**下一次查询/开编辑器/关编辑器**时清空。成功查询后改 models URL 或换预设（A→B）→ 旧 A 的 chips 仍显示，`addAllFetched`/`toggleFetchedModel` 把 A 模型并进现 B 目标的列表。
  > 建议：`watch(formModelsUrl/formBaseUrl)` 清 `fetchedModels`/`modelsQueryError`，或候选随查询 URL 存栈、不匹配即失效。
  > 状态：**done（2026-10-05 修复批次：watch([formModelsUrl, formBaseUrl]) 清候选；2 条新测试）**
- **[A-06] `agent-hub/src/components/ProviderApply.vue:131-134` — codex 不在 model-less 预拦截里** `blockedTargets` 只拦 pi/opencode；guest 侧 `codex.rs::plan_apply` 同样拒 `noModels`，且 `codexNextModel` 为 null 时切换预览横幅也不出现 → codex 目标在查不到模型的预设上无任何点击前警告。
  > 建议：`modelLessTargets` 纳入 codex（或给它 noModels hint）。
  > 状态：**done（2026-10-05 修复批次：codex 纳入 modelLessTargets 预拦截；1 条新测试）**
- **[A-07] `agent-hub/src/composables/useProviders.ts:183-185` — `console.error` 打原始异常** 模型拉取失败时把 guest 原文 `e` 落日志；若 guest 错误回显了请求 URL，而 `queryModels` 允许 `https://user:key@host/...`（见 A-08），凭据可能进日志（违背 no-secrets-in-logs）。
  > 建议：只记状态/错误类别，不回显 guest 原文。
  > 状态：**open（security）**

### Low / Security-low

- **[A-08] `agent-hub/src/components/ProvidersTab.vue:241` — URL 校验只查 `^https?://` 前缀** 允许带 userinfo/query 凭据的 URL；失败路径 `console.error` 回显 guest 原文（可能含 URL）。建议失败日志只记状态/错误类型，不记 URL。**状态：done（2026-10-05 修复批次：userinfo URL 前端拦截 + console 只记状态类别；组件测试 162 全绿）**
- **[A-09] `agent-hub/src/composables/useProviders.ts:132` — `applyProvider` 整块缩进回归到列 0**（`/** ... */` + 函数体），文件内其他方法（`fetchModels`/`savePreset` 等）均为 2 空格。**状态：done（2026-10-05 修复批次：applyProvider 缩进已归位）**
- **[A-10] `agent-hub/src/components/ProvidersTab.vue:258-260` — `modelInList` 每 chip 每渲染重新解析整个 models 文本域** `:class` 与 `:aria-pressed` 各调一次，`fetchedModels` 几百个时 O(chips × lines)。**状态：done（2026-10-05 修复批次：`parsedModels` computed + `parsedModelSet`，O(1) 查激活态）**

### 🚫 误报（clone 范围，跳过）

- `useProviders.ts:132` 「applyProvider 签名 targets 数组与消费方不匹配」——真实工作树 ProviderApply.vue:170 已传 `targets.value`（数组），匹配新签名（批 5 克隆未拷 ProviderApply.vue 的新版）。
- `providers.ts:59` 「codex 进 APPLY_TARGETS 破坏单测」——真实工作树 `providers.test.ts:57-58` 已断言 4 目标列表。
- `providers.ts` 「TARGET_PATHS/deriveEnvKeyName 无消费方」——ProviderApply.vue:29 导入并使用（`:306/347`）。
- `providers.ts` 「parseModelsText/mergeModelIds/deriveEnvKeyName 无测试」——`providers.test.ts:7-11` 导入，`:82+` 有 `describe('defaultModelsUrl')` 等用例。

---

## 4. 批 8 —— W3 构建产物 target-dir

- **[B-01] `wasm-apps/.cargo/config.toml:18-19` — 孤儿 target 目录不在 size 脚本视野** 修复前的仓库根 `target/wasm-apps`/`target/fixtures` 残留不被 `check-target-size.js` 报告（`sharedTargetDirs` 解析到 `bedcode-desktop/target/...`，`rootTargetDirs` 只列 server-libs/cross-end-tests）。**已处理**：本批 CHANGELOG 已删孤儿目录（实测无 `target/wasm-apps`/`target/fixtures` 于仓库根；`cross-end-tests/target` 是后续运行重建）。**状态：done（随 W3 已处理，无需再改）**
- **[B-02] `wasm-apps/.cargo/config.toml:23-25` — `target-dir` 无自动化回归守卫** 唯一的防护是注释里的人力 `cargo metadata` 检查；`cargo test` 的成功与否与 target 落点无关 → 同款「多一个 `..`」回归 CI 全绿。**状态：done（2026-10-05 修复批次：test.yml 新增 `Verify wasm-apps target-dir` 步骤，cargo metadata + realpath 归一化断言；本机验证通过）**

---

## 5. 批 9 —— W5 TaskHistoryView（任务队列扩展）

### 🚫 全误报（clone 范围，跳过）

- `TaskHistoryView.vue:1145` 「mode='out-in' 被丢 + 宿主 `page` 过渡无绝对定位」——错。宿主 `src/style.css:714-724` 有 `.page-leave-active{position:absolute}`（重叠式设计即是本批 W1 的目标）。
- `TaskHistoryView.vue:1144` 「.page-swap 不存在」——错。`style.css:715` 定义 `.page-swap{position:relative; --page-swap-pad-*}`。
- `TaskHistoryView.vue:2369` 「--motion-page-*/data-page-fx/pageTransition.ts 不存在」——错。`style.css:702-705` + `utils/pageTransition.ts` 都在；且兄弟视图（SessionCenterView:110 / DeviceCenterView:116）均已迁 `name="page"`（全仓无残留 `.tab-fade` 用法，仅注释提及）。

（批 9 克隆只拷了 TaskHistoryView.vue，宿主 style.css 是 HEAD 旧版 → 三个「宿主契约缺失」全假。）

---

## 6. 批 10-11 —— W5b 内核拆分可审部分 + file-transfer

### Medium / 有效

- **[K-01] `wasm_core/host_api/ws.rs:1109-1110` — 测试拆出后壳模块残留死导入**（→ **done，新位置天然干净**：ticket-04 迁移后宿主 ws.rs 为 156 行 adapter、`mod tests;` 外部文件，无壳死导入） `mod tests` 只剩子模块声明，但保留的 `build_host_ctx`/`grant_permissions`/`WsConnBase`/`WsRegistration` 等命名导入已无引用（各子文件自带）。`unused_imports` 警告会出现在每次测试构建（若 crate deny warnings 则 CI 红）。
  > 建议：壳模块只留 `use super::*;`。
  > 状态：**open**
- **[K-02] `wasm_core/security/network_auth.rs:832` — 同 K-01**（→ **done，代码已改**：壳只留 `use super::*`；验证被 ticket-04 并发迁移的 ws_e2e 阻塞，待桌面编译恢复后 `cargo test --lib network_auth::tests`） 测试体迁走后保留的 `MetricsRegistry`/`AuthStrategy`/`AUTH_RECORDS_CAP`/`Path`/`AtomicUsize`/`Ordering` 已无引用。
  > 建议：同上只留 `use super::*;`。
  > 状态：**open**
- **[K-03] `wasm_core/security/network_auth/tests/policy_tiers.rs:63` — 文档注释含乱码字符 ``**（拆分时编码残留，`落一条没人读记录`）。**状态：done（已修乱码：落一条没人读的记录 =；代码已改，验证同 K-02 待桌面恢复）**
- **[K-04] `wasm_core/host_api/ws/tests/isolation.rs:7-8` — `WsConnBase`/`WsRegistration` 死导入**（→ **done，新位置天然干净**：迁入后 isolation.rs 只 `use super::scaffold::*; use super::*;`）（仅 import 行出现，正文不用；`use super::*` 已转供）。**状态：done**
- **[K-05] `wasm_core/security/network_auth/tests/general.rs:6-7` — `AuthStrategy`/`Path` 死导入**（→ **done，代码已改**；验证同 K-02 待桌面恢复）（C6 用例只用 `normalize_target`）。**状态：done**
- **[K-06] `wasm_core/host_api/ws/tests/was_clean.rs:3` — `use super::scaffold::*` 未用到**（→ **done，新位置已删**：`bedcode-server-websocket/.../tests/was_clean.rs` 只留 `use super::*`；25/47 测试全绿）（唯一符号 `close_was_clean` 走 `super::*`）。**状态：done**
- **[K-07] `wasm_core/host_api/ws/tests/scaffold.rs:1` — 名为 scaffold 的共享基础设施文件里混了 4 个真 `#[actix_rt::test]`**（→ **done，新位置已拆**：`plugin_binding/tests/connection_context.rs` 独立成组，scaffold.rs 只剩共享助手；25/47 全绿）（`connection_context_*`），会混淆 `scripts/audit-rust-tests.mjs` 类工具；建议单独 `connection_context.rs`。**状态：done**
- **[K-08] `wasm_core/host_api/ws/tests/scaffold.rs:258-260` — 注释与代码自相矛盾**（→ **done，新位置已补真负例**：`connection_context_rejects_cross_owner_and_unknown` 增加「另一端点的客户端查询 → not found in endpoint」负例 + 正例收尾；25/47 全绿） 注释说「不属于该端点的客户端 → 同错」，代码实为**正例查询**并断言 `"authenticated":true`（负例意图丢失，且 `.contains()` 对序列化 JSON 可假过）。**状态：done**
- **[K-09] `wasm_core/security/network_auth/tests/scaffold.rs:171` — C6 正例落进 scaffold**（→ **done，代码已改**：`normalize_target_makes_equivalent_urls_one_target` 移入 general.rs；验证同 K-02 待桌面恢复） 其余 C6 在 general.rs，此处按「scaffold 只放跨组助手」的分组意图属错位。**状态：done**

### Medium / file-transfer

- **[F-01] `file-transfer/src/styles.css:1220-1221` — tab 条 `inline-flex + flex:0 0 auto + nowrap` 长文案溢出** `.ft-tabs` 有 `max-width:100%` 但 tab 不可收缩，本地化长标签时会溢出被 `.ft-queue{overflow:hidden}` 裁掉。建议 `flex-wrap:wrap` 或显式 `overflow-x:auto`。**状态：done（2026-10-05 修复批次：`.ft-tabs` 加 `overflow-x:auto` 保留分段语义，长文案可水平滚动）**

### 🚫 误报（clone 范围，跳过）

- `FileTransferView.vue:478` 「宿主 page/.page-swap/绝对定位不存在」——批 11 克隆只拷了 file-transfer 两文件，宿主 style.css 是 HEAD 旧版；工作树 `src/style.css:710-724` 全有。评注里「保留 mode='out-in' 兜底」与 W1 设计意图相反，跳过。
- `file-transfer/src/styles.css:985` 「注释所说 --motion-page-* token 不存在」——同上，误报。
- `wasm_core/bus.rs:231` 「注释指向 tests/hot_path_logging_lock.rs 不存在」——工作树确有该重命名（`src/hot_path_logging_test.rs → tests/hot_path_logging_lock.rs`，批 10 克隆未拷重命名）。误报。
- `bedcode-mobile/packages/plugin-sdk-mobile/rust/src/types.rs:253` 「测试已迁 `../tests/` 但目录不存在」——工作树 `rust/tests/{pluginmanifest,lifecyclecontribution,plugintype_pluginstate,contributions}.rs` 全在（含 `rust_ts` 拒绝、`is_declared` 映射等断言一一对应）。误报（批 10 克隆未拷未跟踪 tests/ 目录）。

---

## 7. 修复优先级建议（供排期）

**P0（W4 重连三剑客，同一特性、互相咬合，且 CHANGELOG 口径已验证——先修）：**

- M-01 backoff 不重置 · M-02 倒计时不递减 · M-03 reconnecting 不清零

**P1（高/中危高影响，纯 bug）：**

- A-01 刻度升序+Infinity · A-02 亚千刻度抹平 · A-03 fetchModels 条目校验 · A-04 部分失败不可重试 · A-05 候选残留

**P2（UI/兼容/安全低危，低成本）：**

- D-01 space-y-6 回弹 · D-02 calc 乘法改减法 · D-04/D-05 pointer-events + 高度约束 · D-06 启动兜底 · A-06~A-10 · B-02 CI 守卫

**P3（内核拆分测试壳整理，属 `.scratch/2026-10-04-wasm-core-lib-split/` 票 04 范畴）：**

- K-01~K-09 死导入/注释矛盾/分组错位 · F-01

---

## 8. 审核过程元数据

- 命令形态：`git clone --no-hardlinks . /tmp/ocr-<ws>` → 拷入该批文件的**工作树版本** → `ocr review --audience agent --concurrency 1~3 --background-file /tmp/ocr_bg_*.md --output /tmp/ocr_g*.txt`
- 每批独立 session（`ocr session list` 可查；本日批 1 `bc9ef6fb`（aborted）→ 拆分后 `e041d32f`、`cd3a6417`、`8a12af93`、`0a45a80d`（前日 legacy）… 最后一次 `57cfad9b` 等）；输出文件在本机 /tmp（重启清空，重要条目已登记上文）
- **限流代价**：sensenova 429 rpm 严重，拆批 + `--concurrency 1` + `--no-filter` + 批间 sleep 30-120s 才跑通；同一批重跑 2-4 次是常态，token 消耗 ~2-7M/批
- **方法学教训（登记为可复用规则）**：跨工作流拆批必须把被审工作流的**互依文件**（types/i18n/宿主 CSS/被引用组件）按同版本拷入克隆，否则 OCR 的「缺失」类评论全部是真源码里不存在的误报；宁可一批多拷几个文件，也不要为省拷贝造成 18/31 条误报
- 修复时建议：每工作流修完跑对应测试（§3 两段式）；P3 与内核拆分票 04 合并处理；禁止夹带无关改动（§11）

**未登记**：批 11 之外的 40+ 文件（terminal-session rust 目录化、wasm_core manager 全量、bedcode-host-kit、split-rust-tests 脚本）属其他在途任务执行形态，未审（§11 不触碰在途改动）。