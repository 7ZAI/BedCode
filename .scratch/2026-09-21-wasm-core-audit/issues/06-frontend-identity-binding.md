# 06: 前端插件通道身份绑定（P0-5）

**What to build:** 前端插件代码无法再以他人身份调用宿主命令面——`plugin_*` Tauri 命令的调用方身份由宿主绑定，而非前端自报参数。修完后「Rust 端最终仲裁」对**前端通道**同样成立（目前只对 guest/WASM 通道成立）。

**Blocked by:** 无（但「是否实现 isolated sandbox」含产品裁决，见需裁决项）

**Status:** done（2026-09-22 实施完成，见「实施记录」；真机手工回归与 SDK 发布 bump 为登记项）

## 现状（已复核）

- `manager/api_bridge.rs:130-147`（`plugin_storage_get/set` 等）门禁 = `is_activated(参数里的 plugin_id)` + `permission.check(参数里的 plugin_id, ...)` → 查的是**被冒名者**的状态与权限；同 webview 内任一插件前端可直接 `invoke('plugin_storage_get', {pluginId:'受害者', key})`；
- `api_bridge.rs:254` 注释「前端无法伪造 plugin_id」不成立；
- `sandbox: 'isolated'` 只存在于类型定义（`src/plugin/types.ts:26`），`src/plugin/loader.ts:51` 把所有非 inline 直接跳过 → 该字段是虚假承诺，且**所有插件前端代码与宿主同权共生**（无 iframe/worker 隔离）。

## 需裁决项

1. 身份绑定方式：(A) 激活时宿主为每个插件前端签发一次性 channel token，`plugin_*` 命令按 token 反查 plugin_id（推荐，改动集中在 api_bridge + `PluginContext` 构造）；(B) 前端所有插件 invoke 经 `src/plugin/context.ts` 收口并由宿主校验「调用来源脚本 URL ∈ 该插件目录」（webview 内可被绕过，需先验证）；
2. `sandbox` 字段：真正实现 `isolated`（iframe + postMessage 桥，工作量另一票）还是**删除该字段**并明确「前端不做隔离，安全边界只在 Rust 端 + WASM 端」（诚实且省事，但要确认产品接受）；
3. 前端面是否需要与票 03 的审批联动（前端 API 面 = `contributes` + `PERMISSION_API_MAP`，高危 UI 位是否单独确认）。

## 验收

- [x] 契约测试：以他人 plugin_id 调用 `plugin_storage_get` / `plugin_invoke` / 其余 `api_bridge` 命令 → 拒绝（先红后绿）
- [x] `api_bridge.rs` 全部命令按裁决项 1 的机制改造，注释与实际一致（`api_bridge.rs:254` 的错误论断必须删除或改为真约束）
- [x] `sandbox` 字段按裁决项 2 处置（实现或删）——禁止保留未实现的安全承诺字段
- [x] 内置插件前端功能零回归（会话/终端/文件传输/AI 四个插件的 storage、invoke、事件订阅路径手测或测例覆盖）
      —— 测例面：前端全量 78 files / 748 tests 全绿（含四个插件前端套件）+ loader 门禁套件断言加载与令牌获取路径；
      真机手工回归（GUI 启动四插件）**未执行**（本环境无图形会话），已登记为残留验证项
- [x] i18n：新增的用户可见错误文案同步 zh-CN 与 en —— 本票未新增用户可见文案（失败路径沿既有错误提示，凭证错误是技术性错误串，走 logger/透传）
- [x] 门禁：`pnpm run test:run`（`--pool=forks`）+ `pnpm exec eslint .` 0 error + `cargo test`（改了 Rust）

## 实施记录（2026-09-22）

1. **为什么是两级凭证而不是一枚 token**：`签发 token(plugin_id)` 若本身可被任意前端代码调用，
   攻击者直接为受害者 id 签一枚即可冒充 —— 单一 token 等于没做。因此引入
   **loader 会话密钥**作为「谁能取令牌」的闸门：每次页面加载**首个调用者生效**，
   宿主前端在 `pluginLoader.loadAll()` 第一行（导入任何插件模块之前）取得并缓存于模块作用域；
   插件代码只在模块被导入后才开始运行，取不到。页面加载由 Tauri 插件钩子 `on_page_load` 重置
   （`lib.rs::frontend_channel_session_hook`），使 dev 下刷新后宿主仍能重新取得；
   插件令牌 `plugin_channel_token(plugin_id, loader_session)` 用该密钥换取、**插件运行态才签发、
   停用即回收**。
2. **裁决规则 fail-closed**：`FrontendChannelRegistry::authorize(target, credential)` ——
   凭证解析为 loader 密钥 → 宿主面（可操作任意目标，宿主配置页读写插件存储 / 宿主 `pluginInvoke`
   驱动插件命令都是宿主职权）；解析为插件令牌 → 目标必须等于令牌身份（不符即拒，错误点名两侧 id）；
   解析不出 → 拒绝，**不落回「按参数 plugin_id 放行」**。`plugin_id` 自此只是**目标**。
3. **顺带关闭的同族漏洞**：`plugin_fs_auth_respond` 此前无身份约束，而 fs 授权请求事件是**广播**的
   （插件前端也能 `listen`）→ 插件可替用户「同意」自己的文件访问请求，把授权弹窗变成摆设。
   现要求宿主面凭证（弹窗 `FsAuthDialog.vue` 持宿主密钥，插件拿不到）。
   另外 `plugin_invoke` 此前**连权限门都没有**（只查目标是否激活），现已同样绑定凭证。
4. **`sandbox` 退役**（裁决 2）：字段从桌面 SDK `PluginManifest`/`PluginInfo`、两份前端 TS 类型、
   四个随包 `plugin.json`、`loader.ts` 的跳过分支与 `loader.rs` 的 inline 校验全部删除。
   兼容口径：旧产物仍带 `"sandbox"` 键时按「老端忽略未知字段」照常加载，且不再被序列化回写
   （两侧各有一条兼容用例）。**SDK 公开字段退役**：下次发布 SDK 需按 0.x 记 breaking，本票不动版本号。
5. **变更面**：宿主新增 `plugin/security/frontend_channel.rs`（注册表 + 裁决 + 7 条单测）、
   `PluginHost` 字段/访问器/`is_running`/停用回收；`api_bridge.rs` 增两条凭证命令 + 五条插件面命令
   加 `credential`；`lib.rs` 注册命令与页加载钩子；前端 `commands.ts`（凭证缓存 + 令牌 +
   五个函数带凭证 + fs 应答封装）、`context.ts`（`createPluginContext` 异步取令牌并在闭包内持有）、
   `loader.ts`（bootstrap 取宿主凭证 + 异步 context + 摘 sandbox）、四处宿主调用点补凭证
   （`stores/session.ts`、`PluginConfigView.vue`、`PluginCommandPalette.vue`、`FsAuthDialog.vue`）。
6. **门禁实跑（2026-09-22）**：
   - `cargo test --lib`（bedcode-desktop/src-tauri）：**1132 passed / 2 failed**，两条均为既有
     时序抖动（`terminal_output_perf::perf_p2_guest_ring_fetch_batch_curve`、
     `task_e2e::test_task_submit_events_dispatched_and_status`；当时前端全量测试并发跑，机器负载高），
     **单跑均绿**（0.95s）⇒ 非本票缺陷；
   - SDK `packages/plugin-sdk-desktop/rust`：`cargo test --lib` **100 passed / 0 failed**（含新增退役字段兼容用例）；
   - 前端 `vitest run`：**78 files / 748 tests 全绿**（新增通道身份契约 4 条；受影响用例已按
     「凭证属断言外」口径修好，如 session store 的 `plugin_invoke` 参数断言补 `credential`）；
   - 根目录 `eslint .`：**0 error**（120 warnings，既有）；`vue-tsc --noEmit`：仅既有
     `src/dev/terminal-mock/TerminalMock.vue` 3 条（对侧终端下沉遗留，非本票文件）。
7. **变异自检**：`frontend_channel` 用例覆盖「跨插件身份拒绝（点名两侧）」「空/伪造凭证拒绝」
   「重新签发作废旧令牌」「停用回收」「页面加载重置后宿主密钥与插件令牌都失效」；
   前端套件断言每条插件面调用的 `credential` 必达（去掉凭证传递即红）；
   manifest 侧两条兼容用例同时锁「旧产物可加载」与「退役字段不再被序列化回写」。
8. **未做 / 登记项（不静默）**：
   - **真机手工回归未执行**：GUI 启动四内置插件、验证 storage/invoke/事件订阅路径需图形会话，
     本环境只有无头测试面；
   - **宿主管理面命令保持不绑定**（activate / install / uninstall / approve / dev-reload，裁决 3 范围外）：
     它们不是插件 API 面，仅宿主设置页调用；一旦未来有插件可达路径需另行评估；
   - **插件前端的 Tauri 事件订阅面**无法按调用者绑定身份（Tauri 事件无 per-监听者归属），
     本票不治，登记为已知边界；
   - **移动端同族问题未修**：移动端有自己的前端插件通道（`bedcode-mobile/src/plugin/**`），
     本票桌面-only；移动端要跟演需按其 SDK 形态另行评估（AGENTS §7 双端偏离惯例）；
   - **SDK 发布 bump**：桌面 SDK 一个公开字段退役，下次发版按 0.x breaking 记（本票不动版本号）。

## Comments

- 2026-09-21 立项：来源 spec §4-P0-5 与 §2「前端命令面 plugin_id 自报」。

### 裁决（2026-09-22 用户裁决，开工前已定，实施时不得再自行取舍）

1. **身份绑定方式：选项 A（宿主签发 channel token）**，B 否决。
   B 的三条否决依据：① Tauri 命令面**拿不到调用者脚本 URL**（同一 webview 内所有插件代码同权，`#[tauri::command]` 只给声明参数 + `State`/`Window`，无 per-模块标识）；② 即便可行，插件仍可直接调 `window.__TAURI_INTERNALS__.invoke` 绕过收口；③ 收口本身也依赖前端自觉。
2. **`sandbox` 字段：删除**，明确「前端不做隔离，安全边界只在 Rust 端 + WASM 端」；真做 `isolated`（iframe + postMessage 桥）登记为未来选项，不在本票。
3. **不做与票 03 审批的额外联动**：整单批准清单已逐条列出这些位（含 `process:run` / `pty:spawn` / `terminal:input` 等前端也有 API 面的高危位），且本票落地后前端通道读的 `granted` 集 = 批准 ∩ 请求；再弹一次是同一清单确认两次。

### 设计补充（选项 A 的实现细节，实施时不得弱化）

**为什么需要两级凭证而不是单一 token**：Tauri 同一 webview 内无调用者身份，「谁先请求」不可验证。若 `签发 token(plugin_id)` 本身可被任意前端代码调用，攻击者直接为受害者 id 签一枚即可冒充 → 单一 token 等于没做。因此：

- **loader 会话密钥**（宿主前端面凭证）：**每次页面加载首个调用者生效**，由宿主前端 bootstrap 在导入任何插件模块之前取得，保存在宿主模块作用域变量（不进全局/不进 storage）；重复请求返回「已签发」错误。宿主侧在页面加载时重置（Tauri 插件钩子 `on_page_load`），使 dev 页面刷新可重新取得。
- **插件通道令牌**（插件面凭证）：`plugin_channel_token(plugin_id, loader_session)` 校验 loader 密钥 + 插件已激活后签发，**停用即回收**。
- **每个插件面命令都必须带凭证**，凭证决定身份：解析出的身份是插件令牌时，「参数里的 `plugin_id` 只是目标」，与身份不符即拒绝（fail-closed）；解析出 loader 密钥时视为宿主面（宿主有权操作任意插件，如配置页读写插件存储、宿主 `pluginInvoke` 驱动插件命令）。
- **诚实登记残余面（不做硬隔离）**：同 realm 的原型/时序篡改理论上仍可尝试窃取凭据或他人 context（这正是裁决 2 删除 `isolated` 的原因）；本轮关闭的是「一行 `invoke` 自报 plugin_id」这条通道。另外两条已知边界写进验收：宿主管理面命令（activate / install / uninstall / approve / dev-reload）按裁决保持不绑定；插件前端的 Tauri 事件订阅面无法按调用者绑定身份。
