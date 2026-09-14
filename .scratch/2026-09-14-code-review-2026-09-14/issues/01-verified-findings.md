# 01 — dev 分支代码审核复核：已确认结论清单（2026-09-14）

**Type:** review
**Status:** fixed（#1-#6、#8-#9 已修；#2 待 commit；#7 设计差异不修；terminal_ws 待裁决）

固定点 `origin/master`（merge-base 6ae51e6b），范围 = 26 个 commit + 未提交工作区改动。原审核由 10 个 lane（7 Standards + 3 Spec，reviewer × sensenova-6.8-flash-lite）产出；本文件是对全部报告的**逐条代码阅读 + 实测验证**后的最终确认清单。

**复核方法**：对每条报告，读真实代码 + 用 bash / marked 实测行为，区分：✅ 真 bug（按严重度分级）、⚠️ 属实但低危/纪律违反、❌ 误判（非 bug）。

---

## ✅ 真 bug（5 条）

### 1. agent-hub 两个 shell 注入面（RCE）—— 高危，合并阻塞

**位置**：`bedcode-desktop/plugins/agent-hub/rust/src/usage.rs:270` 与 `skills.rs:438`（两处 `scan_script` 同款实现）、`lib.rs:88` 执行点。

**链路**：
1. 用户自定义来源路径经 `add_source` 写入 state —— 该校验只有 `path.starts_with('/')`（usage.rs:884）+ name 合法字符（`is_valid_source_name`）。`/tmp/x$(touch /tmp/pwn)` 以 `/` 开头，**通过校验**。
2. `usage::scan` 把该 path 拼进 `scan_script` unix 分支：`find "{root}" -type f -name '*.jsonl'` —— **双引号内 `$()` 命令替换会执行**。
3. `shell_invocation`（lib.rs:88）用 `/bin/bash -lc` 执行整个脚本。
4. `skills::import_local`（skills.rs:1029）路径来自前端参数或 `platform_pick_folder`，只校验 basename 不含 `..`，绝对路径本身直接进 `start_import` → 同一 `scan_script` 注入。

**实测证据**：`bash -lc 'find "/tmp/x$(touch /tmp/PWN_TEST)" -type f 2>/dev/null'; ls /tmp/PWN_TEST` → 成功创建 `/tmp/PWN_TEST`。**任意命令执行确认**。

**修法**：POSIX 单引号包裹 + `'\''` 转义（或 `find -- "$root"`）；Windows 分支转义 `%`/`&` 等元字符；`add_source`/`import_local` 校验收紧（拒绝 shell 元字符或走路径白名单）。

### 2. agent-hub `plugin.json` 漏声明 3 条 usage-source 命令 —— 已修复但未提交

**位置**：`bedcode-desktop/plugins/agent-hub/plugin.json` contributes.commands。

**现状核对**：
- HEAD（5310c43e5）声明 29 条、**0 条 usage-source**；`lib.rs:233-235` 已分派 `list-usage-sources`/`add-usage-source`/`remove-usage-source`；`useUsage.ts:50,60,71` 已调用。
- 生产宿主白名单仲裁下，日志来源增删列表整块不可用（静默失败）。
- **工作区未提交 diff 已补回 3 条**（现 34 声明 = 34 分派，完全对齐），但尚未 commit。

**行动**：随 agent-hub 收尾提交一起合入即可；合入前确认 diff 覆盖。

### 3. 移动端 HTTP 历史回退键名不匹配（静默失效）—— 高危，合并阻塞

**位置**：`bedcode-desktop/src-tauri/src/server/dtos/session_dto.rs:83` vs `bedcode-mobile/src-tauri/src/terminal_link.rs:1111-1114`。

**链路**：
- 桌面 `SessionHistoryData` 有 `#[serde(rename_all = "camelCase")]` → 线上 JSON 键为 `minOffset` / `snapshotOffset` / `historyBytes` / `dataBase64`。
- 移动端 HTTP 回退路径读 `data.get("min_offset")` / `"snapshot_offset"` / `"history_bytes"` / `"data_base64"` —— **四键全 miss**。
- 落到 `unwrap_or`：min_offset→from、snapshot_offset→from、history_bytes→0、data_base64→""。16MB LRU 淘汰后拼不出历史，且 `min_offset` 未抬升 → 截断信号丢失（前端误以为无截断、不触发清屏重播）。
- 讽刺点：**缓存命中路径（terminal_link.rs:1063-1067）键名是 camelCase（正确），回退路径是 snake_case（错误）**——同一函数内前后矛盾，注释还写着「转 camelCase」。

**WS 路径不受影响**（`subscribe_ok` 走 snake_case 结构体字段）。

**修法**：回退路径改读 camelCase 键（或对 `data` 做 camelCase 归一化后统一读）。

### 4. SDK MarkdownEditor raw HTML 透传 XSS —— 中危

**位置**：`bedcode-desktop/packages/plugin-sdk-desktop/src/ui/MarkdownEditor.vue:262`。

**链路**：
- `const previewHtml = computed(() => marked.parse(draft.value) as string)` 无 options，renderer 只覆盖 `link()`。
- **实测**：`marked.parse('<img src=x onerror="alert(1)">')` 原样输出 `<img src=x onerror="alert(1)">`——marked 默认透传 raw HTML。
- 渲染出口 `v-html="previewHtml"`（:109）→ 浏览器执行 onerror 回调。
- 组件经 SDK 导出（`ui/markdown-editor`），agent-hub `SkillEditor.vue:201` 用于编辑/预览 **skill 内容**——内容可来自 GitHub 安装/本地导入（不可信输入）→ 任意插件 XSS。

**修法**：`marked.parse(draft.value, { html: false })`（或 `marked.use` 里 `html: false`），需要白名单 HTML 时用 DOMPurify 净化。

### 9. divider 插入在特定场景变 setext H2 —— 低危（触发面窄）

**位置**：`bedcode-desktop/packages/plugin-sdk-desktop/src/ui/markdown-editor-syntax.ts:159-181`（`applyDivider`）。

**链路**：`applyDivider` 分支 3（光标在文末空行，`end === text.length`）：`text.slice(0, start) + '---\n'`。当文本以「段落 + 末尾空行」结尾时，`text.slice(0, start)` 只含段落文本 + 一个换行（不含空行自身的换行）→ 拼出 `"hello\n---\n"`。

**实测**：`marked.parse('hello\n---\n')` → `<h2>hello</h2>`（setext H2）。与函数注释「保证 `---` 前有空行，避免被解析为 setext 标题」**完全相反**。分支 1/2 正确，仅分支 3 触发。

**修法**：分支 3 改为 `text.slice(0, start) + '\n\n---\n'` 或 `'---\n'` 前保证前置空行。

---

## ⚠️ 属实但中低危 / 纪律违反（4 条）

### 5. 双端 WIT 各增 3 个 peer 方法但 ABI_VERSION 未 bump —— 纪律违反（非功能性）

**位置**：`bedcode-desktop/packages/plugin-sdk-desktop/rust/wit/bedcode.wit` + `abi.rs:29`（ABI_VERSION=10）；`bedcode-mobile/packages/plugin-sdk-mobile/rust/wit/bedcode.wit` + `abi.rs:18`（ABI_VERSION=8）。

**现状核对**：双端 WIT 均新增 `pause-transfer` / `resume-transfer` / `resume-all-transfers` 3 个 host-peer 方法（git diff master...HEAD 确认新增行），**ABI_VERSION 两端均未变**。

**先例不一致**：abi.rs 演进史 v9→v10（桌面）/ v7→v8（移动）正是「新增 3 个 host-peer 原语（dial-peer-endpoint/close/set-shared-roots）+ 2 个新接口」就 bump——注明「纯增量变更，v8 插件二进制不受影响」仍 bump。本次同样新增 host-peer 函数却未 bump，违反 §7「改 WIT 必须双端同步 + ABI bump」。

**影响**：新插件调用新函数在旧宿主上**运行中 trap**（wasmtime 无该导出）而非加载拒绝；host 加载校验 `version <= ABI_VERSION` 拒绝高版本插件。属兼容性纪律问题，非功能缺陷。

**行动**：双端同步 bump（桌面 10→11、移动 8→9），与 wasm-core 05 core-bus 的 bump 先例对齐。

### 6. 5 处长寿命 spawn 未包 spawn_with_error_boundary —— 低危规范违反

**位置**：
- 桌面 `bedcode-desktop/src-tauri/src/commands/terminal_stream.rs:83,96`（forward_loop + 消费任务）
- 移动 `bedcode-mobile/src-tauri/src/terminal_link.rs:572,590`（link_io + 推送任务）
- 移动 `bedcode-mobile/src-tauri/src/peer_transfer.rs:512`（传输任务）

**现状核对**：双端 `system/error_boundary.rs:4` 注释明确「**所有重要的后台任务都应使用 spawn_with_error_boundary 启动**」；peer_net.rs（536/895/515/981 等）已合规。上述 5 处是裸 `tokio::spawn` / `tauri::async_runtime::spawn`。

**影响**：任务 panic 静默无日志（JoinHandle 不 await 则 Err 丢失），调试盲区。低危但违反自家规范。

### 7. file-transfer 双端「发送结算通知」不一致 —— 数字不准，更可能是设计差异（非 bug）

**现状核对**：
- 移动端 `useTasks.ts:150-228` 有完整 settle 通知：历史 diff 驱动累计 completed/failed/cancelled，tasks 清空时 `notifications.notify`（failed>0 弹失败、completed>0 弹完成、全取消跳过）；i18n `transfer.notify.*` 4 个 key。
- 桌面端 `useTasks.ts` **没有** settle 通知，只有 console.error；自绘 toast（useReceiving）是「接收中提醒」非失败通知。
- **报告数字不精确**：「桌面 0/5、移动 4/5（resume-all 漏）」与代码对不上——移动端单项操作失败（cancel/retry/pause/resume/resume-all）也都是 console.error 非 toast；移动端 settle 是 2 分支通知非 5 个 toast。

**判定**：spec（`.scratch/lan-file-transfer-plugin/spec-zero-transfer.md` 需求 5/6）通知需求**全在移动端**（手机锁屏/后台场景），桌面端场景不要求系统通知 → **双端差异是 spec 使然**。唯一可挑剔：桌面端发送失败无任何用户可见提示（仅队列面板状态）——UX 权衡，非缺陷。

### 8. http_proxy 未设 .redirect() —— 子 agent 降级依据正确，但残留 SSRF 面（中危）

**位置**：`bedcode-mobile/src-tauri/src/commands/http_proxy.rs:105-112`（client()）；`bedcode-desktop/src-tauri/src/plugin/wasm_runtime/host_impl/http.rs` 4 个 Client::builder（均无 .redirect()）。

**现状核对**：
- reqwest 0.12.28 默认 `Policy::limited(10)` 跟随最多 10 跳，**跨 host/port 也跟随**。
- egress 校验（L1/L2/L3）只在请求发出前对**原始 URL** 校验一次，redirect 链上的后续目标**不再过 egress**。
- **子 agent 降级依据实测正确**：`~/.cargo/registry/.../reqwest-0.12.28/src/redirect.rs` `remove_sensitive_headers` 在 host/port 变化时移除 `AUTHORIZATION` / `COOKIE` / `PROXY_AUTHORIZATION` / `WWW_AUTHENTICATE` → 跨域跳转不会带上 JWT/凭据，「已授权 host 转发请求给未授权 host 并带凭据」**不成立**。
- **但残留 SSRF 被低估**：redirect 目标不重过 egress。无系统代理的桌面环境（多数 Linux 桌面无全局代理）下，已授权外网 API 可 302 到 `http://127.0.0.1:port` / `http://169.254.169.254/`（云元数据）等内网地址并直连成功（桌面 HTTP_CLIENT 无 no_proxy，走系统代理；有代理时私网跳转被代理劫持到本地端口反而失败——`is_private_target` 的直连判定只对原始 URL 生效，跳转目标复用同一 client 不做私网判定）。

**修法**：`.redirect(Policy::none())` 禁用跟随（插件需要跳转时自行处理并重过 egress），或跟随后对最终 URL 重做 egress 校验。

---

## ❌ 误判（非 bug，6 条，均已纠正）

| 条目 | 报告 | 复核结果 |
|------|------|---------|
| `settings_store.rs` CRLF→LF | 被引 §9 定「硬性违规 高」 | 误判。§9 只管锁文件/产物不涉行尾。**但复核发现事实小错**：桌面端 `plugins/file-transfer/rust/src/settings_store.rs` 实测**仍是 CRLF**（非「HEAD 已是 LF」），移动端是 LF——同目录行尾分裂 hygiene 问题，非 §9 违规 |
| `ProviderApply.vue:122` `codex · v1 ✕` | 定「硬编码中文」 | 误判。串中无中文字符，`✕` 是符号；属未走 i18n（可本地化性），非 §6 中文字符串禁令 |
| `providers.ts:30` 通义千问 | 引作真中文例 | 当前文件已无此串（或记错路径），无需处理 |
| `TerminalPreview.vue:462,468` `min_offset=${...}` | 定 §8 硬性违规 | 误判。min_offset 不在 §8 枚举字段（session_id/device_id/plugin_id/request_id/batch_id/node_id）；是 console 日志字符串，符合精神非字面 |
| `session_output.rs:1009,1015` | 报违规 | 出界：pre-existing，不在 diff 内 |
| `ScanPanel.vue:179/238`、`useHttpApi.ts:597` | 定「高」 | 事实对但 severity 过重：均为注释/i18n/观测性，无功能影响 |
| `std-agent-hub-rust` manifest 26 条与 invoke 匹配 | 报无问题 | 错：实际 32 条（HEAD），恰漏 usage-source 3 条（与 #2 互相印证） |

---

## 交叉确认：terminal_ws.rs 死代码

`bedcode-desktop/src-tauri/src/server/ws/terminal_ws/`（control_frame.rs / forward.rs 等）约 258 行 bound_session=None 分支的旧路由 handler（handle_terminal/handle_subscribe/handle_unsubscribe/handle_session_control/handle_session_mode）仅 event 通道理论可达、实际无客户端发送。与 scratchpad 2026-09-13 独立记录互证；spec-pty lane 同时报「桌面 WS 环回删除是 scope creep（spec §0.1 不允许动桌面端）」。

**处置选项**：① 记为偏差 D-A7（删除环回是 dev 侧有意为之，spec 侧记录偏差）；② 恢复环回。需用户裁决后落地。

---

## 行动优先级（合入 master 前）

1. **#1 RCE**（高危）→ 修
2. **#3 历史回退键名**（静默数据丢失）→ 修
3. **#4 XSS** → 修（html:false 或 DOMPurify）
4. **#2 manifest 补声明** → 随 agent-hub 收尾提交合入（工作区已补，确认 commit）
5. **#5 ABI bump** → 双端同步 bump
6. **#6 裸 spawn 包装** → 5 处补 spawn_with_error_boundary
7. **#8 redirect** → 禁用跟随或二次校验
8. **#9 divider** → 分支 3 补前置空行
9. **#7** → 记录为设计差异（spec 使然），不修
10. **terminal_ws 死代码** → 等用户裁决 D-A7 或恢复

## Comments

- 2026-09-14：复核完成，9 条主要报告 + 7 条子 agent 报告全部逐条实测验证。RCE、历史键名、XSS 三条为真 bug 且应阻塞合并。

## 修复记录（2026-09-14，pi agent）

| 条目 | 落地 | 备注 |
|------|------|------|
| #1 RCE | ✅ 修 | `lib.rs` 新增 `sh_quote`（POSIX 单引号包裹 + `'\''` 转义）与 `path_rejected_for_script`（控制字符/`"`/`%` 双平台统一拒绝）；`usage.rs`/`skills.rs` 两处 `scan_script` unix 分支改单引号转义；`add_source`/`import_local` 入口拒绝；新增注入防护单测（恶意 `$()`/分号/单引号路径逐字节比对）。**与建议的差异**：Windows 分支保留双引号包裹（cmd 引号内 `&|<>` 按字面处理），`%` 无法在 cmd 命令行上下文转义（`%%` 折叠是 batch 语义），故在入口直接拒绝，注释说明理由 |
| #2 manifest | ⏳ 待 commit | 工作区 diff 已含 3 条 usage-source 声明（与 HEAD 相比），随 agent-hub 收尾提交 |
| #3 历史键名 | ✅ 修 | `terminal_link.rs` 回退路径改读 `minOffset`/`snapshotOffset`/`historyBytes`/`dataBase64`，注释记录根因 |
| #4 XSS | ✅ 修 | **与建议的差异**：marked 18.0.11 已无 `html:false` 选项（查了 `marked.d.ts` 与 esm 源码），安全关闭点改为 `marked.use` 覆盖 `renderer.html` 整体实体转义（块级+内联），零新依赖；组件头注释同步更新；新增 XSS 回归测试（`<img onerror>` 不进 v-html） |
| #5 ABI bump | ✅ 修 | 桌面 10→11、移动 8→9；两端 abi.rs 补 v10/v8（56ee094cb v3 终态收缩）与 v11/v9（传输控制三原语）演进注释；移动 wasm.rs 陈旧「v6」注释改「v9」；测试断言同步 |
| #6 裸 spawn | ✅ 修 | 5 处全包 `spawn_with_error_boundary`：桌面 terminal_stream.rs ×2（forward + 消费）、移动 terminal_link.rs ×2（link_io + 关闭信号）、移动 peer_transfer.rs ×1（pump_after_settle）；terminal_link.handle 字段类型改为 `tokio::task::JoinHandle<()>`（spawn 包装层返回值；tauri async_runtime 底层同为 tokio，字段仅存储未 abort，注释说明） |
| #8 redirect | ✅ 修 | 重校验方案（非禁用跟随）：移动 `egress.rs` 新增 `redirect_decision` 纯函数 + `redirect_policy()`（`Policy::custom` 同步裁决）——私有目标仅放行已声明桌面端目标/同源/全私网链，其余 Stop；接入 `http_proxy.rs` 共享 client 与 `wasm_host.rs` 两处插件 client（check_egress 只校验首跳的同一旁路面）；桌面 `host_impl/http.rs` 4 个 client 统一接入本端 `redirect_policy()`（公网→私网阻断，私网→私网放行）；双端各加裁决单测（含 169.254.169.254 云元数据用例） |
| #9 divider | ✅ 修 | 分支 3 仅当上一行非空时补前置空行（`hello\n` → `hello\n\n---\n`），上一行已是空行不叠空行；新增 setext 回归测试（含 `a\n\n` 不叠空行与空文本用例） |
| #7 | 记录 | 设计差异（spec 通知需求全在移动端），不修，见上文 |
| terminal_ws 死代码 | 待裁决 | D-A7 记偏差或恢复环回，需用户定 |

**验证**：agent-hub crate 96 test 全绿；其余（桌面 cargo test / 移动 cargo test / SDK vitest / eslint）见执行日志。