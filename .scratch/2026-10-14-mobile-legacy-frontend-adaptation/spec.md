# 移动端前端回退后的旧前端 × 当前新后端适配

**Status**: ready-for-agent（2026-10-14 用户裁决三条后落文，可直接开工）

> 用户裁决（2026-10-14）：
> ① 前端扩展点**回退到旧版本**——只回退 SDK `src/types.ts` 类型面（+ dist 构建产物），
>    宿主前端与 wasm-app 前端已回退（三方同代配套，编译天然全绿）；
> ② 状态枚举运行值**只能适配**（宿主 Rust 未回退 → camelCase 小写，前端比较逻辑适配）；
> ③ 事件桥（ws_pairing_* / ws_event_channel_ready）真机/集成验证旧事件名仍转发，若不转发
>    接受「旧 UI 走插件事件」或补转发（验证后裁决）。
>
> 背景：dev 分支移动端前端整体回退至 `92222e8b6`（旧宿主最新版，宿主前端 + 三个
> wasm-app 前端），新壳实现保存于 `feature/mobile-shell-host`（worktree `bedcode-shell-host`）。
> 本规格 = 让**回退后的旧前端**适配**当前（未回退）新后端**的命令面 / 事件面 / SDK 契约面。

## 0. 事实基线（实测，2026-10-14）

1. **回退范围**：`bedcode-mobile/src/**` 宿主前端 + 三个 wasm-app 的 `src/**` 前端回退至
   `92222e8b6`（e92cc40a3 父提交 = 旧宿主前端退役前最后完整态：views 全齐、旧扩展点
   `registerNavTab/registerToolboxPage/registerTerminalView/registerSettingsSection/
   registerTerminalToolbarItem` 全在、`permission.ts` 旧权限门在）。Rust 后端 / SDK /
   wasm-app 的 `plugin.json`·`rust/`·构建配置保持当前（用户「其他保持不变」）。
2. **编译裂缝 41 个 TS 错误**（`vue-tsc --noEmit` 实测）：宿主 35（`views/PluginView.vue` 16、
   `shell/adapters/pluginAppSource.ts` 10、`plugin/types.ts` 5、`plugin/loader.ts` 3、
   `plugin/context.ts` 1）+ terminal-session 3 + ai-chatbox 1 + file-transfer 2。根因两类：
   - **SDK 类型面已退役旧扩展点**：当前 UIRegistry 只含 `registerSurface/registerSlot/
     registerCapsuleItem/registerSettingsEntry/registerRoute/openPage/goBack/onBackPressed/
     showDialog`（`packages/plugin-sdk-mobile/src/types.ts:521`），旧方法全删 → 描述符类型
     （`ToolboxPageDescriptor/NavTabDescriptor/TerminalToolbarItemDescriptor/
     SettingsSectionDescriptor/TerminalViewContribution`）也不存在；
   - **状态枚举大小写**：当前 SDK TS 类型为小写（`'activating'/'activated'/'degraded'/
     'needsApproval'`），旧前端代码比较大写（`'Activated'` 等）→ TS2367 失配。
3. **状态运行值 = 宿主 Rust 序列化**（未回退）：`PluginState` serde
   `#[serde(tag = "state", rename_all = "camelCase")]` → 前端收到 `'activated'` 等小写
   （宿主 Rust 消费 SDK rust 侧契约，**SDK rust 侧绝不回退**）。
4. **命令面缺口 2 个**（54 个旧前端命令 × 当前后端注册，实测）：
   - `set_db_setting` → 后端已改名 `set_db_setting_mobile`（JSON 文件存储，`mobile_commands.rs`，
     签名 `key/value/updated_at` 兼容）；
   - `mdns_start_advertise` / `mdns_stop_advertise` → 宿主侧已退役
     （`retired_mobile_mdns_advertise_lock.rs`：移动端广播面零活消费者死面，2026-10-10
     能力域集中化；前端 `useMdnsAdvertiser.ts` 为唯一调用点）。
5. **事件面缺口 6 个 → 待验证**：旧前端监听 `ws_connecting/ws_disconnected/
   ws_event_channel_ready/ws_link_crypto_pin/ws_pairing_request/ws_pairing_verified`；
   当前后端直接 emit 仅 6 个（`ws_connected/ws_reconnected/ws_reauth_rejected/ws_auth_success/
   ws_paired/egress_consent_request`）。但 `92222e8b6` 恰为票 14 阶段 B 落地后：配对流程事件
   （ws_pairing_request/verified）由 **wasm-app 插件**（terminal-session auth.rs）经
   host-events 广播、事件名与载荷逐字一致（`useMobileCommands.ts` 头注原文）；
   `connection-events.ts` 仍注册 `ws_event_channel_ready`。宿主 guest_events 通道转发
   是否保留旧事件名 = 运行期验证点（§4 T5）。
6. **wasm-app rust 命令面新旧兼容**（实测）：`terminal-session.{request-pairing,list-sessions,
   start-session,stop-session,send-input}` 新旧一致；宿主侧 `plugin_invoke` 命令仍在。
7. **SDK dist 由 tsup 生成**：`pnpm run build`（`tsup src/index.ts src/vite-plugin.ts
   src/types.ts --format esm --dts --clean`）→ 回退 `src/types.ts` 后需重建 dist，
   宿主与 wasm-app 前端经 `file:` 依赖解析到 dist。
8. **宿主前端自持 UI 注册表实现**：`src/plugin/context.ts` 的 `createPluginContext` 构造
   `ui: UIRegistry` 对象（内部调 `registry.registerToolboxPage` 等，回退后旧实现已在位），
   **SDK 只提供类型不提供实现** → 回退 SDK types.ts 即让宿主与 wasm-app 前端三方同代。

## 1. Problem Statement

移动端前端回退到旧宿主最新版（`92222e8b6`）后，无法在当前新后端上编译运行：
SDK 类型面已退役旧扩展点（41 个 TS 编译错误）、两条命令缺失/退役（`set_db_setting` /
`mdns_start_advertise`）、状态枚举大小写失配（宿主 Rust camelCase vs 旧前端大写比较）、
插件事件桥转发面未验证。目标：**回退后的旧前端（宿主 UI + wasm-app 前端）在当前新后端上
编译通过、可运行**，同时不触碰新后端（Rust 零改动）、不污染共享 SDK rust 契约。

## 2. Solution

三层适配，按「能回退就回退、不能回退就适配」原则：

1. **扩展点契约回退（回退侧）**：`packages/plugin-sdk-mobile/src/types.ts` 回退至
   `92222e8b6` 版本（UIRegistry 旧方法 + 旧描述符类型 + 旧状态类型），重建 dist。
   宿主前端 `src/plugin/*`（已回退，含旧权限门）与 wasm-app 前端（已回退，
   `registerNavTab` 等原样可用）三方同代，41 个 TS 错误中「方法/类型不存在」类全消。
2. **状态枚举适配（适配侧）**：宿主前端 `src/plugin/loader.ts` + `src/views/PluginView.vue`
   + `src/shell/adapters/pluginAppSource.ts` 的状态比较逻辑从大写（`'Activated'`）改为
   camelCase（`'activated'`），对齐宿主 Rust 运行值。不动 SDK 类型。
3. **命令面适配（适配侧）**：`useMobileSettings.ts` 改调 `set_db_setting_mobile`；
   `useMdnsAdvertiser.ts` 移除广告调用（移动端广播死面，与退役锁一致），App.vue 中
   广告入口同步清理（若存在）。
4. **事件桥验证（验证侧）**：真机/集成验证 `ws_pairing_request/ws_pairing_verified/
   ws_event_channel_ready/ws_connecting/ws_disconnected/ws_link_crypto_pin` 在当前
   宿主 guest_events 通道是否仍转发到前端；不转发则按用户裁决（旧 UI 走插件事件
   或补转发）落定。

## 3. User Stories

1. 作为移动端用户，我希望设备列表页能正常加载已发现设备，以便查看可用设备（mdns browse 命令面）
2. 作为移动端用户，我希望配对流程（配对码/QR）在旧宿主 UI 上完整走通，以便连接桌面端
3. 作为移动端用户，我希望会话列表能显示/创建/停止会话，以便管理远程会话
4. 作为移动端用户，我希望终端页能打开插件注册的终端视图并收发输入，以便远程操作
5. 作为移动端用户，我希望工具箱能显示插件注册的工具箱页与预设任务，以便使用插件能力
6. 作为移动端用户，我希望 AI Chatbox 插件以导航页签形态出现，以便聊天
7. 作为移动端用户，我希望文件传输插件以工具箱页/设置区块形态出现，以便传文件
8. 作为移动端用户，我希望插件管理页的启用/停用/审批流转正常，以便管理插件
9. 作为移动端用户，我希望设置页能保存/读取移动端设置，以便连接参数持久化
10. 作为移动端用户，我希望连接状态（connecting/connected/disconnected/reconnected）
    在 UI 上正确反映，以便了解链路状态
11. 作为移动端开发者，我希望 dev 前端 `vue-tsc --noEmit` 0 error，以便 CI/本地开发可跑
12. 作为移动端开发者，我希望 dev 前端测试套件（回退带回的旧套件）全绿，以便回归
13. 作为移动端开发者，我希望 SDK rust 侧契约与 dist 构建产物不被破坏，以便宿主 Rust 不受影响
14. 作为移动端开发者，我希望新壳实现（feature/mobile-shell-host worktree）不受 dev 适配影响，
    以便双轨并行

## 4. Implementation Decisions

### D1. 扩展点回退 = 只回退 SDK `src/types.ts` 类型面

- **回退对象**：`packages/plugin-sdk-mobile/src/types.ts` → `92222e8b6` 版
  （旧 UIRegistry 接口 + 旧描述符类型 + 旧 PluginState 类型）。
- **重建 dist**：SDK 目录 `pnpm run build`（tsup 三入口含 types.ts）。
- **绝不回退**：SDK `rust/` 侧、`src/{index,registry,...}.ts` 实现面（宿主 Rust 依赖
  当前 SDK rust 契约，WIT/ABI 锁；回退会炸后端）。
- **宿主前端已回退自带实现**（`context.ts` 构造 `ui` 对象调旧 registry 方法），
  无需额外翻译层——这是「回退优于适配」的关键依据（D0 判据：SDK 只出类型、实现住宿主）。

### D2. 状态枚举 = 前端比较逻辑适配 camelCase

- **改**：`src/plugin/loader.ts`、`src/views/PluginView.vue`、
  `src/shell/adapters/pluginAppSource.ts` 的状态字面量比较：大写 → camelCase。
- **不动**：SDK 类型（回退后旧类型若为大写——需以 92222e8b6 实际为准；若回退类型
  即 camelCase 则比较逻辑无需改，落地时以 `vue-tsc` 全绿为准）。
- 状态值真源 = 宿主 Rust `PluginState` serde camelCase（`types.rs`），运行期以
  `plugin_list_loaded` / `plugin_get_info` 实际 payload 为准做单测断言。

### D3. 命令面适配

- `useMobileSettings.ts`：`set_db_setting` → `set_db_setting_mobile`（参数形状
  `{ key, value }` 已核对兼容）。
- `useMdnsAdvertiser.ts`：整体移除广告调用（`mdns_start_advertise/stop_advertise` 后端
  已退役 + 退役锁钉死不得复活）；`App.vue` 中广告入口同步清理（若引用）。
- 其余 52 个命令后端均在（实测），零改动。

### D4. wasm-app 前端 = 零业务改动

回退后的 wasm-app 前端（terminal-session/ai-chatbox/file-transfer 的 `src/**`）在
SDK 类型面回退后应直接编译通过（`registerNavTab` 等原样可用）；仅当 `vue-tsc` 残留
错误时才动注册入口（预期零残留）。**D0 判据保底**：若 SDK 类型回退不足以全绿，
则改为「宿主前端补旧扩展点注册表 + wasm-app 前端改注册入口」的适配路线（备选 D0'，
见 §7 风险 R2）。

### D5. 事件桥验证（§3 用户故事 2/10 的门禁）

- 验证点：`ws_pairing_request/ws_pairing_verified/ws_event_channel_ready/ws_connecting/
  ws_disconnected/ws_link_crypto_pin` 是否仍从当前宿主转发到前端。
- 路径：宿主 `guest_events.rs`（插件事件导出派发）+ `connection/manager.rs`（连接状态
  事件）。实测当前 manager 仅 emit `ws_connected/ws_reconnected/ws_reauth_rejected`；
  `ws_connecting/ws_disconnected` 疑似已并入插件事件流——以运行期验证为准。
- 裁决（用户已定）：不转发则接受「旧 UI 走插件事件」（配对/会话事件由 wasm-app
  经 host-events 广播，事件名逐字一致），前端监听零改动；验证发现缺转发才补。

## 5. Testing Decisions

- **只测外部行为，不测实现细节**：以「旧前端在当前后端上编译 + 运行链路走通」为判据。
- **Seam 1（TS 编译 seam）**：宿主 `vue-tsc --noEmit` 与三个 wasm-app 各一次，0 error——
  主验证面，覆盖 D1/D2/D3 全部改动。
- **Seam 2（前端行为 seam）**：宿主 `pnpm run test:run`（端目录）全绿——回退带回的
  旧套件（views/composables/plugin/integration）+ wasm-app 域测试（宿主 vitest include
  驱动）。重点断言：`set_db_setting_mobile` 改名、广告调用移除、状态 camelCase 比较、
  插件注册链路（ui:toolbox → registerToolboxPage 权限门）。
- **Seam 3（命令/事件 seam）**：前端单测 + 集成测试覆盖命令改名与事件监听名；
  插件事件桥转发由集成测试（`__tests__/integration/pairing-flow.test.ts` /
  `connection-flow.test.ts`）驱动 mock 验证。
- **Seam 4（运行期 seam，手工）**：真机/浏览器核验——设备列表 / 配对 / 会话 / 终端流 /
  工具箱渲染链路走通（§10 手工项逐条填「跑了/没跑 + 原因」）。
- **回归护栏**：`pnpm exec eslint .` 0 error（warning 不计）、`cargo test` 不跑移动
  后端（Rust 零改动，仅确认编译不破）；WIT 未动 → `compose-wit --check` 应保持绿。

## 6. Out of Scope

- **新后端 Rust 零改动**：本规格不改 `src-tauri/**`（命令面/事件面/权限门维持当前）。
- **SDK rust 侧 / WIT / ABI**：不回退、不升级。
- **新壳实现**（`feature/mobile-shell-host` worktree）：不受 dev 适配影响，不合并。
- **wasm-app 的 `plugin.json` / `rust/` / 构建配置**：保持当前（用户「其他保持不变」）。
- **文档回退**：`bedcode-mobile/docs/code-map.md` 与 README 保持新形态描述（双轨并存期
  可在头注标注，但不回退内容）。
- **iOS / 桌面端**：不在范围。
- **mdns 广播能力**：不复活（退役锁钉死）。

## 7. Further Notes

- **风险 R1（状态枚举回退陷阱）**：92222e8b6 的 SDK 类型若为大写而宿主 Rust 现发
  camelCase，则「回退类型 + 不改比较」= 编译过但运行错——必须按 D2 以运行 payload 为准
  适配比较逻辑，落地时先写断言再改代码。
- **风险 R2（D0' 备选路线）**：若回退 SDK 类型不足以让宿主前端 `vue-tsc` 全绿
  （例如 `shell/adapters/pluginAppSource.ts` 依赖新壳类型），则升级为适配路线：宿主前端
  补 `src/plugin/legacyRegistry.ts` 兼容层，wasm-app 前端改注册入口。R2 触发即停下向用户
  确认（AGENTS §0），不自行切换路线。
- **风险 R3（事件桥缺转发）**：若 ws_pairing_* 不再转发到前端，配对流程在旧 UI 可能
  卡在「等待桌面端」——按 D5 用户裁决执行，验证结果记录于 spec 评论。
- **提交纪律**：改动仅前端路径（`bedcode-mobile/src/**` + `packages/plugin-sdk-mobile/
  src/types.ts` + dist），提交用 `git commit -- <paths>` 限定，避免卷走并行会话
  （peer 重构）的在途改动；回退与适配可合并为一笔或分两笔（`revert(mobile)` +
  `fix(mobile)`），由交付时工作区状态定。

## 8. 交付记录（2026-10-14 落地）

提交 `8334007d9` `fix(mobile): 旧前端适配当前新后端`（60 files，+626/-6088；
回退提交 `407e35e81` 已完成在前）。

### D1 扩展点回退（types.ts）

- 回退对象确认：`packages/plugin-sdk-mobile/src/{types,index}.ts` → `92222e8b6` 版
  （旧 UIRegistry 方法 + 旧描述符类型），dist 重建（tsup，gitignore 不入库）。
- **R1 陷阱落地**：92222e8b6 的 `PluginState` 确为 PascalCase（`'Activated'` 等），
  宿主 Rust 现发 camelCase——按 spec D2 以运行 payload 为准，**PluginState 段落保留
  当前 camelCase 版**不回退（编译过且运行对；若整段回退则「编译过但运行错」）。

### D2 状态枚举适配

`src/plugin/loader.ts`（activating/degraded 门禁）、`src/shell/adapters/
pluginAppSource.ts`（mapState 七分支 + permissions 判定）、`src/views/PluginView.vue`
（16 处模板/脚本字面量）全部改为 camelCase；被测夹具与断言同步
（fixtures/plugin.ts + 7 个测试文件，`NeedsApproval`→`needsApproval` 等）。

### D3 命令面适配

- `src/composables/useMobileSettings.ts`：`set_db_setting` → `set_db_setting_mobile`，
  **顺带补出 spec 漏列的 `get_all_db_settings` → `get_all_db_settings_mobile`**
  （同为退役旧名，实测 92222e8b6 时代后端已无旧名、前端静默 catch）。
- `src/composables/useMdnsAdvertiser.ts`：移除广告 invoke 调用（广播面退役，
  与 `retired_mobile_mdns_advertise_lock` 一致），保留空壳状态机；App.vue 广告入口
  旧版已注释零改动。
- `plugin_open_file*`（context.ts SystemAPI）为 92222e8b6 既有断链（当时后端亦无），
  spec 未点名、非回退引入，保持不动。

### D5 事件桥验证（运行期路径码查 + 集成测试驱动）

| 前端监听事件 | 发射源 | 转发状态 |
| --- | --- | --- |
| ws_pairing_request / ws_pairing_verified | terminal-session 插件 auth.rs `emit_event`（host-events）→ 内核 core_binding → `app_handle.emit` | ✓ 逐字一致，前端零改动 |
| ws_connecting | connection/manager.rs + router/event.rs `emit_connecting` | ✓ |
| ws_disconnected | router/event.rs `emit_disconnected` | ✓ |
| ws_event_channel_ready | connection/manager.rs `EVENT_CHANNEL_READY_EVENT` | ✓ |
| ws_link_crypto_pin | router/event.rs + commands/http_proxy.rs | ✓ |
| ws_connected / ws_reconnected / ws_reauth_rejected / ws_auth_success / ws_paired | 宿主直接 emit | ✓ |

**结论：6 个旧事件名全部仍在当前宿主转发路径上，前端监听零改动即达**——按用户
裁决「不转发才补」，无需补转发、无需改监听。集成测试 pairing-flow / connection-flow
驱动 mock 验证事件链。

### 清理回退残留

`wasm-apps/terminal-session/src/{app,host,settings}` 与仅存在于新壳时代（e92cc40a3 /
675c660b2）的 6 个宿主测试文件（pluginContextShell / retiredHostUIRetirementLocks /
shellSettingsScreen / useShellAppsSurface / connectionEvents / useClearAllData）——
92222e8b6 均无、引用已退役 registerSurface 族 API，为 407e35e81 回退不完整遗留，
删除；vitest include 回退为 92222e8b6 形态（src/__tests__ + terminal/__tests__）。

### 新增回归测试

- `useMobileSettings.test.ts`：改名命令 `set_db_setting_mobile` + `mobile.` key 前缀
  + 旧名 `set_db_setting` 不复辟（反向护栏）+ 单次 invoke reject 不阻断后续项；
  变异自检：命令名回退后 3 用例全红。
- `useMdnsAdvertiser.test.ts`：零后端调用（广播退役）+ 状态机保真 + 幂等守卫；
  变异自检：广告 invoke 复活后 C-1/C-3 变红。
- `useMdnsDiscovery.test.ts`：mock `frontendLogger` 消除异步日志攒批 flush 对
  `not.toHaveBeenCalled` 断言的 flaky（既有测试时序问题，非本任务引入）。

### 验证证据

- 宿主 `vue-tsc --noEmit` 0 error；三个 wasm-app 各自 0 error（41 个基线错误全消）。
- 宿主 `pnpm run test:run`：**69 文件 / 739 用例全绿，连跑 4 次稳定**。
- 根 `pnpm exec eslint .`：0 error（110 warning 不计）。
- WIT 未动：`compose-wit --all --check` 19 分片 ok。
- Rust 后端零改动（未编译验证，改动面不含 src-tauri）。
