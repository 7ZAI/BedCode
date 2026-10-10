# ADR 0046：移动端壳改纯 surface 运行面形态 + 旧嵌入扩展点整面退役

- 状态：**accepted**
- 日期：2026-10-10
- spec：`.scratch/2026-10-10-mobile-full-ui-downsink/spec.md`
- 相关：ADR 0022（宿主/插件边界裁决）、ADR 0017（插件互调）、ADR 0018（移动契约独立）、ADR 0019（双端锁版）、ADR 0032（插件分类）、ADR 0033（认证中心自持签发验签）、AGENTS.md §5.1（六条判据）、§5.1.3（fail-visible 三形态）、§6（移动端前端优先对接宿主壳）

## 背景

1. **用户指令（2026-10-10）**：「wasm app 的界面应和之前的宿主前端完全一致，包括底部导航、设置页面」，并进一步明确「旧的宿主页面相关应当全部下沉到 wasm app 中，并且新的宿主壳以新的形式加载展示不同的 wasm app 页面，而不是旧宿主的插件形式」。
2. **实测的形态不一致**：票 2026-10-09 把旧宿主主流程下沉后，`terminal-session` 注册的运行面是「顶部分段切换（设备/会话）+ 终端整页沉浸」，**无底部导航**；而旧宿主是 4 tab 底部导航。用户指出的差异属实——「下沉」只搬了内容面，没搬导航外壳与设置面。
3. **壳仍在用旧插件形态加载页面**：`src/shell/adapters/pluginAppSource.ts` 的 `resolveSurface` 有四级回退链
   （`terminalView → toolbox → navTab → route`）。这是「壳用旧宿主的插件形式加载页面」的**实际代码**，
   也是壳与插件系统之间最后一道形态耦合。
4. **四个旧嵌入扩展点已全是死代码**（实测）：
   - `registerNavTab`：全仓零调用者；
   - `registerToolboxPage`：唯一调用者 `terminal-session/src/task/activate.ts`，其产物 `AutoTaskToolboxView` 改由 app 域运行面的「工具箱」页签直接渲染；
   - `registerTerminalView`：唯一调用者 `terminal-session/src/terminal/activate.ts`，消费者只有上面那条回退链；
   - `registerTerminalToolbarItem`：唯一消费者 `PluginTerminalBar.vue` 零引用，且宿主从未 `provide('bedcodeHostComponents')`，注入恒为空 ⇒ 运行时恒不渲染。
5. **业务设置真源在宿主**（`useMobileSettings`），本身命中 §5.1 **B3**（宿主持有产品事实的权威存储并对外读写），
   是越线形态；本票把业务设置真源下沉属**修正**而非新增越线。
6. **功能回归**（票 2026-10-09 阶段 B 造成，非本票）：旧 `SettingsView` 的「重置设置 / 清除所有数据」
   两个 action 随页面删除丢失，i18n key 与 composable 仍在但零 UI 调用。

## 决策

### D1 · 壳只认 `registerSurface` 一种运行面形态

`pluginAppSource.resolveSurface` 的四级回退链**删除**，只读 `ShellRegistry` 的 `contributions.surface`。
未注册 surface 的应用返回 `undefined`，由运行屏渲染「该应用尚未提供运行面」空态并写明原因。

**为什么必须删干净而不是降级为「优先 surface」**：
- 回退链让应用只注册一个内嵌片段就能被壳当成整个应用的主面 ⇒ 壳必须认识四种插件形态，正是本 ADR 要拆的耦合；
- 四级回退各带独立权限位（`ui:toolbox` / `ui:navtab` / `ui:input`），回退等于「拿到最弱权限也能进主面」。

### D2 · 四个旧嵌入扩展点整面退役（含权限位）

`registerToolboxPage` / `registerNavTab` / `registerTerminalToolbarItem` / `registerTerminalView`
及其描述符类型、宿主注册表存储面、宿主 UI 组件（`PluginNavTabHost` / `PluginSettingsHost` /
`PluginTerminalBar`）、dev-shell 对应面**一并删除**；权限位 `ui:toolbox` / `ui:navtab` / `ui:input`
从 SDK `VALID_PERMISSIONS` 与 `PERMISSION_API_MAP` 移除。

`registerRoute` / `openPage` / `goBack` / `onBackPressed` **保留**：应用内子页仍需要跳转与返回
（file-transfer 设置页、terminal-session 任务页）。这条与 D1 不冲突——`registerRoute` 走的是
「应用自持子页」而非「壳按插件形态拼装主面」。

**退役权限位必须显式登记 + 装载期拒载**（§5.1.3 fail-visible 形态③）：权限位不在白名单时
`grant_permissions` 会**静默丢弃**，旧插件会「加载成功但功能凭空消失」，排查方向被带偏。
故 SDK 增设 `RETIRED_PERMISSIONS` + `check_retired_permissions()`，宿主 `PluginLoader::load_all`
在身份校验后立即调用，命中即拒载并写明迁移出路；manifest-gen 同批剔除退役位与退役 `contributes`。

### D3 · 前端保留显性抛错桩，类型面同步移除

四个退役方法在 `context.ui` 上保留同名桩，调用即抛错并指名扩展点 + 给出迁移出路；
但**不进 `UIRegistry` 类型**——类型面必须让旧调用点编译期就红。

取舍：桩会让旧插件「加载时崩在 activate」而非「加载成功但功能缺失」。这是有意的——
断链可见优于断链静默，崩点也更可定位。

### D4 · 设置按「平台项留壳 / 业务项下沉」切分

| 项 | 归属 | 依据 |
| --- | --- | --- |
| 主题 / 语言 / UI 字号缩放 / 关于 / 更新检查 | 壳 | 平台机制与平台事实 |
| 出站授权 egress / 链路加密 / 生物凭证 | 壳 | ADR 0022 ②类安全闸门，fail-closed |
| 自动重连 / keepAlive / 默认端口 / 通知三开关 / 震动 / 声音 / 终端上限 / 首选认证 | terminal-session | 业务设置，B3 真源 |
| 重置设置（重置**业务**设置项） | terminal-session | 同上，并补回 §背景 6 的回归之一 |
| 清除所有数据（**设备级擦除**） | **壳**（设置屏危险区） | **与 D4 其余项反向**：清理对象含设备入场凭据与宿主连接态，按 §8 凭据零过境 + ADR 0033，插件不得持有或擦除凭据 |

宿主设置子页随之收窄：`ConnectionSettingsView` 只留链路加密、`AuthenticationSettingsView` 只留生物凭证、
`AppearanceSettingsView` 去掉终端上限、`NotificationSettingsView` 整页退役（`mobile-settings-notifications` 路由同批）。

### D5 · 文件浏览器留宿主作公共组件，入口经 `bedcodeHostComponents` 注入

文件浏览器（`CodeExplorerView` + `FileExplorer` / `FileSidebar`）是跨应用通用能力，
下沉会让每个应用各写一份 ⇒ 裁决**留宿主**（用户裁决，spec §2）。
全屏路由 `/mobile/files/:id` 保留；此外由 `App.vue` `provide('bedcodeHostComponents', { FileExplorer, FileSidebar })`
让 wasm app 终端域**早已写好但一直无人 provide 的两个挂载位**转为可用——既解决了入口可达（§5 C5），
也清掉了长期 `v-if` 恒假的死代码。

### D4b · 「清除所有数据」是设备动作，不是业务设置

票 2026-10-09 阶段 B 删掉旧 `SettingsView` 时丢失了「重置设置」与「清除所有数据」两个 action。
前者是**业务设置项的重置**，随 D4 归 terminal-session（已实现）。
后者**刻意留在宿主**：其清理对象包含设备入场凭据（认证中心托管）、宿主连接态
（配对设备 / 会话配置 / 活动会话）与跨应用共享的 `localStorage`——
这些都不是任何应用的业务事实，且按 §8 + ADR 0031/0033 插件**不得**持有或擦除凭据。
放进应用设置页不是「归属选择」，是 §5.1 越线。

实现落 `src/composables/useClearAllData.ts`（宿主 composable），入口挂 `ShellSettingsScreen`
危险区。执行序固定为「先断连 + 停前台服务 → 再清本地 → 最后 reload」：
顺序反了会让仍在跑的 WS 订阅把已清状态写回去。
失败口径刻意分开：**断连失败不阻断清理**（用户目标是清数据；阻断会让凭据留在盘上），
但结果里如实标记 `disconnected: false`；**清理失败不 reload、不吞错**，以
`completed: false` + 原因返回并由 UI 提示——重载会让用户以为已经清干净。

因壳单向调用宿主 composable，`shellConstraintLocks` 的 L7 桥接白名单新增一条
（登记的是 `ShellSettingsScreen.vue` 单文件），理由见该处注释。

### D6 · 业务设置真源下沉，宿主只留通用 KV 通道

业务设置项的「有哪些项 / 默认值 / 取值范围」是产品事实，自持在应用侧
（`wasm-apps/terminal-session/src/settings/settingsModel.ts`）。宿主只提供**通用机制面**的
KV 桥 `readAllSettings()` / `writeSetting()`——不绑定任何业务设置形状（命中 §5.1 ①类允许的引擎实现）。

写穿规则：键命中宿主已知字段（`mobile.*` 且在 `MobileSettings` 形状内）时写穿响应式单例，
由既有 watch 落盘并触发副作用（字号缩放 CSS 变量 / settingsStore），宿主消费者立即可见；
未命中则直落 KV。键名与 `MobileSettings` 字段同名是**刻意的**——改名会让宿主消费者静默读到默认值。

**与 spec 原文的偏离**：spec §3 批次 A 第 5 条写的是「业务设置进 `context.storage`；
宿主 `mobileSettings` 投影改读插件真源」。实际未按此实现，理由：
`context.storage` 是按插件 id 隔离的命名空间存储，让宿主投影改读它需新增一条
跨命名空间的读取通道——**扩大**宿主对插件存储的耦合，与「宿主只提供通用机制面」相悖。
KV 桥方案下宿主**不持有任何业务形状**（无项清单、无默认值、无取值范围），
这比原方案更贴 B3 的修正目标。真源仍在应用侧（应用决定有哪些项、默认值与区间），
宿主只负责持久化与投影，故本 ADR 认可实际落地形式，spec 已同步改判。

## 与既有裁决的关系

- **AGENTS.md §5.1 B3**：设置真源从宿主下沉到插件，是修正越线形态，不是新增。
- **AGENTS.md §6「移动端前端：优先对接宿主壳」**：本票把业务 UI 从壳移入 wasm app，与该条
  「默认落点」表述相反，但与该条自身的「**业务不落壳**」一致。需在 AGENTS.md §6 补例外条款：
  **壳只保留平台机制与运行面挂载，业务页面一律归 wasm app**。
- **票 2026-10-09 spec.md:129「设置留壳」建议**：本 ADR **反转**该建议（依据 AGENTS.md §0 优先级 1，
  用户当前明确指令优先于文档规则）。
- **AGENTS.md §0 冲突裁决**：本票触及 §5.1 宿主/插件边界与 §6 默认落点两条，故落 ADR 而非直接改文档。

## 影响面

**机制面收缩**：插件 UI 扩展点由 9 个减为 5 个（`registerSurface` / `registerSlot` /
`registerCapsuleItem` / `registerSettingsEntry` / `registerRoute`）。这是**有意的收缩**：
壳不再认识四种「插件嵌入形态」，应用在壳内只有一种运行面形态。

**双端不同步**：本 ADR 只动移动端。移动端插件契约独立（ADR 0018），**不跟演**桌面端的部分 ABI
破坏性变更；桌面端的 `ui:toolbox` / `ui:input` 等位不受影响。wasmtime 版本不变（ADR 0019 无触发）。

**遗留**：`TerminalView.vue` / `TaskEditDialog.vue` 另有 `FileSidebar` / `FileExplorer` 两处注入，
D5 后转为可用；`FileExplorer` 组件自身在本票未做形态调整。

## 验证

- `bedcode-mobile`: `pnpm run test:run` 全量 + `pnpm exec eslint .` 0 error；
  防回接锁 `retiredHostUIRetirementLocks.test.ts` 扩展 R4 / R4b / R5
  （旧嵌入扩展点不得回接、运行面解析无回退链、已下沉设置路由不得回接、已删孤儿不得回接）。
- SDK Rust：`packages/plugin-sdk-mobile/rust` `cargo test --features wasm`，
  含 `check_retired_permissions` 的正反例与「退役位不得留在白名单/映射表」双重保险。
- 宿主 Rust：`bedcode-mobile/src-tauri` 的退役锁测试把原「正向钉住 `ui:toolbox` / `registerToolboxPage`」
  的断言**反转为退役面**；装载期拒载路径需在 wasm-core 的 WIT 分片票（票 02）合流后可跑全量
  （当前 `bedcode-wasm-core` 因在途 WIT 重构无法编译，见交付说明）。
