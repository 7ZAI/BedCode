# 移动端：应用运行期 store 作用域（停止应用 = store 停止运行）

> 用户指令（2026-10-14）：**移动端前端停止应用，即是停止 store 的运行、退出；包括多任务时上滑退出；
> 再次点击是启动 store（全新）。**
>
> 背景：同日审计（`.dev-logs/android-dev.2026-10-14.log` + 代码走读）发现移动端应用「关闭后再进」
> 仍维持旧状态——原因是三层状态里前端（Pinia store + ESM 模块）与后端（WASM 实例）都不随停用复位。
> 本票只解决**前端运行期状态**：停止应用即作废该次运行期的 store；再次启动是全新运行期。
>
> 状态：**已实施**（2026-10-14）。实现落点与验证证据见文末「实施记录」。

---

## 0. 事实基线（2026-10-14 实测）

| 事实 | 证据 |
| --- | --- |
| 停止应用会走到后端 `deactivate`（资源回收是闭环的） | 日志 20:38:47.968→48.07：guest `[File Transfer] plugin deactivated` → bus 订阅全撤 + mdns browse stop + peer node stop → `manager: Plugin deactivated` → `[PluginLoader] Plugin deactivated` |
| 再次打开会重新 activate（同一 WASM 实例） | 日志 20:38:49.069→49.112：bus 重订阅 + mdns/peer 重起 + `WASM plugin activated` + `[PluginLoader] Plugin frontend loaded` |
| **前端 Pinia store 跨停用保留** | `wasm-apps/terminal-session/src/terminal/store.ts:123` `defineStore('terminalBuffer')`；`activate.ts` 只做样式/事件桥回收，没有任何 store 作废；store 实例活在宿主 Pinia（`src/plugin/shared-runtime.ts` 把 `pinia` 模块整个交给插件） |
| **前端模块级状态跨停用保留** | 插件前端经动态 `import(url)` 装载（`src/plugin/loader.ts` `importWithTimeout`），URL 不含版本参数 → ESM 模块缓存命中，模块求值只发生一次 |
| 组件实例本身是全新的 | `ShellHost.vue` 用 `<component :is :key>` 渲染屏幕，无 KeepAlive；`useShellApps.stop()` 会 `closeAppRun(appId)` 摘运行屏 |
| 停用入口只有两处：多任务上滑/×、应用详情「停用」；两处都调 `stop()` | `ShellSwitcherScreen.vue:467-485`（`dismiss` / 全部关闭）、`ShellAppDetailScreen.vue`（停用按钮）→ `useShellApps.stop()` |
| 「回主屏 / 系统返回」**不**停止应用（Android home 语义，保留） | `ShellAppRunScreen.vue` 胶囊 `nav.goHome()`；`useShellNavigation.back()` |
| 卸载**不**回收前端（只调后端 `pluginUninstall`） | `src/shell/adapters/pluginAppSource.ts` `remove()`：只 `pluginUninstall(appId)`；前端模块/注册/store 残留到页面重载 |
| 后端 WASM 实例跨停用复用，插件须在 `deactivate` 自清状态 | `bedcode-mobile/src-tauri/src/plugin/manager.rs:861`（停用只调 guest `deactivate`，实例留在 `wasm_plugins`）；`ai-chatbox/rust/src/lib.rs:63-70` 注释即为该契约的既有实现（「同一进程内停用后重新激活可再次初始化（宿主复用 WASM 实例）」） |
| 插件 Pinia 版本 2.3.1（支持 `useStore(pinia)` 显式传参与 `store.$dispose()`） | `bedcode-mobile/package.json` + `node_modules/pinia/package.json` |

---

## 1. 设计

### D1 「应用运行期」= 一次 activate .. deactivate

- 运行期由 `pluginLoader.activate()` 开启、`pluginLoader.deactivate()` 结束（多任务上滑 / 详情停用 / 卸载
  都必须经过后者——卸载见 D3.3）。
- 运行期拥有的前端资源：模块（及其模块级状态）、**store 作用域**、context disposables、注册项与事件。
- 「回主屏 / 系统返回」不是停止，不结束运行期（保持不变）。

### D2 store 作用域 = 每次运行期一个 Pinia，停止即作废

- 新增宿主模块 `bedcode-mobile/src/plugin/app-run-scope.ts`：
  - `scope(pluginId)`：取该插件**当前运行期**的 `Pinia`（惰性创建）。
  - `scopedDefineStore(id, setup|options)`：包装 Pinia 的 `defineStore`。包装后的 `useStore()` 恒把
    「该插件当前运行期的 Pinia」作为**显式实参**传入（`useStore(pinia)`），因此组件、`activate` 期调用、
    事件回调里的 `useXxxStore()` 都落在同一作用域——不依赖 `activePinia` 全局态。
  - `disposeAppRunScope(pluginId)`：对作用域内已创建的 store 逐个 `$dispose()`（停止其 effect、从
    Pinia 摘除），随后丢弃整个作用域。再次 `useStore()` 会创建新 store（回到初始 state）。
- 作用域**按插件隔离**：两个插件用同名 store id 互不影响；宿主与其它插件的 store 不受影响。

### D3 停止路径（三处收口）

1. **`pluginLoader.deactivate(pluginId)`**（覆盖多任务上滑 / × / 全部关闭 / 详情停用）：
   现有步骤（disposables → `module.deactivate()` → 事件与注册表清理 → 后端 `pluginDeactivate`）不变，
   在**后端停用之后**追加：`await nextTick()`（让运行面组件先随注册表清理卸载，避免把半死的 store
   留在仍在渲染的组件上）→ `disposeAppRunScope(pluginId)`。
2. **前端装载失败路径**：`loadFrontend` 的 catch 里同样作废作用域（模块求值到一半失败可能已建 scope）。
3. **卸载**：`pluginAppSource.remove()` 先 `pluginLoader.deactivate(appId)`（前端对称拆解 + 后端停用），
   再 `pluginUninstall(appId)`。后端 `uninstall` 内部也会 deactivate，但那只回收 Rust 侧；前端必须在此
   收掉，否则应用卸载后其运行面与 store 会残留到页面重载。

### D4 绑定规则（defineStore 的归属如何确定）

- 插件 `defineStore` 调用发生在**模块求值期**（顶层 `const useX = defineStore(...)`，现状全部如此）。
  宿主在 `import()` 外开「装载窗口」记录当前插件 id；包装器在该窗口内把 store 定义**绑定到插件**，
  之后每次 `useStore()` 解析该插件的*当前*运行期作用域——所以模块被 ESM 缓存、只求值一次也不影响
  第二次运行期拿到新作用域。
- 装载窗口**串行化**（`loadPluginModuleScoped` 内部 promise 链）：并发装载会让 A 的模块求值撞上 B 的
  窗口，把 store 绑到错误插件。
- 模块顶层之外的 `defineStore`（如异步 chunk 里）**不做猜测**：回退到宿主 Pinia 并 `warn`
  （fail-visible）。SDK 文档写明「store 定义必须在模块顶层」。

### D5 模块级可变状态 = 插件契约（宿主不重装载模块）

- 宿主**不做** URL cache-busting：重求值会复制模块级副作用（事件监听、样式注入等），对第三方插件不可控；
  内存也会随运行次数增长。
- 契约：模块级可变单例状态必须由插件在 `deactivate` 里清空（或挂到 `context` disposable 上），
  跨 activate 保留属插件缺陷。现状核对：file-transfer 的 `useBatchDialog` / `useConsent` 在 `stop()`
  里已清空（`wasm-apps/file-transfer/src/composables/useBatchDialog.ts:254-265`、`useConsent.ts`
  对应 `stop()`）；terminal-session 的模块级状态只有 store（由 D2 覆盖）。

### D6 非目标：后端 WASM 实例不销毁

- 移动宿主停用只调 guest `deactivate`，实例/`Store` 保留（`manager.rs:861`；桌面同构），
  销毁+重建意味着每次打开应用都要重新实例化（Android 上编译/实例化成本高）。
- 现有契约（AD-项）就是「宿主复用实例、插件在 `deactivate` 自清」——ai-chatbox 的 `DATA_DIR` 清理
  即是先例。本次前端修复与该契约对齐，不新增后端行为。

---

## 2. 落点

| 文件 | 改动 |
| --- | --- |
| `bedcode-mobile/src/plugin/app-run-scope.ts` | **新增**：运行期作用域 + `scopedDefineStore` + `loadPluginModuleScoped` + `disposeAppRunScope` |
| `bedcode-mobile/src/plugin/shared-runtime.ts` | 暴露给插件的 `pinia` 模块换成「`defineStore` 为包装版、其余原样」的对象 |
| `bedcode-mobile/src/plugin/loader.ts` | 动态 import 走 `loadPluginModuleScoped`；`deactivate` / 装载失败路径作废作用域 |
| `bedcode-mobile/src/shell/adapters/pluginAppSource.ts` | `remove()` 先 `pluginLoader.deactivate` 再 `pluginUninstall` |
| `bedcode-mobile/packages/plugin-sdk-mobile/README.md`（+ `_en`） | store 作用域与模块级状态契约一段 |
| `bedcode-mobile/docs/code-map.md` | 插件系统一行补「运行期 store 作用域」 |
| 根 `CHANGELOG.md` / `CHANGELOG_zh.md` | 双语条目 |

---

## 3. 行为契约

| 契约ID | 来源 | 行为/规则 | 前置 | 预期 | 副作用 |
| --- | --- | --- | --- | --- | --- |
| C-01 | 用户指令 | 模块装载期定义的 store 归属该插件的运行期作用域 | 装载窗口内 `defineStore` | `useStore()` 在作用域内创建 store | 作用域登记该 store |
| C-02 | D2 | 停止（dispose）后再次使用 store 得到**初始 state** 的新 store | 已建 store 且已改 state | 新 store state = setup 初始值；旧引用不再被 Pinia 复用 | 旧 store `$dispose()` |
| C-03 | D2 | 两次运行期的 store 不共享状态 | run1 改 state → dispose → run2 | run2 读到初始 state | — |
| C-04 | D2 | 插件间作用域隔离 | A、B 各定义同名 store | 互不可见、互不影响 | — |
| C-05 | D4 | 并发装载各自绑定到自己的插件 | 同时发起 A、B 装载 | 两个 body 顺序执行（串行化）；各自的 store 落在各自作用域 | — |
| C-06 | D4 | 装载窗口外 `defineStore` 不进入任何插件作用域 | 无窗口 | 回退宿主 Pinia + `warn` | 日志 warn |
| C-07 | C-04/D2 | dispose 只作用于目标插件 | A、B 都有 store | dispose(A) 后 B 的 store 仍在原作用域、state 保留 | — |
| C-08 | D2 | 作用域内多个 store 全部作废 | 一个插件两个 store | dispose 后两者都是新实例、初始 state | — |
| C-09 | D2 | 单个 store `$dispose` 抛错不阻断其余 store | mock 一个 store 的 `$dispose` 抛 | 其余 store 仍被 dispose；错误只记日志 | 日志 error |
| C-10 | D3.1 | `pluginLoader.deactivate` 调用作用域作废 | loader 中已装载并激活 | dispose 被调用一次，且在后端 `pluginDeactivate` 之后 | — |
| C-11 | D3.2 | 前端装载失败也作废（半建作用域不残留） | import 失败 | 该插件无作用域／已作废 | — |
| C-12 | D3.3 | 卸载先结束运行期 | `removeApp` | `pluginLoader.deactivate` 在 `pluginUninstall` 之前 | — |
| C-13 | D2 | 幂等：未创建过作用域 / 重复 dispose 不抛 | 无作用域、连续两次 dispose | no-op / 第二次 no-op | — |
| C-14 | D4 | options 形式 `defineStore({id, ...})` 同样受作用域管理 | 装载窗口内 | 同上 C-01/C-02 | — |

---

## 4. 验收与测试计划

**单元（`src/__tests__/plugin/appRunScope.test.ts`，真实 pinia）**：C-01..C-09、C-13、C-14。

**集成（`src/__tests__/integration/pluginStoreScope.test.ts`，真实 loader + 真实 shared-runtime +
可导入的 mock 插件模块）**：

1. activate → 用 store 改 state → deactivate → activate → state 回到初始值（C-02/C-03/C-10 端到端）。
2. deactivate 后壳注册表运行面已摘除 + store 已作废（同一停用动作）。
3. 前端装载失败路径：无作用域残留（C-11，可并入现有 `plugin-flow` 断言风格）。

**回归**：移动端 `pnpm run test:run` 全量 + `pnpm exec eslint .` 0 error + `vue-tsc`（改动含 TS）。

**手工（未纳入自动化门禁的项）**：真机 / 模拟器验证「多任务上滑关闭 → 再次点击」terminal-session
回到初始页（无旧会话/旧页签）；无设备时在交付说明写明未跑。

**变异自检**（每条至少杀一个变异）：去掉 `disposeAppRunScope` 调用 → C-02/C-03/C-10 失败；
把「显式传 pinia」改回裸 `useStore()` → C-01/C-04 失败；去掉串行化 → C-05 失败；
把 dispose 放在 `pluginDeactivate` 之前会否失败？（顺序断言弱，不做强断言，仅注释说明）。

---

## 5. 风险与取舍

| 风险 | 处置 |
| --- | --- |
| dispose 发生在运行面组件卸载前，组件拿到半死的 store | `await nextTick()` 后再 dispose；组件本身随注册表清理卸载（同一停用动作） |
| 插件在 `deactivate` 后还有异步续体写 store | 新 `useStore()` 得到**新作用域**（初始 state），不会读到旧状态；不做额外兜底 |
| 插件把 store 定义写在异步 chunk | 回退宿主 Pinia + warn（fail-visible）；SDK 文档写明顶层定义 |
| 宿主自身 `getSharedModule('pinia')` 的消费方 | 宿主只用模块做插件共享，无自身 store 依赖；宿主自己的 Pinia 实例不受影响 |
| 插件间同名 store id | 作用域已按插件隔离，天然解决 |

---

## 6. 实施记录（2026-10-14）

**落点（与 §2 一致）**

- 新增 `bedcode-mobile/src/plugin/app-run-scope.ts`：`loadPluginModuleScoped`（串行装载窗口）/
  `scopedDefineStore`（作用域包装）/ `disposeAppRunScope` / `_resetAppRunScopesForTest`。
- `shared-runtime.ts`：暴露给插件的 `pinia` 换成 `defineStore` 包装版（其余导出透传）。
- `loader.ts`：`loadFrontend` 经装载窗口 import；`deactivate` 在 `pluginDeactivate` 之后
  `await nextTick()` → `disposeAppRunScope`；装载失败路径同样作废。
- `pluginAppSource.remove()`：先 `pluginLoader.deactivate` 再 `pluginUninstall`。
- 文档：SDK README 中英各补「应用运行期与 store」契约段；移动 code-map 插件系统一行。

**验证**

- 新增 `src/__tests__/plugin/appRunScope.test.ts`（9 条：运行期内复用 / dispose 后初始 state /
  跨运行期重置 / 插件隔离 / 装载串行化 / 并发绑定 / 窗口外回退 + warn / 单个 $dispose 抛错不阻断 /
  未知插件 no-op / options 形式）。
- 新增 `src/__tests__/integration/pluginStoreScope.test.ts`（2 条：停止后重启命中同一模块仍得新
  store；装载失败不残留作用域）+ `fixtures/mockStorePlugin.ts`。
- 扩展 `src/__tests__/shell/pluginAppSource.test.ts`：卸载先结束运行期（调用序断言）。
- 全量：移动端 `pnpm run test:run` 89 文件 / 1055 用例全绿；`vue-tsc --noEmit` 0 error；
  根 `pnpm exec eslint .` 0 error（警告 98 条均为既有）。
- 变异自检：删除 `disposeAppRunScope` 调用 → 集成用例 1 失败；`useStore(scope.pinia)` 改回裸
  `useStore()` + 去掉装载串行化 → 单元用例 5 条失败。
- 未跑（手工项）：真机 / 模拟器验证「多任务上滑关闭 → 再次点击回到初始页」——本机无
  Android 设备与运行中的应用，交付说明已注明；行为已由真实 loader × 真实 pinia 的集成用例覆盖。
