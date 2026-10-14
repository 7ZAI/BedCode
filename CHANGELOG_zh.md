# 更新日志

本文件记录本项目所有值得关注的变更。

格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，
版本遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

> 本文档为中文版本；英文版见 [`CHANGELOG.md`](./CHANGELOG.md)（GitHub Release 流程读取该文件）。

## [未发布]


#### 移动端：停止应用即作废其运行期 store（再次启动为全新状态）

- **修复（宿主壳 / 插件前端）**：停止应用（多任务上滑 / × / 全部关闭 / 详情「停用」）后再打开，
  应用的 Pinia store 会延续上一次运行期的状态（例如 terminal-session 仍停在旧会话 / 旧页签）。
  成因是插件前端模块经动态 import 装载、URL 不变 → ESM 模块缓存命中，模块顶层的 `defineStore`
  只求值一次，`useStore()` 每次从宿主 Pinia 取回同一实例；而停用只回收了注册项与后端订阅。
  现在宿主按「应用运行期」（activate..deactivate）隔离 store：`defineStore` 经运行期作用域包装，
  `useStore()` 恒落在该插件当前运行期的 Pinia 上；停用时逐个 `$dispose()` 并丢弃作用域，再次
  启动是全新 store（初始 state）。插件间同名 store id 因作用域隔离也不再冲突。
- **契约（插件开发者）**：store 必须在插件模块顶层定义（异步 chunk 内会回退宿主 Pinia 并告警）；
  模块级可变单例状态须在 `deactivate` 中清空——宿主不重新求值插件模块（SDK README 已写明）。
- **卸载收口**：`removeApp` 先结束运行期（前端对称拆解 + 后端停用）再删包，卸载后不再残留运行面
  与 store 直到页面重载。
- 新增 9 条作用域契约测试 + 2 条 loader 集成测试（含「模块缓存命中仍得到新 store」）；移动端
  89 文件 / 1055 用例全绿，`vue-tsc` 与根 ESLint 0 error。

#### 移动端：运行页切页改「同屏一页」——停靠面板屏外裁切 + 入场过渡

- **修复（terminal-session 运行面）**：底部四页签用左右滑动切换时曾表现为「页面重叠、切不干净」。
  两个成因：停靠面板停在 `translateX(±24%)` 且 `opacity: 0.55`，而面板自身没有背景、容器也不裁切——
  相邻页从当前页背后透出来并溢出到头部与导航上；首次挂载的面板又直接落在最终位置，浏览器没有
  「停靠 → 居中」的变化可过渡（表现为硬切）。现在停靠面板完全停在屏外（`translateX(±100%)` +
  `visibility: hidden`），面板容器以 `overflow: hidden` 兑现裁切；新挂载面板先按入场侧渲染，
  强制一次布局读取后再切居中，滑动才真正跑起来。停靠与取邻规则抽成纯函数（`panelDock.ts`），
  15 条契约测试钉住规则并对 CSS / 模板加形态锁，半屏叠放不得回接。
- **说明**：切页仍是抬手即切（不跟手）。让 SDK 共享的 `useSwipeTabs` 上报位移属影响所有插件的契约变更，
  本次有意不做。

#### 移动端：宿主壳第三轮修订（多任务卡片轨道 / 主屏多任务入口 / 退出摘运行屏）

- **多任务卡片轨道**：卡片改为并排成行而不是叠放——同屏一张完整可见，屏宽放不下时邻卡只在边缘露出半张
  （是裁切，不是叠压）。轨道按「卡宽 + 间距 × 序号」平移（几何真源在 `shell.css`），拖拽跟手、松手吸附。
  点任意卡片即进入该应用（对齐 Android recents），独立的「进入」按钮退役；× 关闭仍是独立控件，命中区 44px。
- **手势健壮性**：越过轴锁确认拖拽后捕获指针（此前快速划出轨道会把卡片卡在偏移位）；距屏幕左右边缘 24px
  内起手整段让位给系统返回手势；拖拽抬手后浏览器补的那次 click 被吞掉，上滑关闭不再顺手打开刚关掉的应用。
- **无障碍**：卡片位置改为可播报（「第 N 张，共 M 张」），多任务空态补「回到主屏」主动作，不再只剩页头返回键。
- **退出修复**：停用 / 卸载应用时同步把它的运行屏从壳屏幕栈里摘掉（`closeAppRun`），退出应用回到主屏，
  不再落到「应用暂无可用界面」空态。
- **可达性**：主屏「正在运行」区补多任务入口，不进运行页也能打开应用切换器。
- 移动端全量：87 文件 / 1043 用例全绿；`vue-tsc` 与 ESLint 零告警。

#### 移动端：修复工具箱横滑被「可滚动容器」误判吞掉

- **修复（terminal-session 工具箱）**：自动任务工具箱（任务记录 ↔ 定时任务）内的左右滑动曾被整体吞掉——
  只要任一任务行把列表撑出少量横向溢出（无空格长串如日志片段 / URL / 错误码、或 DPR 亚像素舍入），
  `insideHorizontallyScrollable` 判定就会把这类「纵向滚动容器」当作横向可滚动（规范下 overflow-y 可滚动时
  overflow-x 必计算为 auto），导致整轮手势 skip：内部页签切不动、外部主页面翻不动，滑动表现为失效，
  而两侧按设计半透明停靠的邻面板静止不消，观感就是「页面重叠显示」。新判据排除真正纵向滚动的容器，
  仅把 ≥25px 的实际横向溢出当作横滚区（真实 Chromium 布局验证：日志长串的 6px 溢出不再杀掉横滑）。
  新增 7 条契约测试钉住判定（`wasm-apps/terminal-session/src/task/swipe-zone.ts`）；移动端全量
  86 文件 / 1005 用例全绿。

#### 移动端：宿主壳第二轮修订（导航 / 多任务叠放 / 设置归属 / 自定义壁纸）

- **导航**：底部系统条（`ShellSystemBar`）整条退役——常驻导航条占掉一截应用可视高度，且与 Android 系统
  手势条重复。导航入口改为：各非根屏页头自持返回；应用运行页顶栏左侧显示应用图标与名称，右侧胶囊提供
  菜单 / 多任务 / 回主屏（`···` / 卡片 / home 图标；原「退出应用」小圆圈语义删除）。
- **主屏**：删除「平台自带」「由应用提供」「最近使用在前」等方案期旁白；快捷卡片改为横向 scroll-snap 轨道
  （定宽卡片，一次滑动一张）；应用管理入口从文字 chip 改为 44px 图标按钮（带可访问名）。
- **多任务**：卡片轨道改为固定宽高居中叠放（等差偏移 + 线性缩放），前层完整可操作、后层只露边；
  横向滑动与「上一个 / 下一个」按钮切前层（拖拽非唯一路径，WCAG 2.2），点后层卡片提到前层，上滑关闭，
  减动效偏好下过渡归零。
- **设置归属**：危险操作（清除所有数据）从平台设置一级页迁入「关于 WasmApp」；「连接」「认证」两项从
  平台设置删除（连接与凭据归各应用自持，不是宿主面）；设置二级页页头补顶部安全区（状态栏不再与页面
  贴死）、底部补导航栏安全区；应用贡献的设置项没有处理器时落应用详情（不留死点击）。
- **自定义壁纸**：背景分组新增「从相册选择」——宿主命令 `shell_import_image` 选图并拷进
  `$APPDATA/shell-assets/<namespace>/`（单槽位，旧图自动清理；Android 的 SAF `content://` 经 SafIo
  中转），壳只存路径偏好并经 asset protocol 渲染；支持更换 / 移除与回落到内置渐变档。
- 测试：移动端 85 文件 / 998 用例全绿（新增运行页胶囊 / 叠放切卡 / 自定义壁纸偏好用例）；移动宿主
  `cargo test` 全绿（含 `system/shell_ui.rs` 9 条单测）；根 `eslint .` 0 error；`vue-tsc` 0 error。
- 未跑（需人工 / 真机）：自定义壁纸 SAF 选图实机核验、完整 wasm 构建。WIT / ABI 零改动。

#### 移动端：宿主壳改造成 Android 式平台（主屏 / 多任务 / 通知栏 / 壁纸）

- **主屏**对齐桌面设备壳布局：壁纸层 → 正在运行卡片带（MRU，点切换、点「停止」停实例）→ 应用贡献的
  快捷卡片 → 应用宫格（全部已安装应用、状态点、待审批角标）→ 系统宫格（应用管理 / 权限管理 / 平台设置）。
- **导航**：底部页签（`ShellTabbar`）退役，改由底部**系统条**（`ShellSystemBar`，Android 导航条语义：
  返回 / 主屏 / 多任务）承担。主屏键 = 回主屏，上滑 = 呼出多任务；多任务键是与上滑手势等效的显式路径
  （WCAG 2.2 Dragging Movements：拖拽不得是唯一路径）。应用管理与平台设置补顶部返回行，系统入口改为
  入栈（可返回），不再重置导航栈。
- **多任务**（`ShellSwitcherScreen`）改为 Android 卡片轨道（原生 scroll-snap 横向滚动）：运行中应用在前
  （按最近使用排序），最近用过但已停止的在后；点卡片进入，`×` 或上滑关闭，「全部关闭」停掉全部运行中
  实例并清空最近列表。卡片只呈现平台知道的事实（运行态 / 权限项数 / 应用 id），不伪造应用内截图。
- **通知栏**（`ShellNotificationShade` + `useShellNotifications` + `useShellHostEvents`）：独立面，自顶部
  滑下；入口 = 壳状态条（系统状态栏占位时不渲染）与主屏铃铛。内容：待处理（待审批应用，由应用状态派生，
  批准后自然消失）+ 平台通知（应用启动/停止/审批/安装/卸载/启动失败，以及引擎事件 `ws_connected` /
  `ws_reconnected` / `ws_disconnected` / `ws_reauth_rejected`，最后一项为驻留），支持单条关闭与「清除全部」
  （驻留项保留）。v1 不做快捷开关：能真驱动的平台开关都在 `@/composables/**`，壳按 L7 不可直连，做假 toggle
  违反诚实原则。
- **审批流**：`needsApproval` 不再被拍平成「已停用」——它是壳内一等状态（`ShellAppState: 'approval'`），
  落到主屏角标、权限管理待审批分区与应用详情的「批准并启动」。`ShellAppSource` 新增 `approve()`；
  `installFromLocalPackage()` 改为返回安装后的应用 id（用户取消返回 `null`），使生命周期通知能点名应用。
- **壁纸**：设置新增「背景」分组——四档 token 派生渐变（无 / 极光 / 沙丘 / 礁湖）+ 0–70% 遮罩强度滑杆，
  持久化在壳本地存储（不写宿主 AppConfig：引擎面不加壳词汇）。自定义图片壁纸需宿主命令（SAF 选图 + 拷到
  `$APPDATA`），本批次刻意不做。
- 测试：新增六套壳测试（通知栏 / 偏好 / 多任务 / 系统条 / 宿主事件桥 / 应用动作），旧 UI 退役锁改为正面
  钉住新的壳面；移动端全量 84 文件 / 968 用例全绿。
- 未改动：Rust 后端、WIT / ABI、wasm-app 源码。`packages/plugin-sdk-mobile/dev-shell` 仍带旧壳副本
  （仅开发预览用），另行对齐。

#### 移动端：前端切换回新壳（feature/mobile-shell-host）

- 移动端前端回到新壳线：宿主前端（`bedcode-mobile/src/**`）、三个 wasm-app 前端
  （terminal-session / ai-chatbox / file-transfer）与 SDK TS 契约
  （`packages/plugin-sdk-mobile/src/{types,index}.ts`，`registerSurface` 族）自
  `feature/mobile-shell-host` 整面恢复，仅属旧前端的 views/components 删除。在同一未发布周期内，
  本条目取代下方「旧前端适配」条目。
- 保留必要的适配：`useMobileSettings` 继续调用当前 `*_mobile` 数据库命令（后端旧名已退役）；
  terminal-session `task/activate.ts` 补上分支中缺失声明的 `supportedAgents` 缓存（原为 WIP 编译错误）。
- `dev-shell` 补齐宿主 `ShellApp.iconUrl` 契约（图片图标渲染），保持 SDK 契约漂移锁绿。
- 未改动：Rust 后端（含 peer 收拢重构）、SDK rust 侧、WIT / ABI。

#### 移动端：旧前端适配当前新后端（前端回退 92222e8b6，后端保持不变）

- **背景**：移动端前端（宿主 UI + 三个 wasm-app 前端）已回退至 `92222e8b6`（旧宿主退役前最后完整态），
  Rust 后端 / SDK rust 契约 / WIT / ABI 保持当前线。本条目 = 让回退后的旧前端在当前后端上
  编译通过、可运行的适配层。
- **SDK 类型面回退（仅类型）**：`packages/plugin-sdk-mobile/src/types.ts` 恢复旧 `UIRegistry`
  （`registerToolboxPage` / `registerNavTab` / `registerTerminalToolbarItem` /
  `registerTerminalView` / `registerSettingsSection`）与旧描述符类型；`index.ts` 重新导出。
  `PluginState` 联合类型**有意保留 camelCase**：宿主 Rust 现仍序列化 camelCase
  （`#[serde(rename_all = "camelCase")]`），若把类型一并回退成 PascalCase 会编译过但运行永不命中。
- **状态比较适配 camelCase**：`src/plugin/loader.ts`、`src/shell/adapters/pluginAppSource.ts`、
  `src/views/PluginView.vue` 的状态字面量改为 `'activated'` / `'activating'` / `'degraded'` /
  `'needsApproval'` …，与线上 payload 对齐。
- **命令面适配**：`useMobileSettings.ts` 改调 `set_db_setting_mobile` / `get_all_db_settings_mobile`
  （旧名已在能力域集中化时退役）；`useMdnsAdvertiser.ts` 移除广播调用（移动端广播面已退役，
  退役锁 `retired_mobile_mdns_advertise_lock` 钉死）。
- **事件桥验证**：`ws_pairing_request` / `ws_pairing_verified` 由 terminal-session 插件经 host-events
  发射并逐字转发；`ws_connecting` / `ws_disconnected` / `ws_event_channel_ready` /
  `ws_link_crypto_pin` 仍由宿主直接发射——旧前端监听零改动。
- **清理回退残留**：`wasm-apps/terminal-session/src/{app,host,settings}` 与仅存在于新壳时代的
  测试文件（`pluginContextShell` / `retiredHostUIRetirementLocks` / `shellSettingsScreen` /
  `useShellAppsSurface` / `connectionEvents` / `useClearAllData`）为非完整回退的遗留（92222e8b6 无，
  且引用已退役的 `registerSurface` 族 API），已删除；vitest include 同步回退为旧套件形态。
- **新增回归测试**：`useMobileSettings.test.ts` 断言改名后的持久化命令（`set_db_setting_mobile`、
  `mobile.` key 前缀、旧名 `set_db_setting` 不得复活、失败隔离）；`useMdnsAdvertiser.test.ts`
  断言零后端调用（广播退役）与保留的状态机形状。`useMdnsDiscovery.test.ts` mock `frontendLogger`，
  消除异步日志攒批 flush 对 `not.toHaveBeenCalled` 断言的 flaky。

#### 桌面端：插件状态按线上 camelCase 识别（修好应用中心启停开关与整条状态判定链）

- **根因**：插件状态真源是共享内核 crate `packages/bedcode-wasm-core` 的 `PluginState`
  （`#[serde(tag = "state", rename_all = "camelCase")]`，线上 `{state:'activated'}`），
  但桌面端 TS 类型与所有状态判断都按 PascalCase（`'Activated'`）写。接口能编译、测试用
  同款 PascalCase 夹具自洽，真机上整条链却是死代码：`isRunning` 永不命中 → 应用中心
  启停开关恒显示未启用态（点击启用后样式不变）、已启用/未启用分区恒错位、降级/错误/
  待授权徽标全错、贡献面（侧边栏/设置分组）永不生效、前端加载器的 activated/degraded
  门禁也从未放行。
- **修复**：`src/plugin/types.ts` 与 SDK 双写副本 `packages/plugin-sdk-desktop/src/types.ts`
  改为线上 camelCase；`contributionKinds` / `loader` / `registry` / `PluginConfigView` /
  `PluginDetailView` 的判定与内部登记值同步；测试夹具与断言改用线上值。
- **回归锁**：新增 `src/__tests__/plugin/pluginStateVocabulary.test.ts`——解析 wasm-core
  `manifest.rs` 真源变体并与两份 TS 副本逐字比对（L1/L2）、生产消费面禁回接 PascalCase
  （L3）、逐态判定正反例且不兼容旧值（L4）；`plugin-flow` 集成测试直接断言开关样式变量
  `--toggle-on`（启用 1 / 未启用 0）随状态切换，钉住用户可见症状。
- **未在本次改动内**：桌面 SDK Rust 副本 `packages/plugin-sdk-desktop/rust/src/types.rs`
  的 `PluginState` 仍是旧 `tag="state", content="error"` PascalCase 形状（无仓库内消费者，
  不在宿主→前端线上路径），留作后续单独对齐。
- **验证**：桌面 `pnpm run test:run` 1627/1628（唯一失败是 `terminalPreview.test.ts` 的
  resize 重试用例：该目录本次零改动、隔离运行也复现，与改动面无关）；`vue-tsc --noEmit`
  干净；根 `eslint .` 0 error（110 warning）；变异自检：`isRunning` 写回 PascalCase →
  词汇锁 L3/L4 + plugin-flow 开关样式断言转红，TS 副本写回 PascalCase → L2 转红。

#### 移动端宿主壳：修好插件状态 / i18n / 图标 / 主题链路，并做一轮壳内页面收口

逐页走查 Android dev 构建（adb 截图 + 视觉审查）发现。以下每条都是真机复现的缺陷，不是风格偏好。

- **插件状态从未被识别**：共享 `PluginState` 序列化为 camelCase（`{state:'activated'}`），而移动 SDK 类型与壳适配器按 PascalCase（`'Activated'`）分支——应用一律被读成未启动：应用页「0 个运行中」、多任务永远空、活着的应用显示「未启动」，插件加载器里的 `activating` / `degraded` 门禁也是死代码。移动 SDK 类型、`pluginAppSource.mapState`、`PluginLoader` 已改用线上值；桌面 SDK 类型仍写 PascalCase，疑似同源缺陷（已上报，未在本次修改）。
- **插件 i18n 回退成 key 字面量**：`registerMessages` 把嵌套消息树前缀成「键名带点、值是对象」的形式（`'com.x.app' → {title}`），vue-i18n 按点号寻路径时在第二段就断了——terminal-session 整个运行面显示 `com.bedcode.terminal-session.app.title`。现改为合并前展平成叶子键，嵌套与扁平两种插件包都能命中。
- **图片路径图标被当文本渲染**：清单里 `icon: "icon.svg"` 是相对插件目录的路径，壳的 `iconKindOf` 却当成 emoji，于是每个应用图标显示 `icon.svg` 字面量。现经共享的 `toPluginAssetUrl`（新增 `src/plugin/assets.ts`，loader 同用）解析为 asset URL 并以 `<img>` 渲染，失败回退首字母；URL 解析放在插件适配器侧，壳的「不得直接 import `@tauri-apps/api/core`」锁得以保持。
- **浅色主题没覆盖到所有渲染根**：浅色 token 原先只挂在 `.mobile-ui` / `.mobile-app` 上，不带这两个类的根节点掉回 `:root` 的深色默认值——5 个设置二级页与 Teleport 到 body 的胶囊菜单在浅色主题下整片发黑。现改为挂 `html:not(.dark)`；流式字号变量移到 `:root`（它只与视口宽度有关，与主题无关），`App.vue` 恢复 `mobile-app mobile-ui` 祖先类。
- **状态栏重复**：壳在真实 Android 状态栏下方又画了一条自己的时间/信号/电量。现在只在顶部安全区为 0（无系统状态栏）时才画。
- **冷启动可能一直空**：壳只在挂载时拉一次应用清单，撞上「后端扫描未就绪」就停在「0 个应用 / 尚未安装 WASM 应用」。现按空清单轮询重试（250ms × 20s，与插件扫描同节奏）。
- **重开已运行应用丢失运行面**：`launch()` 对 `state === 'running'` 早退，但后端实例跨 WebView 重载存活、前端模块不存活——重载后打开应用只看到占位屏。现始终走数据源的幂等 launch 路径。
- **壳内页面收口**：应用页统计改为单行指标条（不再三张等宽卡片；零值用中性色而非绿色）；权限总览与应用详情补齐所有已声明权限的展示词表（新增 `auth` / `mdns` / `peer` / `ws:client` / `fs:pick` / `fileservice` / `transfer` / `broadcast` / `system:open` / `ui:*` / `notify` 的中英双语文案）并新增「设备与链路」分组；多任务空态说明自己并给出回首页出口（「上滑停止」提示只在真有卡片时出现）；详情页危险操作改描边按钮（淡粉底读起来像禁用态）；仅开发期可见的「演示：运行时授权弹窗」按 dev 构建门控；首页快捷卡片空态不再自称「尚未安装应用」；出站策略区的空态不再复用「暂无授权记录」；terminal-session 快捷卡片在 `0` 旁补状态说明。
- **验证**：移动 `vitest run` 78 文件 / 896 用例全绿；`vue-tsc --noEmit` 干净；根 `eslint .` 0 error（97 条既有 warning）；真机重走首页 / 应用 / 我的 / 权限总览 / 应用详情 / 多任务 / 各应用运行面 / 5 个设置二级页 / 胶囊菜单。

#### core 收窄为机制必需契约；功能级交集归能力域；端扩展归宿主（ADR 0052，票 04）

WIT 包 **minor bump `0.1.0 → 0.2.0`**（ADR 0051 D4：interface 增删 / 签名变 = minor；file-transfer 双端单产物要求双端同步 bump，ADR 0051 D3）。规格与逐票记录：`.scratch/2026-10-13-domain-owned-contracts/`；决策：`docs/adr/0052-core-mechanism-only-and-domain-owned-contracts.md`。

- **`core.wit` 只留机制（12 个 interface：8 import + 4 export）**：`host-mdns` / `host-peer` / `host-http`（出站）/ `host-websocket`（客户端）迁出到各能力域分片（`discovery-engine/wit/mdns.wit`、`server-peer-net/wit/peer.wit`、`server-http/wit/http.wit`、新建 `ws-client-engine/wit/ws.wit`，裁决 Q4=A），由双端 `compose.json` 的 caps 直接消费。内核唯一绑定仍是 `core_binding` 绑 `wit/core.wit`；core ↔ 分片的逐字重复副本消失。
- **功能级统一（R2/D3）**：同一功能不再被两端形状切碎。① 通知：桌面 `host-events-desktop.notify(title, body)` 与移动 `host-notify.notify(title, body, options-json)` 合并为共享 `host-notify` 契约，住新建轻量契约房 `packages/bedcode-notify-engine/wit/notify.wit`（双端消费同一份）；平台不支持的选项（桌面 vibrate/sound）按契约文档**静默忽略**，非法 JSON 显性报错。② 目录选择：core `pick-folder` 与桌面扩展 `pick-folders` 合并为 core `host-platform.pick-folders`（`string[]` JSON，空数组=取消；移动 SAF 返回 0/1 个树 URI）。③ 移动通知附加 4 函数（权限查询/请求、震动、提示音）改名 `host-notify-mobile`，留移动 cap 分片。
- **端扩展归宿主（D4）**：`host-http-endpoint` / `host-websocket-server` 的声明住桌面 `wit-src` cap（world `cap-http-endpoint` / `cap-ws-server`），其 `impl Host` 与端专属引擎面迁 `bedcode-desktop/src-tauri/src/plugin/{http,ws}.rs`；能力域 crate 端中立。
- **连带变化**：`host-events-desktop` 整 interface 退役；桌面 `plugin/notify.rs` 实现共享契约（无权限门，与旧行为逐字一致），SDK 新增 `HostNotify`/`NotifyOptions`（桌面）与 `HostNotifyMobile`（移动）；移动 file-transfer manifest 补 `fs:pick`（原先缺失、会被内核权限门拒绝）。
- **产物全量重建**（桌面 4 插件、移动 3 插件含 file-transfer 共享产物、wasm-core 夹具）；旧 `0.1.0` 产物在实例化期被 linker 点名包版本拒绝，并附 `stale_artifact_rebuild_hint` 重建指引（证据：`.scratch/2026-10-13-domain-owned-contracts/evidence/`）。
- **验证**：`check:wit` 绿；wasm-core `sliced_interfaces` / `binding_surface` 锁绿；桌面 `cargo test --lib` 157/157 + `http_e2e` 1/1 · `ws_e2e` 6/6 · `system_component_test` 10/10 · `owner_e2e` 5/5 · `pty_wiring` 6/6 · `capabilities_lock` 9/9 · `sdk_e2e` 3/3 · `component_e2e` 2/2；移动组合锁 3/3 · 纯净锁 7/7 · 共享产物探针 4/4；file-transfer 应用 83/83；file-transfer 共享核 76/76。

#### 宿主端事实搬出内核：平台标识 / 入站服务器端口由各端宿主注入

收口 2026-10-13 单形态裁决的最后余量（ADR 0049 D1：「桌面这样写、移动那样写就不是机制」）——`packages/bedcode-wasm-core` 内最后两处编译期端分支消失。

- **新端口词汇**：`host_api::ports::HostFactsPort`（`platform_id` / `inbound_server_port`）+ `WasmHostContext::set_host_facts`，须在任何插件 `activate()` 之前注入（共享 file-transfer 产物在激活期探测 `host-platform.platform()`）。缺省 `UnimplementedHostFacts` 为 fail-visible：平台取契约外 `"unknown"`（guest 显性拒绝）、端口返回 `Err` 而非回退配置默认值。
- **宿主实现**：桌面 `bedcode-desktop/src-tauri/src/plugin/host_facts.rs`（平台 = `desktop`；`network.port` 读 `ServerSupervisor` 运行态），经 `PluginHost::new` 注入；移动 `bedcode-mobile/src-tauri/src/plugin/host_ports.rs`（`HostPorts` 实现该端口；平台 = `mobile`，无入站服务器 → `Err`）。`host_api::platform` / `host_api::config` 里两处 `#[cfg(feature = "desktop-assembly")]` 分支（含 `"desktop"` / `"mobile"` 字面量）删除——内核不再认识任一端名。
- **依赖边清理**：wasm-core 对 `bedcode-server-core` 的生产依赖降为 dev-dependency（仅 `host_harness` 测试起服）；桌面 `crate_boundary_lock` 对应生产必填边摘除（dev 边仍登记）。
- **测试基建保持双消费面、共享符号单点化**：`test_support` 保留桌面（`common`）与移动（`bare`，fork 期历史名）两个消费面——运行时壳与 core 绑定早已单份；共享符号（`TEST_PLUGIN_ID` / `plugin_raw_store`）上提父模块单点定义。`.scratch/2026-10-13-runtime-single-form/spec.md` 第 8 条按实修正，ADR 0049 附收口补录。
- 行为等价（取值与报错文案不变）。验证：wasm-core `cargo test --lib` 576/576；移动 `cargo test` 全绿（含共享产物探针实际跑移动 `HostPorts`）；桌面 `cargo test --lib` 155/155 + `crate_boundary_lock` 9/9 + `system_component_test` 10/10；移动 purity / fork 锁 7/7 + 6/6。

#### 平台选择器改为宿主装配端口（修复 Android 构建；ADR 0052 D1）

ADR 0052 目录选择统一时把桌面对话框实现留在了内核（`host_api/platform.rs`），而
`FileDialogBuilder::pick_folders` 是 tauri-plugin-dialog 的 `#[cfg(desktop)]` 专属 API——
`bedcode-mobile` 的 `aarch64-linux-android` 构建直接报 ``no method named `pick_folders` ``。
按 ADR 0052 D1「差异应该在各自的宿主侧」改为窄端口：

- **内核只留共享机制**（`fs:pick` 准入门 → 选择器 → `authorize_picked` 结果门 → JSON 形状，口径不变），新增 `host_api::ports::PlatformPickerPort` + `UnimplementedPlatformPicker`（fail-visible 缺省）+ `EnginePortsPlatformPicker`（移动适配器）；`WasmHostContext` 增加选择器槽（`set_platform_picker`，与 `host_facts` 同款两阶段注入）。
- **桌面**：新增 `bedcode-desktop/src-tauri/src/plugin/platform_picker.rs`（`DesktopPlatformPicker`，原生对话框，实现自内核逐字搬出），经 `PluginHost::new` 新增的 `platform_picker` 参数注入——在实例化 / 激活之前，与 `HostFactsPort` 同装配点；无头 / 测试传 `None`。
- **移动**：`WasmHostContext::new_with_ports` 把选择器接到既有 SAF 引擎端口（`platform_pick_files`；`platform_pick_shared_directory` → 0/1 个树 URI）——移动侧零改动；AppHandle 在装配期捕获。
- **验证**：`cargo check -p bedcode-mobile --target aarch64-linux-android --lib` 绿；wasm-core `cargo test --lib` 579/579（platform 7/7，含新增的端口委派 + 授权门用例）；桌面 `cargo check --tests` 绿；移动 `cargo test --lib` 226/226 + purity / composition / fork 锁 16/16。

#### WIT 契约搬上包 semver，整数 ABI 退役（ADR 0051）

契约版本管理从「全局单一整数」搬上 wire：WIT 改为 `package bedcode:plugin@0.Y.Z;`，
import / export 名自带版本；整数 `ABI_VERSION` 与 `interface abi` **同票退役**（用户裁决
2026-10-13：wit 包版本独立从 `0` 起算、不做 1:1 派生、不考虑旧产物兼容——项目处研发起步
期、无用户）。规格与逐票记录：`.scratch/2026-10-13-wit-package-semver/spec.md` +
`issues/01…06`。

- **版本上 wire 是破坏性改名，不是多一份兼容余地**：组件 linker 对 import 名做**含版本**
  精确匹配——版本相等照常加载，不等即实例化期被拒，且失败文案自带包名 + 版本（诊断改由
  wasmtime 直接报出，撤掉自造解释层）。
- **整数 ABI 退役（R6）**：现有 5 项职能分别归位——「产物新于宿主」拒绝与 upgrade 提示
  由 linker 的包名匹配接管（本已冗余）；向后兼容加载**不接管**，且在现行 bump 政策下
  **已是空壳**（任何契约变更都 bump ⇒ 不存在「纯加法且不 bump」的产物）；
  `abi-form.form = 1` 与 version 正交、**原样保留**；逐版语义台账迁 CHANGELOG（见下）；
  `SHARED_ABI_VERSION` 随之整体退役。
- **版本真源单点化（R7）**：版本值 = 仓根唯一常量，双端 `compose.json` 各自引用；
  `compose-wit.mjs` 整行透传、cap 分片逐字拷贝，**脚本零改动**。**双端版本对齐仅在「同一份
  产物跨双端加载」时成立**（file-transfer 双端单产物，ADR 0050）；其余场景双端各自 bump
  ——**不是**「双端必须同版本」的全局约束。
- **新 bump 政策与起算值**（票 01 实施时确认）：任何 WIT 变更**含函数级追加** → patch
  （`0.1.0` → `0.1.1`）；interface 增删 / 签名变 → minor（`0.1.x` → `0.2.0`）；双端起算同为
  `0.1.0`。理由：组件模型下任一契约变更都要求 guest 重建，「加法免 bump」不存在——
  patch / minor 的区分只是让**破坏面大小**在版本号上可见。
- **记账去处**：双端 `abi.rs` 的逐版语义迁入下方双语台账（WIT 源有意不复制历史）；文件
  随后随退役面一并删除。
- **双端偏离**：ADR 0018 维持不撤销——移动序列不跟演桌面独有破坏性变更。

**票 02（契约层）已落地**：版本真源 = 仓根 `contracts/bedcode-plugin-package.rs`（Rust 常量 `PACKAGE_NAME` / `PACKAGE_VERSION` / `PACKAGE`）——全仓唯一手写版本处。双端 `compose.json` 的 `package` 退化为**只声明包名**（版本由 `compose-wit.mjs` 注入生成的 `bedcode.wit`；清单包名与真源 `name` 漂移即抛）；10 个 WIT 源文件全部带 `package bedcode:plugin@0.1.0;`（cap 分片逐字拷贝，版本随之进生成物），双端已重拼（`pnpm run check:wit` 9/9 绿）。新锁 `bedcode-desktop/src-tauri/tests/wit_package_version_lock.rs` 断言 sha256 漂移锁抓不到的那条不变量——源文件版本与真源不一致时，重拼后整棵树仍自洽（漂移锁全绿），只在实例化期才炸。变异自检 3/3。编译面不变且全绿（wasm-core / 双端 SDK / 双端宿主）：版本化改的是 linker 的 import 名匹配，不是代码生成。**运行期按名匹配点（票 03）与整数 ABI 退役（票 04）尚未落地，故端到端实例化路径当前为红——属票序在途范围，非回归。**

**票 03（运行期按名匹配点）已落地**：先做了一次 wasmtime 48 + wit-parser 0.256 实测（临时探针跑完即删），把「按名匹配点」的形态定死——组件**导出实例**的键 = `ns:pkg/iface@version`（**版本在末尾**，无版本写法对版本化产物恒 None）；实例内函数名按**平名**查找，`iface#func` 连 `ItemName` 语法都不合法；`ItemName` 字面量把版本写中间会解析失败。于是正解是**末尾追加 `@<version>` + 单一派生入口**：新增 `packages/bedcode-wasm-core/src/contract.rs`（`export_item(iface, func)` / `interface_path(iface)`，`OnceLock` 池保证 `&'static [&'static str]` 形态不变），内核 30 余个探测点（`core_binding` 2 · `component` 4 · `owner` 16 · `capability` 8）全部改为只传接口名 + 函数名，**零散落版本字面量**。真源由 JSON 改 Rust 常量文件 `contracts/bedcode-plugin-package.rs`——能力域 crate 的 `inventory` 静态自报要求编译期常量，JSON 只能运行期解析；能力域 `routing.rs` 的 const 清单由 `wit_package_version_lock.rs` 新增判据钉住，内核对偶锁改为「双形态抽取 + 值层逐字比对」。验证：wasm-core `cargo test --lib` **575/575**、`discovery-engine --features common-host` **35/35**、版本锁 **5/5**、双端宿主 `cargo check` 绿；变异自检 **2/2**（能力域字面量去版本 → 版本锁 + 对偶锁双红；内核派生退回旧名 → contract 三测 + 对偶锁红；还原后 sha256 一致）。

##### 契约演进台账（整数 ABI）—— SDK `abi.rs` 删除前的迁移去处

两条独立序列（ADR 0018）：桌面 v1…v38 / 移动 v1…v21。「增量」= 既有 interface 加函数 /
加字段；「破坏」= interface 或函数删除、签名变、或行为刻意变更。

桌面：

| 版本 | 语义 | 性质 |
| --- | --- | --- |
| v1 | 初始版本（27 个 host functions + 11 个插件导出） | — |
| v2 | 4 个 `*_params` SQL host functions（消灭插件侧手写转义） | 增量 |
| v3 | 提交输入行观察扩展点（`session-input-register` + 可选导出 `on-input-submitted`，ADR 0001） | 增量 |
| v4 | 插件状态上报扩展点（`mark-plugin-error`） | 增量 |
| v5 | 通用文件服务能力（`filesrv-*` / `transfer-*` + 可选导出 `on-upload-request`） | 增量 |
| v6 | 会话创建与宿主定时器（`session-create` / `timer-register`，ADR 0003） | 增量 |
| v7 | 会话关闭（`session-close`） | 增量 |
| v8 | 生命周期契约补全：`on-startup` / `on-shutdown` 携带 `result<_, string>`，启动 / 清理失败可如实上抛（Degraded 而非静默 Activated） | 增量 |
| v9 | host-peer 原语化收缩第一阶段（ADR 0022 v2）：`dial-peer-endpoint` / `close` / `set-shared-roots` 与旧函数并存；新增 `host-mdns`（browse-only）与 `host-platform` | 增量 |
| v10 | host-peer WIT 收缩终态（桌面 13 函数定稿，ADR 0022 v3，commit `56ee094cb`） | 破坏 |
| v11 | host-peer 传输控制三原语 `pause-transfer` / `resume-transfer` / `resume-all-transfers` | 增量 |
| v12 | 总线二进制载荷（`publish-binary` / `subscribe-binary`）+ 可选导出 `events-binary` | 增量 |
| v13 | host-mdns v2：`advertise` / `stop-advertise` / `is-advertising` + 属主定向浏览事件（后为 `<owner>::mdns:found\|lost`；payload 增 `serviceType` / `browserId`） | 增量 |
| v14 | host-websocket 基础传输（客户端域 5 函数 + 服务端域 + 可选导出 `events-ws`） | 增量 |
| v15 | 密钥托管原语（`host-auth`：`get`/`set`/`delete`/`keys`，权限门 + `plugin_secrets` 持久化 + 明文不落日志） | 增量 |
| v16 | 插件私有伪终端 `host-pty`（6 函数 + `pty:exit.<owner>`，后为 `<owner>::pty:exit`；单生产者环形缓冲游标 pull；权限 `pty:spawn` / `pty:io`） | 增量 |
| v17 | 认证策略导出 `auth-policy.verify-device-token`（desktop 独有） | 增量 |
| v18 | host-auth 认证记录面（4 函数：可信设备清单 / 吊销、连接历史清单、认证设置写入；desktop 独有） | 增量 |
| v19 | host-session 配置面 + 创建动作面（`config-upsert`/`-get`/`-delete`；后 `create-with-spec`、`restart`、`remove`、`rename`、`resize`；再后 `annotate`、`connections-list`；`host-platform.wsl-distros`——均为函数级追加不 bump） | 增量 |
| v20 | host-task：宿主调度并发任务域（desktop 独有） | 增量 |
| v21 | host-session 收敛退役：删 `create`（legacy 按 configId）与 `restart`——创建编排统一走 `create-with-spec` | 破坏 |
| v22 | `host-platform.reveal-in-dir`（desktop 独有，无权限门；`system:open` 权限退役） | 增量 |
| v23 | host-session 配置写原语退役（删 `config-upsert` / `config-delete`；权限 `session:config` 退役，`config-get` 改挂 `session:read`） | 破坏 |
| v24 | 认证记录下沉（2026-09-22 用户裁定）：host-auth 记录面七函数退役，host-session 配置读取面（`config-list` / `config-get`）随主库表退役删除；保留 secret-store / biometric-* / device-token-* / link-identity-parts / auth-setting-set | 破坏 |
| v25 | host-peer 节点生命周期原语 `start-node` / `stop-node`（peer-net 属主化，内核按 id 分支退役；desktop 独有） | 增量 |
| v26 | host-crypto 宿主加密引擎原语面（AEAD aes-256-gcm / chacha20-poly1305、KDF hkdf-sha256、X25519；权限 `crypto:aead` / `crypto:asym` / `crypto:kdf`；desktop 独有，新 interface） | 增量 |
| v27 | 会话原语域整 interface 退役：`host-session` 12 函数 + 导出 `terminal-hooks` + `events.on-session-lifecycle` / `on-input-submitted`；权限 `session:write` 退役 | 破坏 |
| v28 | WebSocket 业务下沉：新增 `host-websocket.connection-context`；退役 `host-events.broadcast-sync` 与 `host-pty.spawn` 的 `hostBroadcastSessionId` 字段 | 破坏 |
| v29 | HTTP 路由代码注册下沉：新增 `host-http` 服务端域（`register-endpoint` / `unregister-endpoint`），路由以插件名为命名空间 | 增量 |
| v30 | 传输编排下沉票 1：host-peer 追加 `active-transfers` / `collect-outgoing`；`peer:transfer-event` / `peer:receive-event` 双写 | 增量 |
| v31 | 传输编排下沉票 3：退役 `resume-all-transfers`；`send-files` 收窄为一次调用 = 一会话立即发起、`concurrency` 字段退役；旧快照 topic `peer:transfer` / `peer:receive` 退役；`pull-files` 逐文件立即发起 | 破坏 |
| v32 | 认证中心显式注册 + 组合式认证原语（ADR 0031）：host-auth 追加 `auth-center-register` / `-unregister` / `auth-methods-list` / `auth-method-invoke`，fail-closed（desktop 独有） | 增量（行为破坏：旧认证中心产物不注册即全拒） |
| v33 | 认证中心持有入场签发密钥与验签（ADR 0033）：删 `device-token-issue` / `device-token-verify`，宿主 `utils/auth/jwt.rs` 退役（desktop 独有） | 破坏 |
| v34 | 生物凭证面下沉：删 `biometric-credential-bound` / `biometric-verify-signature` / `biometric-credential-bind`，宿主 `utils/auth/biometric.rs` 退役（desktop 独有） | 破坏 |
| v35 | 沿用序号（无独立 bump note）：host-websocket 服务端域 10 函数随 ADR 0039 域整面迁出期间保持形状 | — |
| v36 | 交集接口切片：`host-fs` → `host-fs-desktop`、`host-platform` → `host-platform-desktop`、`host-events` → `host-events-desktop`、`abi` → `abi-form`（desktop 独有） | 破坏 |
| v37 | 交集切片收拢：`host-websocket` → `host-websocket-server`（10 函数）、`host-http` → `host-http-endpoint`（2 函数；desktop 独有） | 破坏 |
| v38 | file-transfer 双端单产物（2026-10-13）：core `host-platform` 加 `platform`（双端同步）；桌面 `events` 扩为七函数并集；新增 binding-only `world desktop-compat` | 增量 |

移动：

| 版本 | 语义 | 性质 |
| --- | --- | --- |
| v1–v5 | 自研 ABI 演进史（组件迁移后仅作版本号序列保留） | — |
| v6 | 组件形态首个版本（语义与旧 v6 一致） | — |
| v7 | host-peer 原语化收缩第一阶段（ADR 0022 v2）：`dial-peer-endpoint` / `close` / `set-shared-roots`；新增 `host-mdns`（browse-only）与 `host-platform` | 增量 |
| v8 | host-peer WIT 收缩终态（移动 12 函数定稿，commit `56ee094cb`） | 破坏 |
| v9 | host-peer 传输控制三原语 | 增量 |
| v10 | 总线二进制载荷 + 可选导出 `events-binary`（与 dev 分支 v9 合并） | 增量 |
| v11 | host-mdns v2：三原语 + 属主定向 `<owner>::mdns:found\|lost`，payload 增 `serviceType` / `browserId` | 增量 |
| v12 | 发送编排下沉（票 06）：删 `resume-all-transfers`，`send-files` 收窄，`concurrency` 字段显性拒绝，新 topic `peer:transfer-event` | 破坏 |
| v13 | 接收编排下沉（票 07）：接口函数集不变（host-peer 仍 19 函数），变的是**事件回流契约**（新 topic `peer:receive-event`，旧 `peer:receive` 退役）。**须重编译**：协商单向，v12 产物不在加载期被拒 | 破坏 |
| v14 | WebSocket 出站连接（host-websocket 客户端域 5 函数 + `ws:client` 权限 + 两条属主私有投递通道）。**不跟演**桌面服务端域与 `connection-context` | 增量 |
| v15 | 终端订阅协议客户端迁插件（票 12）：`host-terminal-stream.forward-output` + `host-connection.primary-target`；websocket config 增 `jwt-auth` / `heartbeat-secs` / `auto-reconnect` | 增量 |
| v16 | 认证 / 配对编排下沉（票 14 阶段 B）：host-auth 新增 5 函数 + `auth` 权限（fail-closed，凭据零过境） | 增量 |
| v17 | host-terminal / terminal-hooks 整面退役（票 15 阶段 B）：删 `host-terminal`（send）与导出 `terminal-hooks`，权限 `terminal:input` 退役；`terminal:output` 保留 | 破坏 |
| v18 | 通知 / 震动 / 声音整族封装（新增移动特有域 `host-notify` 5 函数 + `notify` 权限；`host-events.notify` 收编迁入） | 破坏（引用 `host-events.notify` 的产物） |
| v19 | `host-database`（主库）整面退役（2026-10-09 双端统一机制决策）：接口与 `database:main` 权限位自双端 WIT / SDK 面移除（桌面 34→35 / 移动 18→19）；插件数据库能力 = 插件私有库（`host-plugin-database`，声明 `storage` 位） | 破坏 |
| v20 | 交集切片收拢：共享 core 收拢 `host-websocket` 客户端 5 / `host-http`（`fetch`）/ `host-fs` 交集 6；移动独有 `save-to-document` / `write-media-downloads` 拆入 `host-fs-mobile` | 破坏 |
| v21 | 共享核心 `host-platform.platform: func() -> string` 探测原语（2026-10-13，单产物，加法）；移动宿主返回 `mobile`，本端 WIT 其余面零变更（桌面 ABI 另走 v38） | 增量 |

#### wasm-core 运行时单形态：common/bare 壳合并，ABI 策略宿主注入

`packages/bedcode-wasm-core` 此前在 `common-host` / `bare-host` 两 feature 下挂两套运行时装配树。按 2026-10-13 用户裁决（`.scratch/2026-10-13-runtime-single-form/spec.md`）两壳合并为**一份运行时**（桌面形态）：`manager/runtime.rs` + `runtime/component.rs`；形态 feature 退役（桌面专属装配面收进 `desktop-assembly`：PluginHost / intercall / 认证中心）。

- **ABI 不再是 wasm-core 的事实**：`verify_abi` 的版本上限与 `abi-form` 要求经 `AbiPolicy` 由宿主装配时注入（桌面 37/38 + form=1；移动 20/21 + None）。两个 SDK crate（`bedcode-plugin-api` / `bedcode-plugin-api-mobile`）退出 wasm-core 无条件依赖；`mobile_wasm_core_purity_lock` 判据 7c 钉移动 SDK 词汇在内核零命中。
- **移动宿主收敛到统一壳**（D7 收口）：双端全等 8 域（storage / log / config / plugin-database / events / fs / bus / platform）删除移动 host_domains 副本，由内核 `core_binding` 单份实现——权限门收敛统一 `PermissionManager`（激活时授予进统一词表）、插件私有库布局统一内核布局；移动 host_domains 只留 http / peer / mdns / ws。`on_bus_message` 调用点改走统一 `on_message`；`LoadedComponentPlugin` 更名 `LoadedWasmPlugin`。
- **Linker 装配一份**：`component::add_to_linker`（core_binding + 宿主自报 `HostModule` + WASI）无条件编译；内核白名单表删除——白名单 = 纯宿主自报 `expected_host_modules`（端差异宿主组装；server 能力域 crate 绑定层由桌面宿主开启，移动只链接引擎面）。
- 锁与文档更新：`mobile_wasm_core_purity_lock`（单形态判据）、`crate_boundary_lock`（wasm-core 对 crypto-engine / discovery-engine 的边摘除）、AGENTS §5.4、ADR 0049 遗留项 1 状态。


#### file-transfer 双端单产物：一份 wasm，平台差异收敛为同 trait 双实现 + 运行期分支

`com.bedcode.file-transfer` 此前是「一个共享业务核 + 双端各一份 wasm 适配器产物」（ADR 0044）。现在双端**只构建一份 wasm**：产物源收敛到桌面端 `bedcode-desktop/wasm-apps/file-transfer/rust/`（按桌面 `world plugin` 构建），桌面与移动宿主加载**同一份组件**；移动端 `wasm-apps/file-transfer/` 只剩前端与 manifest（`rust/` 退役），构建经 `package.json` 的 `bedcode.sharedRustManifest` 声明指向共享 crate。产品差异不消除——同一端口 trait 的桌面/移动两套实现（`adapters.rs` / `adapters_mobile.rs`）+ 激活时 `host-platform.platform()` 探测端值后分支（`platform.rs`，未知取值 = 激活 fail-visible）。

契约面（双端同步，ADR 0019 流程）：

- **core.wit v38/v21**：`host-platform` 新增 `platform: func() -> string`（宿主平台标识探测，无权限门；端值由宿主回答——初版为各端装配 feature 编译期判定，后收口为注入式 `HostFactsPort`，见上方条目）。桌面 ABI 37→38、移动 20→21。
- **桌面 `events` 接口扩为七函数并集**：并入移动词汇（`on-bus-message` / `on-auth-success` / `on-disconnect` / `on-session-created` / `on-session-stopped`）——共享产物必须同时导出两端宿主要按名调用的回调；对桌面原生插件是加法（默认空实现，旧宿主按名调用不受影响）。
- **新增 binding-only `world desktop-compat`**（import `host-api-call` + `host-platform-desktop`）：移动宿主 bindgen 它为共享产物提供 fail-visible 桩（`src-tauri/src/plugin/desktop_compat.rs`，全函数显性 `Err`）——桌面分支在移动端不可达，桩只为 import 闭合。
- **协商 ABI**：共享产物经桌面 SDK 新宏 `wasm_entry_with_abi!` 自报 `SHARED_ABI_VERSION = 21`（≤ 双端宿主 AbiPolicy 下界；桌面 38 / 移动 21 均放行），锁断言受双端 `ABI_VERSION` 约束。
- **工具链**：file-transfer 改用 `wasm32-unknown-unknown` + 仓库级组件化工具（`packages/bedcode-componentize`，自移动 SDK 提升）——与移动端同一编码路径，stable 可编；桌面其余 wasm 应用维持 wasm32-wasip3。
- **行为统一（T7=B 同款）**：桌面随共享获得 `node-stopped` 中断标注 + 陈旧句柄摘除（与引擎契约一致的修正）；命令面取桌面形状（pause/resume 返回 `true` 字面量、`list-remote` 返回 `{entries, notice}`——双端前端契约实测一致）。
- 规格与逐票记录：`.scratch/2026-10-13-file-transfer-single-wasm/spec.md`；ADR 0044 状态更新（共享核阶段完成，单产物为其延续）。

#### wasm-core 单形态：机制一份、绑定归位宿主（ADR 0049）

`packages/bedcode-wasm-core` 本已是一份 crate，但内里仍挂着两套**装配树**——`common-host`（桌面）与
`bare-host`（移动），各挂一套全 world `bindgen!` 加十来个 `impl … Host`。这让内核自己知道
「有桌面这回事、有移动这回事」，还得靠 `all(common-host, bare-host)` 的 `compile_error!` 守卫挡住
两个同名 `bedcode` 生成模块冲突。现在内核只有**一份机制**、**零端绑定**（ADR 0049）。

- **单形态，机制零分叉。** Engine 构建、Store 装配、组件加载 / AOT / 实例化、事件循环、安全、
  存储、db 机制、任务引擎、消息总线全部一份双端共用。判据很简单：一段代码若「桌面这样写、移动
  那样写」，它就不是机制——应整体迁出（到共享 crate 或端宿主），而不是长出一个 `cfg`。
- **绑定层归位宿主（ADR 0049 D2）。** 内核唯一合法的 WIT 绑定只剩
  `manager/runtime/core_binding.rs`，它绑契约真源 `wit/core.wit` 的 `world core`（双端全等的 17 个
  interface），且双端是同一份。其余全部迁出：桌面全 world `bindgen!` + 12 个 `Host` impl
  （`manager/runtime/common/component.rs`，1412 行）落到
  `bedcode-desktop/src-tauri/src/plugin/{bindings.rs, <domain>.rs}`；移动全 world `bindgen!` +
  12 个 `Host` impl + 16 域 `host_impl/`（`manager/runtime/bare/component.rs`，1781 行）落到
  `bedcode-mobile/src-tauri/src/plugin/{bindings.rs, host_domains/**}`（域逻辑与对应 `Host` impl
  同处）。两端形态壳只剩 Engine / Linker / Store 生命周期与导出调用骨架——**零 `bindgen!`、
  零 `impl … Host`**。
- **删掉内核里最后 4 个 `#[cfg(test)]` 替身与 `TestGuestEvents`。** 内核不再为「测试方便」私藏一份
  宿主绑定：12 个夹具 e2e 用例迁到两端宿主的测试二进制（`bedcode-desktop/src-tauri/tests/`、
  `bedcode-mobile/src-tauri/tests/`）——它们链的是**不带 `cfg(test)`** 的 lib。内核留下的是真机制：
  能力模块白名单（`common/component.rs`）、AOT / Store / 燃料接线、trap 判定。
- **WASI 从形态差异轴降级为共享机制。** `manager/runtime/wasi.rs`（p2/p3 linker 注册 + preopen /
  `fs_auth` 过滤链）双端同源一份，**双端默认全开**，开 / 关走**运行期配置** `CoreConfig::wasi`
  而不是 cargo feature。理由：worker 类别插件（ADR 0034 L3.b）将来双端都要跑 wasm，装接线必须已经
  同源，不能等各端再重建一遍。原「桌面有 WASI / 移动没有」这条差异轴随之消失。
- **事件派发：机制留内核、调用契约面上移（ADR 0022）。** 事件循环留内核；`events` 导出的**调用
  契约**两端词汇不同，且带 `result` 返回的导出无法原生探测，故
  `manager/runtime/guest_events.rs` 定义 `GuestEventDispatcher` trait，由各端宿主实现并在装配期
  注入；缺省 `UnwiredGuestEvents` 让「漏接线」显性报错而不是静默丢事件。移动宿主因此多实现四个
  端专属生命周期回调（`on_auth_success` / `on_disconnect` / `on_session_created` /
  `on_session_stopped`）。
- **派发器晚绑定（2026-10-13 票 06 回归修复）。** `PluginHost::new` 扫描期即实例化插件，而实例原先在
  创建时**快照** dispatcher、宿主只能在 `new` 之后注入——启动期实例全持 `UnwiredGuestEvents`，总线 /
  WS 帧投递即 trap（桌面 5 个集成目标同因红）。现改为**共享槽**：`LoadedWasmPlugin` 持
  `Arc<RwLock<Arc<dyn GuestEventDispatcher>>>`（与 `WasmRuntime` 同一个槽），派发时实时读取——
  装配期注入对已建实例即时生效；未注入仍 fail-visible。
- **宿主上下文合一 + 命名中立。** `context::WasmHostContext` 与 `bare_context::WasmHostContext`
  并成一个中立类型；零消费者的 `host_api/sql_guard.rs` 垫片与已无调用点的 `host_context_registry.rs`
  按退役删除而非留垫片。`host_api/ports.rs` **刻意留内核**——它是那个统一上下文的端口词汇。
- **移动不再以别名消费 wasm-core。** `bedcode-wasm-core-mobile` package rename 别名删除，
  `bedcode-mobile/src-tauri` 直连根 crate、源码统一 `bedcode_wasm_core::`。顺带把 `fs_auth` 统一判定链
  带来的 6 个路径 / 插件白名单管理命令退役（前端 grep 实证零调用点）。
- **契约零变更。** 双端 `compose.json` 不动，生成物逐字一致（`pnpm run check:wit` 通过），桌面 ABI
  仍 **37**、移动 ABI 仍 **20**；**ADR 0018 维持不撤销**——双端契约各自独立演进，初版 spec 的
  union world 方向已否决。
- **锁。** `retired_mobile_wasm_core_fork_lock.rs` 泛化为「禁任何第二份 wasm-core 织体」（fork 目录 /
  独立包名 / rename 别名键 / fork 形态路径 / **任何第二个 `bedcode-wasm-core*` 目录**）；
  `mobile_wasm_core_purity_lock.rs` 重写为七判据，新增「端词汇全集」：内核代码行零 cap 分片 world 名、
  声明位零端名（豁免走登记表）、端 SDK 词汇封顶在唯一已知落点。内核内
  `desktop_binding_surface_must_not_return_to_wasm_core` 判据扩到**双端四壳**并**去端名化**为
  `binding_surface_must_not_return_to_wasm_core`（旧名带端名，违反 ADR 0047 D2）。每把改写的锁都做了
  变异自检：注入违背终态的形态 → 红 → `cp` 还原（核 sha256）→ 绿。

**明列的遗留项**（详见 ADR 0049「遗留项」段）：cargo feature 收敛**未做**——`common-host` /
`bare-host` 仍在，`compile_error!` 守卫仍在（其判据注释已改写成如实说明现状），内核仍有少量地方消费
双端 SDK（现已由锁封顶，防扩散）。移动 core-8 刻意**不**改用内核那份 `core_binding`：权限词表与每插件
私有库布局两处不同源，直接切会让移动侧 storage / fs / db 恒拒、或让移动的 `bus` 权限位静默失效。

#### 移动宿主加固：补注册漏命令、补 egress 闸门、清永不编译的测试与死框架

对 `bedcode-mobile/src-tauri/src` 做的一次代码组织梳理（ADR 0022 §5.1.3 宿主边界；
**未新增任何业务代码，无 ABI / WIT / wire 变更，移动端前端零改动**）。

- **修复插件下载的 egress 旁路（安全）**：`plugin_download` 的 `zip_url` 由前端传入，
  且用自建的 `reqwest::Client` 直接抓取，**全程零 egress 裁决**——是当时宿主里唯一
  绕过闸门的出站路径，而隔壁 `commands/http_proxy.rs` 就做了 `is_desktop_target` + `decide()`
  双重校验。下载即执行（zip 装完即进扫描/激活链路），放行未授权目标 = 远端代码在设备上落地。
  该路径现经 `EgressPolicy::decide` + 授权弹窗（fail-closed），并用 `egress::redirect_policy()`
  逐跳复检跳转目标，不再只管住首跳
- **修复前端已在调却漏注册的命令**：`plugin_approve`（用户安装插件的人工审批入口）带
  `#[tauri::command]`、前端 `src/plugin/commands.ts` 也有对应 `invoke` 包装，却漏在
  `generate_handler!` 之外——装插件后 UI 卡在 `NeedsApproval`，用户看到的是「命令不存在」
  而不是审批弹窗。现已注册。`get_startup_time`（声明未注册、且其 `AppStartTime` 从未
  `app.manage`，注册即 panic、零消费者）选择删除而非接线
- **清理永不编译的测试**：`src/peer_receive/tests/`（2 文件 95 行）引用的符号随接收引擎
  下沉 `bedcode-server-peer-net` 已删除，`#[cfg(test)] mod tests` 声明也不在了——而
  `tests/*.rs` 不会被 rustc 自动发现，于是静静躺着：读代码的人以为有覆盖，实际零覆盖。
  已删除，并把退役锁扩到盯住该目录，防其复活
- **移除监督任务里的 panic**：`state::get_plugin_manager()` 在插件管理器未初始化时 panic，
  而它正被 spawn 出来的连接监控任务调用——在那里 panic 一次就静默终止整个重连自愈循环。
  改走 Tauri 托管态 + `try_state` 兜底。两个 panic 版访问器（`get_plugin_manager` 与零调用的
  `get_system_info`）一并删除，`state.rs` 现在不提供任何 `expect` 版访问器
- **删掉落在移动客户端里的桌面形状服务端框架**：`connection/traits.rs` 曾携带一整套泛型 WS
  **服务端**栈（`ClientInfoTrait` / `MessageHandlerWithClientInfo` / `SendStrategy` +
  `RetrySendStrategy` / `SendInterceptor` + `Logging`/`Metrics` 拦截器 / `ResponseHandler`），
  零实现方——移动端是纯 WS 客户端（ADR 0018），按设计即死，其中一个 trait 还自带
  `#[allow(dead_code)]`「此 trait 目前未使用」。342 → 44 行
- **断开 `router` ↔ `handler` import 环**：`router.rs` re-export `handler::*`，而 `handler/*`
  又 import `router::{ClientRouteContext, ClientRouteHandler, MobileEvent}` 构成闭环。
  唯一消费方改为直接从 `crate::handler` 取
- **清理死面**：`SESSION_CONFIGS`（有读者零写者——两条已注册命令恒返回 `Ok(vec![])` /
  `Ok(None)`，正是 AGENTS.md §5.1.4 禁止的「查不到就返回空」静默降级形态）连同
  `model::data::SessionConfig`（带 `wsl_distro` / `auto_start` 的桌面 DTO，命中 B1，零消费者）；
  `device.rs` 设备 ID 注册桥（对应 `DeviceIdPlugin.kt` 在 `gen/android` 里并不存在，且注册函数
  从未进装配）；`system/constants/terminal.rs` 整个文件（终端 UI 已迁插件）；以及 ~15 个
  零引用的 `pub` 项（含一个永不写入的预授权 provider 注册表——真机制已在
  `bedcode-wasm-core`）。**注**：`dead_code` 对 `pub` 项从不报警，这批死面正是因此存活至今
- **新增三道防漂移锁**（均做过变异自检）：
  - `command_registration_completeness_lock.rs`——命令声明与注册双向比对，拦住上面那类
    「编译全绿、真机才炸」的漏注册
  - `outbound_egress_gate_coverage_lock.rs`——凡构造 `reqwest` 客户端的文件必须能追到 egress
    引用，凡设 `.redirect(...)` 的必须逐跳复检；插件下载那条以接线测试钉住（闸门调用必须
    出现在客户端构造之前）
  - `retired_mobile_receive_orchestration_lock.rs` 扩了对「孤儿测试目录」的判据

#### 对等网络能力域集中化：移动端接入共享 peer 能力域 crate + mDNS 复用（ADR 0048）

- **双端唯一 peer 引擎**（用户指令：基础服务能力化集中，宿主装配端口、wasm app 编排）：移动宿主的
  对等网络引擎控制面（约 4.4k 行：节点装配 / 生命周期闸门 / 会话句柄表 / 接收台账与设置落盘 /
  事件翻译 / 远端拉取编排）整体下沉双端共享能力域 `packages/bedcode-server-peer-net`
  （`default = []` 纯引擎形态，WIT 绑定留在 `common-host`）；移动 `peer_*.rs` 只剩**适配层**
  （`PeerEnginePorts` 五端口实现 + 平台缝 + `peer_ctx` 装配 + 委托）。桌面经窄端口集装配——
  **零行为变化，零 ABI / WIT / wire / 权限位变更**
- **mDNS 一律复用（不造第二份）**：引擎广播走共享 `bedcode-discovery-engine` 守护；移动宿主
  `MdnsAdvertiser` 死面（零活消费者）、`mdns_start_advertise` / `mdns_stop_advertise` 命令与
  前端死 composable **退役**；browse 薄壳保留（活消费者：terminal-session 宿主页）
- **平台缝按端注入**：SAF 共享访问 / MediaStore 落点提升 / 内置 `"下载"` 条目 / 下载目录语义为
  `PeerPlatformSeam` 参数——桌面传 `None`，行为逐字不变
- **防回接锁**：`peer_engine_consolidation_lock.rs`（退役引擎面缺席 / 适配面在场 /
  共享依赖在场，变异自检 1/1）+ `retired_mobile_mdns_advertise_lock.rs`（广播面缺席 /
  browse 薄壳在场 / 共享守护复用锚点，变异自检 1/1）

#### wasm-core 纯净性收口：形态命名中立化 + 移动独有域落移动宿主 + 域逻辑进共享 crate（ADR 0047）

- **形态命名中立化（零行为变化）**：wasm-core 内不再出现端名 / 机制名标识符——feature
  `desktop-host` / `mobile-host` → **`common-host` / `bare-host`**，模块 `runtime/{desktop,mobile}*` →
  `runtime/{common,bare}*`，`test_support/{desktop,mobile}*` → `{common,bare}*`，
  `host_api/mobile_context.rs` → `host_api/bare_context.rs`；能力域 6 crate 的装配 feature 同步改名。
  （本轮小改：`bedcode_host_kit::limits::defaults::*` 路径、`StoreSpec` 形态 re-export 补齐）
- **移动独有域整体落移动宿主**（用户指令：移动独有能力不进 wasm-core）：`host-notify` /
  `host-terminal-stream` / `host-fs-mobile` / `host-connection.primary-target` / `host-auth` /
  移动 events 的 WIT 壳 + 逻辑 + 端口迁 `bedcode-mobile/src-tauri/src/plugin/*`；移动装配链改接
  host-kit `WasmPluginState` + `HostPorts` + `ModuleRegistry`（宿主自带 `bindgen!` 解孤儿规则，
  `expect_host_module!` 白名单双向校验）；wasm-core 只保留 `#[cfg(test)]` 替身
- **含逻辑域收窄 / 下沉**：`host-http` 出站执行本体（非流式 / SSE 流 / 字节上限 / 重定向再校验）
  下沉能力域 crate `bedcode-server-http::plugin_binding::egress`（移动自持引擎退役，两域共用一份；
  `jwtAuth` 宿主代注改为**可选端口** `HttpPorts::jwt_auth_token`，默认 `Ok(None)` ⇒ 桌面行为零变化）；
  `host-fs` 交集 6 原语的机制面（权限门 → 访问裁决 → 路径形状守卫）上移
  `bedcode-host-api-core::fs`（Android/SAF 删除经端口注入）；`host-plugin-database` 判定已在目标形态
  （机制语义在 `host-api-core::database`，本端只留私有库句柄解析）
- **防回接锁**：`bedcode-mobile/src-tauri/tests/mobile_wasm_core_purity_lock.rs` 六判据
  （移动独有域不回流 / 生产装配段无其 `Host` impl / 形态壳不含域逻辑 / 共享实现核在场 /
  壳行数双上限 / 退役引擎不复活），变异自检 **4/4**
- **零 ABI / WIT / SDK / wire / 权限位变更** ⇒ 既有插件产物无需重建（纯 Rust 侧落点搬迁与改名）

#### wasm-core 单一 crate：移动 fork 退役 + 依赖图脱桌面（票 06）

- **双端共用一份 `bedcode-wasm-core`**（ADR 0045 D1）：移动 fork crate
  `bedcode-mobile/packages/bedcode-wasm-core`（`bedcode-wasm-core-mobile`，约 2.4 万行）**已删除**。
  移动 `src-tauri` 改为经 package rename 别名依赖仓库根 crate
  （`bedcode-wasm-core-mobile = { package = "bedcode-wasm-core", path = "../../packages/bedcode-wasm-core",
  default-features = false, features = ["mobile-host"] }`），`bedcode_wasm_core_mobile::` 导入路径零改动。
  桌面形态 = 默认 `desktop-host`；两形态编译互斥（`compile_error!`）
- **依赖图脱桌面**：桌面独有依赖（plugin-api / server-{core,http,ws,peer} / 加密套件 7 项 /
  wasmtime-wasi / tauri-plugin-dialog）全部 optional + `desktop-host` 门控；
  `cfg(all(target_os = "linux", not(target_os = "android")))` 把 dbus / webkit2gtk 挡在 Android 外；
  零引用依赖删除。门禁：`cargo tree --no-default-features --edges no-dev` 能力域命中 0
  （余 webkit/dbus 全经 tauri 传递，如实登记），Android target tree 全零
- **Store 装配分形态**：host-kit `wasi-store` feature 门控（桌面 WASI p3 vs 移动无 WASI）
- **退役 fail-visible**：① 旧读路径删除（目录删除 + 别名指根 crate，残留引用编译期红）
  ② 旧产物实例化期点名（v37/v20 `stale_artifact_rebuild_hint`）③ 新退役锁
  `bedcode-mobile/src-tauri/tests/retired_mobile_wasm_core_fork_lock.rs`（目录 / 包名 / 路径字形 /
  别名指向四判据，变异自检 2/2）；fork 时代移动锁改钉根 crate 路径
  （`retired_mobile_terminal_link_lock.rs`、`retired_mobile_host_terminal_hooks_lock.rs`）
- **门禁**：移动全量测试绿（22 个测试目标）；桌面零回归（宿主 check + lib 测试）；内核桌面形态 lib 591/0

#### 单一 wasm-core + WIT 分片组合：锁与文档收口（票 07）

- **契约生成物端到端漂移锁**：`scripts/compose-wit.mjs --check` 双端接入独立 CI workflow
  （`.github/workflows/wit-drift-lock.yml`）与 husky pre-commit；仓库根 `pnpm run check:wit`
  为可复跑入口。变异双向自检：手改生成物 → 红（点名文件）；改分片真源没重拼 → 红（双端）
- **新增防回接锁**（每条均变异自检）：能力域自持分片 bindgen 锁 + 端清单↔装配面双向动态对照锁
  （宿主 `src/plugin/bindings.rs` cfg(test)）；移动 fork 退役锁 + 移动 WIT 组合锁
  （`bedcode-mobile/src-tauri/tests/`）
- **边界锁登记表复核**：`crate_boundary_lock.rs` ALLOWED 表补票 06 mobile-host 新边
  （`bedcode-wasm-core → bedcode-discovery-engine`）；REQUIRED 不动（该边仅移动形态）
- **文档收口**：ADR 0045 转 accepted（实施记录：POC 四命题结论 / D3 复评 = 拆分执行 /
  D6 fork 退役证据 / 与 ADR 0035·0036·0037·0040 衔接）；双端 code-map 登记核心 world /
  能力域分片 / 端清单三处 + 新锁索引条目；AGENTS §4/§5.4 按单一 crate 改写；
  `docs/commands.md` 增 WIT 拼装小节；插件开发检查清单更新（分片真源 / 生成物禁手改 /
  v37·v20 产物重建口径）

#### 核心 WIT 交集切片收拢 ws / fs / http（票 04）：双端 ABI bump + 产物重建

- **三个交集接口收拢进共享核心**（`packages/bedcode-wasm-core/wit/core.wit`）：
  `host-websocket`（客户端域 5）、`host-fs`（交集 6）、`host-http`（出站 fetch 1）。
  核心 WIT 现有 17 个全等接口；各端 `cap-*.wit` 只留扩展与端独有面
- **桌面扩展拆出**（破坏性）：`host-websocket-server`（服务端域 10）与
  `host-http-endpoint`（入站路由 2）为能力域分片里的新接口
  （`server-websocket/wit/ws.wit`、`server-http/wit/http.wit`）；WIT impl 留在能力域
  crate（desktop-host），经同一模块自报装配路径注册
- **移动扩展拆出**（破坏性）：`host-fs-mobile`（2：save-to-document /
  write-media-downloads）为 `cap-mobile.wit` 新接口，由移动 fork 实现
- **双端 ABI bump**：桌面 36 → **37**、移动 19 → **20**（断言 + WIT/abi.rs 历史注释联动）。
  旧产物（v36/v19 SDK 构建）在实例化期被 `stale_artifact_rebuild_hint` 点名缺失接口与
  重建版本——正向（重建）与反向（升级 BedCode）两分支都有新判据与测试；原 v29 反向
  判据退役（其锚 `host-http.register-endpoint` 现归 v37 正向分支所有——v28- 宿主在
  当前代码库不存在）
- **bindgen path 目录化补齐**：`plugin-component-test`（移动）与 `plugin-system-test`
  （桌面）改指生成物 `wit/` 目录（push_dir）而非合成后的 bedcode.wit 单文件——票 03
  十五条目清单漏掉的两位消费者
- **插件产物全量重建 + wasmHash 注入**（桌面 4 + 移动 3）；双端 SDK `cargo check --features
  wasm` 绿；内核 `cargo test --lib` 593/593、移动 fork 284/284、桌面宿主 `plugin::*` 75/75。
  `cross-end-tests` 延后至重构波次稳定后统一验证（项目级决策，非本票门禁）


#### 移动端：壳只按 surface 形态加载应用——旧嵌入扩展点整面退役（ADR 0046）

- **壳只解析一种运行面形态**：`pluginAppSource.resolveSurface` 删掉四级回退链
  （`terminalView → toolbox → navTab → route`），只读 `ShellRegistry.contributions.surface`。
  这条回退链正是「壳用旧宿主的插件形式加载页面」的实际代码；留着它还等于「四个独立权限位里
  最弱的那个也能打开应用主面」。未注册 surface 的应用改为显示「该应用尚未提供运行面」空态并
  写明原因，不再静默找替代面
- **四个旧嵌入扩展点整面退役**（ADR 0046 D2）：`registerToolboxPage` / `registerNavTab` /
  `registerTerminalToolbarItem` / `registerTerminalView`，连同描述符类型、宿主注册表存储面、
  宿主 UI 组件（`PluginNavTabHost` / `PluginSettingsHost` / `PluginTerminalBar`）、dev-shell
  对应面与 SDK 模板一并删除。四者在运行期**本就全是死代码**——逐个核实零活消费者
  （其中工具栏组件的宿主注入从未 provide，`v-if` 恒假）。
  `registerRoute` / `openPage` / `goBack` / `onBackPressed` **保留**：应用内子页仍需跳转与返回
- **退役权限位加载即抛**（§5.1.3 形态③）：`ui:toolbox` / `ui:navtab` / `ui:input` 从 SDK 白名单
  与 API 映射表移除，并登记进新增的 `RETIRED_PERMISSIONS` 表 + `check_retired_permissions()`；
  `PluginLoader::load_all` 对声明了它们的 manifest **直接拒载**，报错指名权限位与迁移出路；
  manifest-gen 幂等剔除退役位与退役的 `contributes.views` / `navTab` / `terminal.toolbarItems`。
  理由：不在白名单的权限位会被 `grant_permissions` **静默丢弃**，旧插件会「加载成功但能力
  凭空消失」，把排查方向带偏
- **保留显性抛错桩让断链可见**：四个退役方法仍留在 `context.ui` 上，调用即抛错并指名扩展点 +
  指向 `registerSurface`；但**不进 `UIRegistry` 类型**——类型面必须让旧调用点编译期就红
- **业务设置按归属切分**（ADR 0046 D4）：宿主设置子页只留平台项（链路加密 / 生物凭证 / 出站授权 /
  外观 / 关于），业务设置（自动重连 / 保持连接 / 默认端口 / 通知三开关 / 震动 / 声音 / 终端上限 /
  首选认证）归 terminal-session 应用内设置页。`NotificationSettingsView` 与
  `mobile-settings-notifications` 路由整页退役——每一行都是业务项
- **业务设置真源下沉出宿主**（修正 AGENTS.md §5.1 **B3** 越线形态）：「有哪些设置项 / 默认值 /
  取值范围」是产品事实，现由应用自持（`wasm-apps/terminal-session/src/settings/settingsModel.ts`）；
  宿主只提供不绑定任何业务形状的通用 KV 通道（`readAllSettings()` / `writeSetting()`）。
  键名**刻意**沿用 `MobileSettings` 字段名，使宿主消费者经写穿路径仍读到真值
- **文件浏览器留宿主作公共组件且恢复可达**：`App.vue` `provide('bedcodeHostComponents',
  { FileExplorer, FileSidebar })`，激活终端域早已写好却一直无人 provide 的两个挂载位——
  文件浏览器因此获得一条不占宿主路由的入口，长期恒假的 `v-if` 也转为生效
- **删除 7 个宿主死文件**：组件 `DeviceCard` / `InputBar` / `PluginIcon` / `RepeatableToggle` /
  `SessionListItem` 与 composable `useAndroidFeatures` / `useRunTime`（各自在 `wasm-apps` 下
  有自持副本）；`PluginIcon` 的测试随之删除
- **补回票 2026-10-09 阶段 B 造成的回归**——该票点名两个丢失 action 并要求都补回，
  二者按设计落在**不同归属**：
  - 「重置设置」重置的是**业务设置项**，落在 terminal-session 应用设置页（ADR 0046 D4）
  - 「清除所有数据」是**设备级擦除**，刻意留在宿主危险区
    （`src/composables/useClearAllData.ts`）。其清理对象含设备入场凭据（认证中心托管）
    与宿主连接态；按 §8 凭据零过境 + ADR 0033，插件不得持有或擦除凭据，
    放进应用设置页属 §5.1 越线而非归属选择。执行序固定为
    「先断连 + 停前台服务 → 再清本地 → 最后 reload」；**断连失败不阻断清理**
    （用户目标是清数据，中断只会让凭据留在盘上）但如实标记 `disconnected: false`；
    **清理失败不 reload、不吞错**，以 `completed: false` + 原因返回并提示——
    「清了一半」的设备绝不能被呈现为已清干净
- **防回接锁扩展**——`retiredHostUIRetirementLocks.test.ts` 新增 R4（退役扩展点与其宿主组件
  不得回接；`resolveSurface` 不得有回退链）、R4b（已下沉设置路由不得回接）、R5（已删孤儿不得
  回接且文件确已不在盘）
- 移动端插件契约独立（ADR 0018）：本次退役**不波及**桌面端，也未触发 wasmtime 版本变更
  （ADR 0019）。ADR：`docs/adr/0046-mobile-shell-surface-only-and-retired-embed-extension-points.md`

#### 移动端：dev-shell 调试壳改用新宿主壳形态——内置 mini 壳 + 内置应用降级

- **调试壳不再自造一套导航形态**：`dev-shell` 原先是「三页签骨架 + 全局 activeView 视图栈」，
  与真机宿主壳形态不同，插件在浏览器里跑通并不能说明真机上也跑通。现内置与宿主
  `src/shell/**` **同构的 mini 壳**（`dev-shell/src/shell/**`：types → registry → composables →
  components/screens，数据源 `adapters/devAppSource.ts`），屏幕栈（首页 / 应用 / 详情 / 运行面 /
  多任务 / 权限总览 / 我的）、胶囊、平台覆盖层与「只认 `registerSurface` 一种运行面形态」逐项对齐
- **npm 自包含边界不变**：SDK 的 `files` 白名单整份打包 dev-shell，外部用户装不到 app 源码，
  因此这里是同构实现而非共享组件；契约漂移由 `contractDrift.test.ts` 钉住
  （`ShellApp` / `ShellPermissionGrant` / `ShellAppState` / `ShellAppSource` 字段集与
  屏幕 id 集合逐项比对宿主源文件，宿主源不在场时整组跳过）
- **内置页降为内置应用**：模拟终端不再是壳的调试页签，而是自持运行面 + 首页快捷卡片的
  **内置应用**（`dev-shell/src/apps/mock-terminal/`），与被调试插件走同一条加载与渲染路径——
  预览环境里始终有一个形态正确的应用在场，壳的挂载路径因此可被真实验证
- **插件动态路由改走 vue-router**：`registerRoute` / `openPage` 与宿主同构为整页跳转
  （`/plugin/{pluginId}/{id}`），不再塞进壳内视图栈；运行面容器仍套 `PluginContextHost`
  补 `provide('pluginContext')`，插件形态细节不外泄到壳
- **顺带修两处真实缺陷**：`toShellApp` 漏投影 `error`（详情页只剩「启动失败」徽标、无原因）；
  `activatePlugin` 在停用后再启动会追加同 id 第二条记录（`getPluginRecord` 命中旧记录 →
  插件 inject 到已释放 context，应用列表出现同一应用两行）
- **i18n 双语同步**：壳文案按宿主 `locales/*/shell.ts` 同键同义迁入
  （`dev-shell/src/locales/`），随旧页签退役的 `devshell.*` key 整面删除
- **测试**：SDK 根 vitest 纳入 `dev-shell/__tests__/**`（逻辑层，不挂载 SFC——组件渲染由宿主
  `bedcode-mobile/src/__tests__/shell/**` 负责），5 文件 61 例；漂移锁经突变验证（加一个多余
  字段 / 删一个屏幕 id 均测红）。dev-shell 与三个移动 wasm-app（terminal-session / ai-chatbox /
  file-transfer）`vite build` 全绿，移动端 `pnpm run test:run` 895 例全绿
- 未覆盖：`src/loader.ts` 无自动化测试（依赖 vite 虚拟模块与 SFC 链，接进 vitest 需引入与 vitest
  内置 vite 主版本不一致的 `@vitejs/plugin-vue`，改用真实 wasm-app 的 `vite build` 冒烟覆盖）；
  壳内组件渲染不在此重复测；真机/浏览器人工核验未跑

#### 移动端：宿主前端迁移进 wasm-app——壳为默认入口、三应用独立页面、旧前端机制落地新界面

- **默认入口改为宿主壳（`/mobile/shell`）**：`/` 重定向到壳；旧四页宿主（`MobileSwipeContainer`）
  过渡期仍经 `/mobile` 可达（退役另票，待真机复核）
- **三个 wasm-app 成为壳内独立页面**：`file-transfer` / `ai-chatbox` 停掉工具箱页 / 导航 Tab
  嵌入，改为注册壳运行面（`context.ui.registerSurface`）；manifest 同步对齐（清掉失效的
  `views` / `navTab` 贡献块与不再使用的 `ui:toolbox` / `ui:navtab` 权限位）
- **旧宿主主流程下沉到 `terminal-session`**（`wasm-apps/terminal-session/src/host/**`）：
  设备发现（mDNS / 手动 / 连接历史）→ 配对（配对码 / 生物 / **二维码**）→ 会话列表
  （启动 / 停止 / 删除）→ 终端，作为该应用的壳运行面 + 首页快捷卡片（`host-sessions`）；
  任务页也在壳内可达（动态路由 + 胶囊菜单项，新增 `ui:route` 权限位）
- **引擎事实投影进插件（零 WIT / ABI 变更）**：`mobileApi` 的连接态与连接历史、6 个连接生命周期
  事件白名单（`ws_reconnecting` …）、mDNS 原始发现事实、生物凭证**仅状态**（凭据留宿主，C4）
- **旧前端机制落地新界面**：插件域错误码机制（`classifyConnectionError` + `ensureCommandOk`；
  错误槽位存 i18n key 或服务端原文、一律经 `t()` 渲染）、所有 catch 经 `context.logger` 记录、
  无硬编码用户可见文案（双语 `hub.*` / `hub.qr*` 键集合由测试钉住）
- **缺陷修复**：宿主页两个区块的模板引用了九个未声明的绑定——运行期 Vue 告警且 mDNS 按钮、
  连接历史、配对区、断开按钮、生物入口整块不渲染；改为显式 `computed` 收口，并新增组件挂载
  测试锁住
- **门禁**：移动端 `pnpm run test:run` 全绿（77 文件 / 831 用例，含新增 9 个门禁文件）；根
  `eslint .` 0 error；插件 `vue-tsc --noEmit` 0 error；插件产物已重建。**真机核验未跑**
  （Rust 核心重构中），核验清单见 `.scratch/2026-10-09-mobile-host-into-wasm-apps/`

#### 移动端：旧前端宿主退役——旧视图 / 壳布局 / 插件嵌入面删除，并加防回接锁

- **旧四页宿主已删**：`MobileSwipeContainer` / `MobileNav` / `MobileLayout` /
  `MobileStatusBar` 与 `src/views/{DevicesView,SessionsView,TerminalView,PluginView,
  SettingsView,PresetTasksView,ToolboxView}`，以及它们独占的孤儿组件（`ScanPanel` /
  `BiometricAuthDialog` / `PairingInput` / `PresetTaskCard` / `SessionConfigCard` /
  `SessionCard`）与对应失效的测试 / 夹具
- **`App.vue` 改为渲染 `<router-view />`**：布局框由壳自承（`ShellView.vue` 的
  100dvh / 安全区 / 祖先类）；router 删旧宿主路由，仅保留 `/` → `/mobile/shell`、
  `/mobile/files/:id`（CodeExplorer 归属待定，暂留）与 `/mobile/settings/*` 子页
  （壳设置跳转链接受）
- **`registerSettingsSection` 全链路退役**（旧宿主设置区已无消费方）：SDK 类型、
  `src/plugin/{context,registry,permission,types}.ts`、dev-shell（`registry` /
  `mock-context` / `PluginsView.vue`）、夹具 `mockLifecyclePlugin.ts`、
  `pluginReactivate.test.ts`、file-transfer 的设置区块（改 `registerSettingsEntry` →
  `ui.openPage('settings')`）
- **新增防回接锁**：`src/__tests__/shell/retiredHostUIRetirementLocks.test.ts`——R1 退役路由名、
  R2 退役视图 / 组件符号（词边界、跳注释、排除锁文件自身）、R3 正面钉壳等价物
  （`ShellView` / `ShellHost` / `ShellTabbar` / `ShellSettingsScreen` + 路由 `mobile-shell`）在场
- **门禁**：移动端 `pnpm run test:run` 全绿（72 文件 / 812 用例；835→812 系六个退役面测试
  文件删除的预期差）；根 `eslint .` 0 error；插件 `vue-tsc --noEmit` 0 error；SDK dist 重建；
  三插件产物全部重建；此前偶发的 `useMdnsDiscovery` 失败未复现（确认重负载 flaky，非回归）；
  **真机核验未跑**

#### 移动端：Android 原生通知/震动/声音能力封装——`host-notify` 域（ABI 18）

- **新增移动特有域 `host-notify`（5 函数）**：`notify`（title/body + `options-json` 的
  `{ vibrate?, sound? }` 开关，两者缺省均 true）/ `check-permission` / `request-permission`
  （Android 13+ POST_NOTIFICATIONS）/ `vibrate`（毫秒，直接走 Vibrator 不经通知渠道、
  无需通知权限）/ `play-sound`（系统默认通知提示音，重复触发先停上一次防叠音）；
  新增权限位 `notify`（fail-closed——通知/震动/声音是用户打扰面，独立成位）
- **`host-events.notify` 收编入新域**：host-events 回归纯事件语义（只剩 emit）；
  **破坏性收缩**——引用了 `host-events.notify` 的 v17 及更早产物在实例化期被点名失败
  （fail-visible ②）须重编译；内置插件零消费者且随 APK 分发，产物随本版重建
- **实现**：fork crate `host_impl/notify.rs`（权限门 + options-json 严格解析 + Android 分支）、
  宿主 `plugin/host_ports.rs` 端口接 Kotlin `TaskNotificationPlugin` / `TaskNotificationManager`
  （showPluginNotification 参数化 + pluginVibrate / pluginPlaySound；vibrateOnce /
  playSoundOnce 抽取）；`android-backup/app-java/` 恢复副本同步
- **门禁**：fork crate `cargo test --features test-support` 全绿（含 A1/A3 锁更新：17 import /
  22 interfaces / ABI 18，host-events 收缩 + host-notify 行）· 移动宿主全量 · Kotlin
  `./gradlew :app:compileUniversalDebugKotlin`

#### 移动端：防回接与漂移锁——SDK 契约对照锁 Part A + 对称结构锁 Part B（票 19）

- **Part A**（`packages/bedcode-wasm-core/tests/sdk_wit_contract_locks.rs`，4 例）：A1 WIT 接口
  清单锁（world import/export 集合 + ABI 版本，先改锁再改 WIT）· A2 权限词汇五同步锁
  （SDK 表 ↔ fork re-export 可见集逐字一致 + 「定义了未登记表 = grant 静默丢弃」完备性检查）·
  A3 WIT↔host_impl 接线全表（接口×函数三方对照：WIT 名 / 实现名 / component.rs 委托行）·
  A4 wire 形状对照对 + 单源防副本锁（3 对真实双份逐字段钉住；9 个宿主自持单源形状双侧零副本）
- **Part B**：`fork_boundary_lock.rs` 新增对称结构锁——21 个机制核模块路径必须同时在桌面整核
  与 fork 双侧在场（`src/error.rs` 不入对称面：移动 AppError 是自持形状，桌面真源在
  `bedcode-server-base`）；共享锚点白名单扩入 `bedcode-discovery-engine`（ADR 0042）与
  `bedcode-ws-client-engine`（ADR 0043）——两者都是纯引擎能力 crate
- **事实修正（ADR 0022）**：历史声明的 `mobile_parallel_copy_shape_lock` 此前不存在，且移动
  宿主 `enums/` 并非 SDK wire 的平行副本（12 形状中 9 个为宿主自持单源，真实双份仅 3 对）——
  锁按 A4 双层落地，「平行副本」前提按事实修正
- **门禁**：fork crate 303 绿 + 移动宿主全量（22 测试目标）零失败；变异自检 3/3（WIT 增接口 /
  权限常量未登记表 / 宿主 enum 字段改名均测红）

#### 核心：host-api 共享实现核——database / log / events 三域 + 主库收归（票 18 批次 3+4，ADR 0040）

- **共享核再收三域**（`packages/bedcode-host-api-core/src/{database,log,events}.rs`）：
  database = 权限门化的插件私有库机制（SQLite authorizer 引擎层纵深 / 语句超时护栏 /
  行+字节结果集护栏 / 批次事务，机制级依赖 rusqlite hooks + regex）· log = 桌面全套
  （callsite 缓存 / per-plugin 级别阈值 / `[plugin:xxx]` 前缀，thread_local 缓存）·
  events = 载荷严格 JSON 解析（非法载荷拒绝投递 + warn，对齐桌面 fail-visible）。
  config / fs / http 三域**判定不抽**（实现层引用各端 SDK 枚举或属各端平台接入，
  票 18 §10 记录在案）
- **移动 fork 补齐此前缺失的桌面机制**：host-plugin-database 获得语句超时护栏 /
  错误文案对齐（`database error: {}`）；host-log 从裸 `tracing::*!` 切换为共享核
  callsite 缓存实现；host-events 拒绝畸形载荷而非宽松降级为字符串投递
- **主库双端退役（用户裁决，ABI 桌面 34→35 / 移动 18→19）**：`host-database` 接口、
  `HostDatabase` SDK trait 与 `database:main` 权限位自双端 WIT / SDK 移除；主库是
  wasm-core 机制内部真源（激活状态 / 审批记录 / 授权记录 / plugin_storage）——
  **插件数据库能力 = 插件私有库**（`host-plugin-database`，`storage` 位）。双端插件
  生态实测零主库消费者（零迁移负担）；旧产物实例化期 import 缺失点名失败、须重建
- **桌面 adapter 收缩**（`host_api/database.rs` 865→约 300 行，组件绑定零签名变化）；
  `bedcode-server-base::constants` 改 re-export 保 `PLUGIN_DB_*` 常量真源单点；
  能力注册表删 `host-database`（20 组）
- **门禁**：共享核 34+2 全绿；桌面 wasm-core 669 绿 + 1 既有 perf 基线；移动 fork
  284 + 锁 8 绿（A1/A3 更新至 v19）；移动宿主全量；桌面 ABI/WIT/world 零漂移

#### 文件传输：双端共享业务核 `packages/bedcode-file-transfer-core`（ADR 0044）

- **新建双端共享业务核 crate**：文件传输业务实现（任务台账归约 / 重试 + 发送闸门 + 拉取意图判据 /
  共享根注册表 / 接收设置 / 会话表，约占两端各端代码 70%）收敛为**双端同一份**；每个端差异
  都表达为端口 trait（`ports.rs`，差异面唯一落点：SQL 表 vs KV 持久化、`path` vs `safTreeUri`
  wire 形状、节点电源、落点策略、平台选择、信任决策路径）——零 SDK / 零 WIT / 零平台依赖，
  **零产品身份字面量**（插件 id 经 `PluginIdentity` 运行期注入；核内 `boundary_lock.rs`）
- **双端退化为薄层**：各 wasm app 保留 `adapters.rs`（自家 SDK trait → 核端口的 1:1 委派，
  零额外判据）+ 签名不变的模块包装，`peer.rs` 与全部前端代码零改动（移动端页面按要求不动）；
  接线防漂移锁（`src/wiring_lock.rs`，5 例 + 变异自检 4/4）钉住核与端 app 的边界
- **桌面行为对齐（用户裁决 B）**：桌面 `peer.rs` 按移动端票 08 修正语义演进——引擎事件即
  `pull-started` 建行锚点、重试判据前置、发送闸门、排队批派发失败落终态行；旧快照通路
  （`merge_snapshot` / `prune_absent` / `reconcile_diff`）保留承担对账校正
- **门禁**：核 crate 72 全绿（含边界锁 + 接线锁）；函数级等价校验（核内 23 函数 vs 两端基线）PASS；
  桌面插件 crate 47 全绿；移动插件 crate 29 全绿 + 产物重建且 host import 集合与改动前同集合
  （未新增 import）；桌面产物重建被在途 `manifest-gen` 权限表阻塞 + 本机缺 wasip3 工具链，留 T6

#### 移动端：WS 出站连接引擎抽根为 `packages/bedcode-ws-client-engine` 能力 crate（ADR 0043）

- **新能力 crate `packages/bedcode-ws-client-engine`**：移动端 `host-websocket` 客户端域机制
  （约 1,350 行：句柄表 + 属主仲裁 / reader-writer 双任务 / 心跳与静默判死 / 退避自动重连 /
  帧信封 / 停用回收）自 fork crate（`bedcode-mobile/packages/bedcode-wasm-core/.../host_impl/ws.rs`）
  抽出为通用 crate，默认形态 = **纯引擎 + 端口抽象**——零 WIT / 零 SDK / 零平台（tauri）/
  零机制内核（host-kit）依赖。平台差异面经 7 方法端口 `WsClientPorts` 注入：权限门
  （`ws:client`，fail-closed）/ 总线 JSON 与二进制投递（topic 由引擎拼好）/ 宿主运行时任务
  派生（可取消 `WsTask`，运行期句柄不外泄）/ jwt 代发 token（凭据不落插件）/ 重连策略与
  钳制边界（全局退避单一事实源仍在宿主）
- **移动端收窄为薄适配器 + `MobileWsClientPorts`**：5 原语（connect / send-text / send-binary /
  close / is-connected）与 `purge_for_plugin` 签名及错误文案逐字保留；同步↔异步桥
  （`guarded_host_call` + `block_on_async`）留在宿主侧。**零 ABI / 零 WIT 变更**（本任务不触碰
  `bedcode.wit` 与 SDK `abi.rs`）**且行为零变化**：事件 topic、帧信封形状、权限位、fail-closed
  与停用回收语义逐字保留
- **wire 契约自持 + 漂移锁**：`src/wire.rs` 自持副本（事件名 / 属主私有 topic 拼法 / 帧 kind
  与头长 / `ws:client` 字面量），`wire::drift_lock` 与移动 SDK 源文件（`host/ws.rs` /
  `permission.rs`）文本级比对——任一侧漂移即测红；插件消费面（`parse_ws_frame` / `HostWs`）
  仍住移动 SDK
- **治理与防回接**：crate 内 `boundary_lock.rs`（生产源码零平台 / SDK / wasm-core / host-kit /
  WIT 绑定层针脚，生产清单零内部 crate，单测限 `src/` 内）；登记进桌面
  `capability_crates_no_product_ids` 扫描面；fork 边界锁共享锚点白名单 4 家 → 5 家
  （+ `bedcode-ws-client-engine`，与 ADR 0042 的 discovery-engine 同理由）；移动 ws 域边界锁
  修陈旧实现路径 + 扫描面扩到引擎 crate，并加目标文件存在性断言（陈旧路径会让锁静默空转）
- **门禁**：新 crate `cargo test` **20 绿**（引擎 13 + 漂移锁 3 + 边界锁 4）；fork crate
  `cargo test --features test-support` **285 + 4 + 4 绿**（含真实组件
  `ws_client_domain_full_loop_with_real_component` 全链路回归）；移动宿主 `cargo test`
  全量；变异自检 3/3（漂移锁 / 边界锁 / 适配器映射）


#### 桌面+移动：双端共享 mDNS 引擎（ADR 0042，M1–M4）

- **共享引擎** `packages/bedcode-discovery-engine` 成为双端唯一的 mDNS 引擎（引擎机制 + 双句柄表
  + 属主仲裁；`desktop-host` feature 门控 WIT 绑定层，默认形态零 WIT）。新增 `set_daemon_init_hook`
  平台钩子 / `pub daemon_if_initialized`；`register_host_service` 去掉端口参数（NullTask 占位——
  桌面 `MdnsPort` 实现同步简化）
- **移动端** fork crate `host_impl/mdns.rs`（821→~230 行）重写为薄转发层 + `MobileDiscoveryPorts`
  （权限门 = manifest / 总线发布 / 节点 ID 自播回显过滤 / 宿主运行时任务派生，替换历史
  std-thread 阻塞 recv 循环）；宿主 `mdns/engine.rs` **删除**——守护单例真源移入共享引擎（Android
  多播锁经宿主 setup 装配的 init 钩子获取）；`HostEnginePorts::{mdns_daemon, mdns_daemon_if_initialized,
  mdns_reannounce_interval}` 退役；mdns 事件 topic wire 统一为 `<owner>::mdns:found|lost`
  （ABI 不变；file-transfer 插件 Rust 订阅字面量与 SDK/WIT 注释同步迁移）
- **peer-net**：`spawn_peer_mdns_advertiser` + `DiscoveryAdvertiser` 删除（零生产消费者；广告面由
  宿主自播 / 插件 advertise 在共享守护上覆盖）
- 门禁：移动端 fork crate `cargo test --features test-support --lib` **290 全绿**（清偿上一条票 18
  的「暂挂」注记）、移动端宿主 245 绿、桌面 discovery-engine 31 绿、peer-net 101 绿；移动端
  `ServiceDaemon::new()` 代码层归零；旧 `mdns:found.<owner>` 字面量扫描归零

#### 桌面+移动：host_api 共享实现核批次 2——bus 域语义（票 18）

- **bus 语义抽取**至 `packages/bedcode-host-api-core::bus`：topic 形态机制（`owned_topic` /
  `topic_owner` / 回复道 / legacy 形态识别——宿主侧单点，guest 侧拷贝留在桌面 SDK；票 19 Part B
  对照锁将钉住两份一致）+ 审计票 05 三道门禁（命名空间 / 订阅面 / 互调门）+ 发布判定链
  （权限位 → 严格 JSON → 命名空间 → 互调门 → 投递）。队列与订阅簿留各端
- **两端策略分叉由端口承载**：桌面无权限位（topic 形态即 ACL）→ `Option<&PermissionGate>` 传
  `None`；移动查 `PERMISSION_BUS`；桌面互调门走 core-security 授权框架，移动恒放行（WIT v17
  无 host-api-call）
- **移动行为对齐**（此前移动总线无任何门禁——双份漂移税实例）：发布/订阅过命名空间门、回复道
  + legacy 形态订阅显性拒绝、退订仅命名空间门（清理幂等）、非法 JSON 发布由「降级原始串」改为
  显性拒绝（既有移动插件经 SDK `serde_json::Value` 走公开道——实测 file-transfer 零回归）
- **桌面** adapter 重写，绑定层与测试套件逐字保真（已对 HEAD 核验）；门禁：`cargo test` 全量
  677 绿（+1 既有 perf 红基线）、无头编译过、ABI / WIT / world 零字节变动
- **移动**：fork crate lib 编译通过；全量测试门禁待并行「双端共享 lib M3」会话收口后补跑
  （其在途夹具面当前使 crate 测试构建编译红）

#### 桌面+移动：host_api 共享实现核批次 1——storage 域（票 18）

- **新建 crate** `packages/bedcode-host-api-core`（ADR 0040 第二步）：无 WIT 依赖域的 host_api
  机制实现层，「实现层 + 各端 adapter」两层化；仅机制级依赖（serde_json / tracing——禁 SDK /
  tauri / tokio），由新建 crate 边界锁强制（变异自检 2/2）
- **storage 域抽取**：权限门（权限词汇经参数传入）→ 系统空间纵深守卫 → 能力路由（桌面独有，
  端口默认 `None`）→ 键值原语（serde_json 规范形）；`SYSTEM_PLUGIN_ID` 真源随实现层上移，
  双端经 re-export 保既有路径
- **桌面** `bedcode-wasm-core`：`host_api/storage.rs` 变薄 adapter（`SqlitePorts` → 共享核端口），
  域函数签名与 guest 可见错误文本逐字保留。门禁：`cargo test` 全量 677 绿（+1 既有 perf 红基线）、
  `--no-default-features` 无头编译通过、ABI / WIT / world 零字节变动
- **移动** fork crate：`host_impl/storage.rs` 同形 adapter；**行为对齐**——系统空间纵深守卫自本批
  起对移动端生效（此前移动缺该守卫，双份漂移税实例）；`set()` 的 JSON 解析移到权限门之前
  （仅边缘入参错误文本变化，授权路径零变化）。移动全量门禁暂挂：并行在途「双端共享 lib M3」
  （mdns → discovery-engine）使 fork crate 处中间态；storage 侧 3 文件在该基线下名字解析零报错

#### 移动端：egress 三档访问策略对齐 + 闸门锁（票 20）

- **收口**：egress 安全闸门（`src-tauri/src/egress.rs`，留宿主裁决 ADR 0022 D5——三档只回答
  「遇到授权记录未覆盖的目标时要不要问」，B1–B6 零命中）：档位→动作映射保持单点
  （`StrategyStep::of`）；写入面保持 `parse_wire`（未知值显性报错，不猜档位）；deny 记录优先于
  一切放行路径（含 always_allow 档）；弹窗超时 fail-closed；`always_allow` 必落审计记录
- **修复**：① `decide` 此前 `strip_prefix("plugin:")` 去前缀，而 `record_grant` / `set_plugin_strategy`
  存带前缀 key → 记录与档位永远匹配不上（即长期以「在途基线」挂账的 6 例失败根因）；带前缀来源
  现保留完整前缀，无前缀来源归一为 `host`。② `EgressSettingsView` 读 `path_prefix` 而 `AuthRecord`
  serde camelCase → 路径粒度恒显示「全部路径」；interface 与读取点统一 camelCase。
  ③ 共享全局 `policy()` 的测试并行互踩 → 加 `POLICY_LOCK` 串行锁确定性（18/18 绿）
- **新防回接锁**：`egress_tier_mapping_single_point_lock.rs`（3 例 + 变异自检 3/3）——映射单点旁路 /
  写入面读面解析 / 安全义务符号（`must_land_auto_allow` / `CONSENT_TIMEOUT` / deny 记录消费）在场
- **前端测试**：`EgressSettingsView.test.ts`（12 例：档位切换 / 记录管理 / 空态加载态 / 异常 / 多来源隔离）
- **门禁**：移动端宿主 `cargo test` 全量绿（320 lib + 全部集成目标）；前端 `pnpm run test:run` 732/732；
  根 eslint 0 error；零 ABI / WIT / wire 变更（宿主内部收口，cross-end-tests 不适用）

#### 移动端：wasm-core fork crate 迁入移动运行时与 16 域 host 原语（票 17 批次 1b）

- **落地**：`bedcode-mobile/packages/bedcode-wasm-core`（`bedcode-wasm-core-mobile`，fork 自桌面整核）
  迁入移动运行时与绑定层——`manager/runtime{,/component.rs,/host_impl/}`：wasmtime Engine/Store/AOT
  缓存、bindgen 换绑移动 WIT v17（16 import / 5 export + 可选 events-binary）、16 域 host 原语
  （auth/bus/config/connection/db/event/fs/http/mdns/notify/peer/platform/storage/terminal_stream/ws/support）。
  宿主引擎调用（auth / egress / peer 四模块 / mdns 守护 / android 平台桥）经新增
  `host_api/ports.rs` 的 `HostEnginePorts` 端口注入（30 方法 + 五个子 trait + `UnimplementedPorts`
  无头占位）——auth 凭据（C4）、egress 安全闸门（D5）、peer 引擎、mDNS 守护单例、重连状态机真源留宿主
- **拆分迁入**：宿主 `wasm_host.rs` 拆为 `host_api/{http_engine,sql_guard}`（HTTP 执行引擎 + SQL
  表名前缀护栏，egress/token 经端口）；`terminal_stream_gateway.rs` 窄转发表迁 crate（Tauri 命令
  薄壳留宿主）；`test_support` 测试支持面（夹具构建器 + mock WS server + `MockPorts` 端口替身，
  `any(test, feature = "test-support")` 门控）。fs_auth 形状漂移裁决：宿主保持自持，经 `FsAuthGate`
  端口（check/check_batch）注入，白名单/弹窗真源不动
- **门禁**：fork crate `cargo test` lib 295 用例 + fork_boundary_lock 3 用例全绿（批次 1 基线 230
  + 新增 65）；桌面 crate 零改动；移动宿主零改动（未接线）。宿主切换（垫片替换）为批次 2b，
  前置裁决三项见票文档 §6.1

#### 移动端：宿主机制面切换至 fork crate（票 17 批次 2b）

- **落地**：宿主 `plugin/` 变转发垫片（`pub use bedcode_wasm_core_mobile::…`）——76+ 处
  `crate::plugin::` 引用路径零改动；`wasm_host` 符号面逐字保真（glob re-export http_engine/sql_guard）。
  宿主侧端口装配 `plugin/host_ports.rs` 注入真引擎（auth C4 / egress D5 / peer 四模块 / mDNS 共享
  守护 / android 桥 / `FsAuthGate`）；`lib.rs` 插件库连接所有权移交 crate `Database` wrapper
  （schema 真源留宿主 `db_schema.rs`）
- **退役（宿主侧）**：`plugin/{wasm_runtime,wasm_host,validation,storage,message_bus}.rs` 与
  `terminal_stream_gateway.rs`（窄转发表迁 crate 根）
- **锁/测试收口**：4 把保留面锁（terminal_link / host_terminal_hooks / auth_orchestration /
  session_control）改钉 fork crate 新真源；`session_http_flow` 换 `http_engine` 端口签名
  （真 `HostPorts`，全局 token JWT 代注语义不变）
- **门禁**：fork crate 295 lib + 3 锁；宿主 245 lib + 全部集成目标（含 4 锁 + session_http_flow）；
  前端 `pnpm run test:run` 732/732；根 eslint 0 error

#### 移动端：业务应用源码目录 `plugins/` → `wasm-apps/`（对齐桌面）

- **重命名**：`bedcode-mobile/plugins/`（ai-chatbox / file-transfer / terminal-session）→
  `bedcode-mobile/wasm-apps/`，与桌面 `wasm-apps/<app-id>/` 同构。「插件」机制词保留：SDK 包、
  WIT / 权限位 / bus·events 话题、运行时 `app_data_dir/plugins`、`resources/plugins/mobile`、
  `src/plugin/`（前端机制）与 `src-tauri/src/plugin/`（Rust 机制）全部不动
- **触点已同步**：CI 插件安装循环（`test.yml` / `release.yml`，working-directory 相对与带前缀两种形态）、
  `scripts/{dev-run.js,plugin-build.js}` 扫描路径、`vitest.config.ts` include、`vite.config.ts`
  chunk 前缀判断、`tailwind.config.js` 内容扫描、4 把防回接锁源码路径字面量、`test_support.rs:52`
  夹具构建路径、8 个 file-transfer 测试相对 import、文档（`AGENTS.md`、`code-map.md`、
  `plugin-dev-mobile.md`、`commands.md`、`wasip3-toolchain.md`）；审计记录
  `.scratch/2026-10-08-mobile-wasm-app-rename/audit.md`（含审查补漏触点 #18-#23、release.yml
  #292-297、code-map #299 修正）

**票 04（整数 ABI 退役）已落地**：契约面 `core.wit` 删 `interface abi`、双端 cap world 各删 `export abi`（`abi-form` 原样保留）、双端 `compose.json` 删 `abi.version`；运行期 `AbiPolicy` 退化为「只管形态」（`FormPolicy` + `AbiPolicy` 别名，桌面 `form = Some(1)` / 移动 `None`），删 `verify_abi` 的 version 比较分支与 `core_binding::abi_version` 探测，`test_support` 的 `plugin_abi_version` 转发面随之退役；SDK 面双端 `abi.rs` **文件删除**（台账已在票 01 迁入 CHANGELOG；桌面 `FORM_COMPONENT` 迁入新模块 `form.rs`），`wasm_entry!` 不再导出 `abi.version`、`wasm_entry_with_abi!` 宏退役，夹具 `plugin-system-test` 的 `impl abi::Guest` 改为 `abi_form`；共享产物侧 `SHARED_ABI_VERSION` 与其协商机制整体退役，`wiring_lock.rs` 判据 6 改写为「双端包版本对齐」+ 退役面不得复活。**四方必测不变量收敛为两方**（compose 版本 == 源文件 package 行，由版本锁承担），端到端实例化再无运行期版本协商——版本不等由 linker 的含版本 import 名匹配直接拒绝。验证：wasm-core `cargo test --lib` 574/575、file-transfer-core 76/76、版本锁 5/5、双端 SDK 与双端宿主 `cargo check` 绿、`pnpm run check:wit` 9/9；变异自检 1/1（注入已退役的 `SHARED_ABI_VERSION` → 新判据红并点名退役机制）。**遗留**：`a03_p2_fuel_async_semantics` 红——该探针依赖 wasip3 fixture 重建后的产物字节与 fuel 计量敏感，实验证明**与本次退役无因果**（临时恢复 `interface abi` 后数值完全不变），列入票 06 全量重建回归。
