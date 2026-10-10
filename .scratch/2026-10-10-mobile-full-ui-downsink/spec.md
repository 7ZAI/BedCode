# 移动端全量 UI 下沉 wasm app + 宿主壳改 surface 形态加载

> 状态：批次 A / C / D 已实施（2026-10-10）；ADR 0046 记账。批次 B 已取消（文件浏览器留宿主）
> 起因：用户指出「wasm app 的界面应和之前的宿主前端完全一致，包括底部导航、设置页面」，
> 并进一步明确「旧的宿主页面相关应当全部下沉到 wasm app 中，并且新的宿主壳以新的形式加载
> 展示不同的 wasm app 页面，而不是旧宿主的插件形式」。

---

## 0. 与既有裁决的关系（必须先记账）

本票**反转** `.scratch/2026-10-09-mobile-host-into-wasm-apps/spec.md:129` 的
「设置留壳」建议，并**扩大**票 2026-10-09 阶段 B 的退役范围。

依据 AGENTS.md §0 优先级 1（用户当前明确指令优先于文档规则）。但仍需落 ADR，因为触及三条：

- **AGENTS.md §5.1 B3**：设置真源当前在宿主（`useMobileSettings`），本身是越线形态；
  本票把业务设置真源下沉，是**修正**而非新增越线。
- **AGENTS.md §6「移动端前端：优先对接宿主壳」**：本票把业务 UI 从壳移入 wasm app，
  与该条默认落点相反。需在 AGENTS.md §6 补例外条款：**壳只保留平台机制与运行面挂载，
  业务页面一律归 wasm app**（这与 §6「业务不落壳」一致，冲突的只是「默认落点」表述）。
- **旧插件扩展点退役**：属机制面收缩，需 ADR 记「旧插件 UI 嵌入形态整面退役」。

## 1. 现状事实（已核实）

### 1.1 wasm app 侧

| app | 注册的扩展点 | 证据 |
| --- | --- | --- |
| terminal-session | `registerSurface`(host/activate.ts:18)、`registerSlot`(:19)、`registerTerminalToolbarItem`(task/activate.ts:271)、`registerToolboxPage`(:356)、`registerRoute`(:370)、`registerCapsuleItem`(:376)、`registerTerminalView`(terminal/activate.ts:48) | — |
| file-transfer | `registerSurface`(index.ts:52)、`registerRoute`(:55)、`registerSettingsEntry`(:64) | — |
| ai-chatbox | `registerSurface`(index.ts:41) | — |

terminal-session 的运行面 `HostPage.vue` 现状：**顶部分段切换**（设备/会话，:43-54）+
终端整页沉浸，**无底部导航**。这与旧宿主 4 tab 底部导航不一致——即用户指出的差异。

### 1.2 宿主壳侧（新的 surface 形态已就位）

`ShellAppRunScreen.vue:53-60` 已明确：胶囊 + homebar 之外**全是应用运行面**，壳不给应用
加标题栏/返回键。`ShellHost.vue:72-75` 运行面不显示平台 Tab。**新形态的骨架已经对了。**

### 1.3 旧插件形态仍在寄生（用户所指）

`src/shell/adapters/pluginAppSource.ts:187-204` 的 `resolveSurface` 在应用未注册
`registerSurface` 时，**回退链**依次尝试 `terminalView → toolbox → navTab → route`
四个旧扩展点。这是「壳用旧宿主的插件形式加载页面」的实际代码。

旧 UI 宿主组件存活状况（grep 外部引用）：

| 组件 | 状态 |
| --- | --- |
| `PluginNavTabHost.vue` | **零引用**（死面） |
| `PluginSettingsHost.vue` | **零引用**（死面） |
| `PluginTerminalBar.vue` | **零引用**（死面） |
| `PluginViewHost.vue` | 活（context provider 包装器，pluginAppSource:224 依赖） |
| `PluginRouteView.vue` | 活（router 经 `meta.pluginRoute`，routes.ts:34） |
| `PluginDialogHost.vue` | 活（App.vue:16 全局挂载） |

`registerNavTab` 全仓零消费者（wasm-apps 内无调用）。

### 1.4 仍在宿主的宿主 UI（待下沉）

**页面**：`views/CodeExplorerView.vue`、`views/settings/` 6 子页（1387 行）。
**组件孤儿**（对应 wasm app 已有副本，宿主侧成死代码）：
`DeviceCard` / `InputBar` / `PluginIcon` / `RepeatableToggle` / `SessionListItem`。
**composable 孤儿**：`useAndroidFeatures` / `useRunTime`。

### 1.5 已被删除但需回到 wasm app 的面（票 2026-10-09 阶段 B 退役）

`DevicesView`(1191) / `PluginView`(858) / `PresetTasksView`(587) / `SessionsView`(540) /
`SettingsView`(413) / `TerminalView`(74) / `ToolboxView`(205) /
`MobileNav`(272) / `MobileLayout`(62) / `MobileStatusBar`(86) / `MobileSwipeContainer` /
`ScanPanel`(356) / `PairingInput`(273) / `SessionCard`(150) / `PresetTaskCard`(161) /
`BiometricAuthDialog`。原件可用 `git show e92cc40a3^:<path>` 取回，**视觉真源**。

### 1.6 功能回归（已发生，非本票造成）

旧 `SettingsView` 的「重置设置」/「清除所有数据」两个 action 随删除丢失，
i18n key（`locales/*/settings.ts:157-159`）与 composable（`useMobileSettings.ts:165`）
仍在但**零 UI 调用**。本票必须补回。

---

## 2. 归属映射

| 旧宿主面 | 目标 wasm app | 备注 |
| --- | --- | --- |
| MobileNav 底部导航 | terminal-session | 4 tab：连接 / 会话 / 工具箱 / 设置 |
| DevicesView（连接/设备） | terminal-session `host/` | 已有 `DevicesSection`，需补导航外壳 + 扫码/配对面 |
| SessionsView（会话） | terminal-session `host/` | 已有 `SessionsSection` |
| ToolboxView + PresetTasksView | terminal-session `task/` | 已有 `AutoTaskToolboxView` + 两页签 |
| SettingsView 首页 | terminal-session 新增 `settings/` | 业务项 |
| settings 子页（业务项） | terminal-session `settings/` | 见 §3 切分表 |
| settings 子页（平台项） | 壳 | 外观/关于/出站/链路加密 |
| CodeExplorerView + File* 组件 + useFileTree/useCodeHighlight | **宿主（公共组件）** | 用户裁决：文件浏览器留宿主作公共组件，不下沉。路由 `/mobile/files/:id` 保留 |
| PluginView（插件管理） | 壳 | 已是 `ShellAppsScreen` / `ShellAppDetailScreen` |
| TerminalView | terminal-session `terminal/` | 已完成 |

### 设置切分口径（§5.1 B3 裁决）

| 项 | 归属 | 依据 |
| --- | --- | --- |
| 主题 / 语言 / UI 字号缩放 | 壳 | `--mobile-font-scale` + i18n store 是平台机制 |
| 关于页 / 更新检查 | 壳 | 平台事实 |
| 出站授权 egress / 链路加密 | 壳 | ADR 0022 ②类安全闸门，fail-closed |
| 自动重连 / keepAlive / 默认端口 / 通知三开关 / 震动 / 声音 / 终端上限 / 首选认证 | terminal-session | 业务设置，B3 真源 |
| 重置设置（重置**业务**设置项） | terminal-session | 同上，且补回 §1.6 回归之一 |
| 清除所有数据（**设备级擦除**） | **壳**（危险区） | **改判**：清理对象含设备入场凭据与宿主连接态，按 §8 凭据零过境 + ADR 0033，插件不得触碰；它是设备生命周期动作而非产品事实 |

---

## 3. 批次计划

### 批次 A：terminal-session 补齐完整 UI（导航 + 3 页 + 业务设置）
1. `host/components/NavBar.vue`：逐字复刻 `e92cc40a3^:src/components/MobileNav.vue`
   （4 tab + 激活指示条 + 图标几何注释全留），改为 app 内页签状态机（不用 vue-router）
2. `HostPage.vue` 改为「页签容器 + NavBar」结构，替换现有分段切换
3. 补回 DevicesView/SessionsView/ToolboxView 的旧交互（扫码 ScanPanel、配对 PairingInput、
   会话卡 SessionCard、预设任务卡 PresetTaskCard）
4. `settings/` 域：业务设置项 + 重置/清除 action
5. ~~真源：业务设置进 `context.storage`；宿主 `mobileSettings` 投影改读插件真源~~
   **已改判（见 §6 收口）**：实现为宿主侧**通用 KV 桥**（`readAllSettings` / `writeSetting`）+
   宿主已知键写穿；业务项定义 / 默认值 / 取值范围自持在应用侧。理由见 ADR 0046 D6

### 批次 B：~~file-transfer 承接文件浏览器~~ → 已取消
用户裁决：**文件浏览器作为宿主侧公共组件保留**，不下沉。原 `CodeExplorerView` +
`File*` 组件 + `useFileTree` / `useCodeHighlight` / `useSwipeTabs` 留在宿主，
路由 `/mobile/files/:id` 保留。插件如需浏览远程文件，走既有宿主命令面。

### 批次 C：壳改纯 surface 形态 + 退役旧插件面
1. `pluginAppSource.ts:187-204` 回退链删除，只认 `registerSurface`
2. 退役 `registerNavTab` / `registerToolboxPage` / `registerTerminalToolbarItem` /
   `registerPluginRoute` / `registerTerminalView` 五个扩展点及其宿主 UI 组件
3. app 内页签/子页改走 app 自有路由（不占宿主 router）
4. 删 `ShellSettingsScreen` 中已下沉的入口；6 条 `mobile-settings-*` 路由按新归属处置
5. 文件浏览器作为公共组件，其入口需从宿主可达（不随壳退役）

### 批次 D：清理与锁同步
1. 删 5 个组件孤儿 + 2 个 composable 孤儿
2. `retiredHostUIRetirementLocks.test.ts` R1/R2/R3 扩展到旧插件扩展点与新退役面
3. `shellSettingsScreen.test.ts` 按新归属改写
4. 两端 code-map + CHANGELOG 双语 + ADR

---

## 4. 阻塞与风险

1. **设置写路径**：插件侧当前只有只读投影（`plugin/index.ts:71`），
   `useMobileSettings` 的写面（`set_db_setting` / `get_all_db_settings` / localStorage /
   settingsStore 同步）需重新设计。真源进插件 storage 后，宿主 `mobileSettings` 投影
   与 `syncToSettingsStore` 的 CSS 变量副作用需重新接线。
2. **凭据零过境**（C4）：设置里的「首选认证方式」与认证子页涉及凭据面，
   迁入插件时必须遵守 ADR 0033 与 C4，不得让凭据材料过境。
3. **fail-visible**：每批真源搬迁必须配齐 §5.1.3 三形态（旧读路径删除或显性报错 /
   旧 ABI 产物实例化期点名 / 退役权限位加载即抛）。
4. **规模**：A+B 两批合计约 3000+ 行前端代码 + 测试，不是单 session 可交付。

---

## 5. 待确认

- [x] 批次顺序是否 A → C → D（**B 已取消**）
- [x] file-browser 归属 → **宿主侧公共组件**（用户裁决）
- [x] 设置平台项（外观/关于/出站）留在壳的 UI 形态维持现状 —— 平台项 UI 形态未改，
      只把同页里的业务项拆走（ADR 0046 D4）

---

## 6. 实施结果（2026-10-10 收口）

| 批次 | 状态 | 落点 |
| --- | --- | --- |
| A terminal-session 补齐完整 UI | **完成（A3 见下方诚实记账）** | `wasm-apps/terminal-session/src/app/**`（AppRoot + NavBar + 页签状态机 + 嵌套横滑仲裁）、`src/settings/**`（业务设置项 + 重置）、`src/host/**` 出内容面、宿主 `useMobileSettings` 通用 KV 桥 |
| B file-transfer 承接文件浏览器 | **取消** | 用户裁决留宿主；入口经 `App.vue` 的 `bedcodeHostComponents` 注入 |
| C1 壳改纯 surface 形态 | **完成** | `pluginAppSource.resolveSurface` 回退链删除 |
| C2 四个旧嵌入扩展点整面退役 | **完成**（`registerRoute` 保留） | SDK types / dev-shell / template / 宿主 context+registry+permission / terminal-session 两域调用点 / 三个宿主死组件 |
| C2e 退役权限位 fail-visible | **完成** | SDK `RETIRED_PERMISSIONS` + `check_retired_permissions()`、宿主 `load_all` 拒载、manifest-gen 幂等剔除、三个 app 的 manifest |
| C4 设置子页按归属收窄 | **完成** | 连接页只留链路加密、认证页只留生物凭证、外观页去终端上限、通知页整页 + 路由退役 |
| C5 文件浏览器入口可达 | **完成** | `App.vue` provide `bedcodeHostComponents` |
| D1 删宿主孤儿 | **完成** | 5 组件 + 2 composable（+ `PluginIcon` 测试） |
| D2/D3 锁与测试 | **完成** | `retiredHostUIRetirementLocks.test.ts` 加 R4 / R4b / R5；`shellSettingsScreen.test.ts` 按新归属改写并加反向断言；`pluginContextShell.test.ts` 加 C-S6 抛错桩用例；`retired_mobile_auto_task_plugin_lock.rs` 断言反转 |
| D4 文档 | **完成** | ADR 0046、`AGENTS.md` §6 例外条款、移动端 code-map 防回接锁索引、CHANGELOG 双语 |

### 6.1 与 spec 原文的三处偏离（均为改判，非遗漏）

| 项 | spec 原文 | 实际落地 | 理由 |
| --- | --- | --- | --- |
| A3 旧交互补回 | 「逐字复刻」`ScanPanel` / `PairingInput` / `SessionCard` / `PresetTaskCard` | 功能等价面**就地实现**，未保留同名组件：扫码 = `QrScanner.vue`、配对输入在 `DevicesSection.vue` 内、会话增删停在 `SessionsSection.vue` 内、工具箱双页签 = `TaskHistoryTab` + `ScheduledJobsTab` | 旧组件的宿主副本在 D1 已作为孤儿删除；组件名不承载行为契约，行为由各自测试钉住 |
| A4 清除所有数据 | 归 terminal-session | **改判为壳的危险区动作**（`src/composables/useClearAllData.ts` + `ShellSettingsScreen` 危险区） | 该动作的清理对象含设备入场凭据（认证中心托管）与宿主连接态；按 §8 凭据零过境 + ADR 0031/0033，**插件不得持有或擦除凭据**。放进应用设置页即 §5.1 越线 |
| A5 真源位置 | 业务设置进 `context.storage`，宿主投影改读插件真源 | 宿主侧**通用 KV 桥**（`readAllSettings` / `writeSetting`）+ 已知键写穿响应式单例 | `context.storage` 是插件命名空间隔离存储，宿主投影改读它需新增跨命名空间读取面，反而扩大宿主对插件存储的耦合。KV 桥让宿主**不持有任何业务形状**（更贴 §5.1 B3），且写穿保证既有宿主消费者读到真值。已在 ADR 0046 D6 记账 |

**未纳入自动化门禁的项**：
- `bedcode-mobile/src-tauri` 全量 `cargo test` **被在途的票 02（wasm-core WIT 分片）阻塞**
  （`error: interface or world 'core' does not exist`，与本票无关，已用 stash 对照确认）。
  本票改动的 `src-tauri` 侧只有 `plugin/loader.rs` 的退役位闸门与锁测试断言，
  编译与断言正确性待在票 02 合流后复核。
- 真机 / 浏览器核验未跑（无设备）。