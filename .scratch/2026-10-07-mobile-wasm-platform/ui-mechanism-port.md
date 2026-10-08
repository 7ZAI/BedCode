# 移动端前端：旧机制 / 公共组件 → 宿主壳 落地记录

> 状态：已实现（2026-10-08）· 与 `host-shell-spec.md` 同属移动端 wasm 平台专项
> 规则来源：AGENTS.md §6「移动端前端：优先对接宿主壳（强制）」
> 代码：`bedcode-mobile/src/shell/**`（组件库 `components/ui/`、机制 `composables/`）

---

## 1. 用户指令与裁决

- **指令**：把旧前端的机制与各公共组件**复制实现**进新前端界面，并在 AGENTS.md 加**强制规则**——
  以后移动端前端重构**优先对接新界面**，直到彻底替换旧的。
- **裁决口径（本次落地采用）**：
  1. 新界面 = **宿主壳**（`src/shell/**`，路由 `/mobile/shell`），不另起目录。
  2. 复制范围 = **通用 UI 原语 + 平台机制**；**业务组件 / 业务机制不复制**——终端 / 文件 /
     会话 / 设备 / AI 等页面按 AGENTS.md §5.1 归各 wasm-app（已有的「应用内页面只做预留」裁决
     继续有效），复制进壳会变成要拆的第二份业务真源。
  3. 复制件**契约逐字一致**（props / emits / 插槽 / 行为），旧页面迁移时只换 import 路径。
  4. 壳内从此**不依赖旧目录**：由 L7 锁拦截回接；缺件先复制进壳再改。

## 2. 落地清单

### 2.1 公共组件库 `src/shell/components/ui/`（9 件）

| 壳内文件 | 旧来源 | 迁移方式 |
| --- | --- | --- |
| `Button.vue` | `src/components/Button.vue` | 逐字复制（补 prop 默认值消 lint warning） |
| `Toggle.vue` | `src/components/Toggle.vue` | 逐字复制（同上） |
| `Modal.vue` | `src/components/Modal.vue` | 逐字复制（同上） |
| `ConfirmDialog.vue` | `src/components/ConfirmDialog.vue` | 复制；loading 转圈白边改 token 派生；移除未用的 `useI18n` |
| `PromptDialog.vue` | `src/components/BottomSheet.vue` | 复制 + **仅重命名**（旧名与实现不符：它是居中输入弹窗，不是底部抽屉） |
| `LoadingDialog.vue` | `src/components/LoadingDialog.vue` | 逐字复制 |
| `CollapseSection.vue` | `src/components/CollapseSection.vue` | 逐字复制 |
| `QuickActionButton.vue` | `src/components/QuickActionButton.vue` | 复制；hover 光晕改 `--shell-action-glow` |
| `LetterAvatar.vue` | `src/components/LetterAvatar.vue` | 复制；六组渐变移入 `styles/shell.css` 的 `.shell-avatar-g*` |

`index.ts` = 出口 + 迁移映射表（新复制件必须同步登记）。

### 2.2 平台机制 `src/shell/composables/`（5 件）

| 壳内文件 | 旧来源 | 迁移方式 |
| --- | --- | --- |
| `useToast.ts` | `src/composables/useToast.ts` | 复制；`ToastOptions` 内联（不再依赖旧 `@/composables/model`） |
| `usePlatform.ts` | `src/composables/usePlatform.ts` | 复制；**三处差异见 §3.2** |
| `useOrientation.ts` | `src/composables/useOrientation.ts` | 逐字复制（含 `useBreakpoints`） |
| `useSwipeTabs.ts` | `src/composables/useSwipeTabs.ts` | 逐字复制 |
| `useViewportPanGuard.ts` | `src/composables/useViewportPanGuard.ts` | 逐字复制 |

### 2.3 壳内消费方切换（切断对旧目录的依赖）

`ShellAppDetailScreen.vue`（ConfirmDialog / Toggle / useToast）、`ShellAppsScreen.vue`（useToast）、
`ShellStatusbar.vue` 与 `ShellSettingsScreen.vue`（usePlatform）——切换后 `src/shell/**` 对
`@/components|@/composables|@/views` 的 import 数归零（实测 grep 0 命中）。

### 2.4 门禁与锁

- **L7 锁**（`src/__tests__/shell/shellConstraintLocks.test.ts`）：壳内不得 import
  `@/components|@/composables|@/views`；`BRIDGE_ALLOWLIST`（当前为空）为唯一桥接出口；
  同锁正面钉住 9 组件 + 5 机制文件在场（防「删文件绕过依赖锁」）。
- **行为契约测试**：`src/__tests__/shell/shellUiLibrary.test.ts`（27 例，覆盖 9 组件分支：
  disabled/loading 守卫、变体映射、toggle 取反、loading 期间确认/关闭抑制、背板关闭策略、
  纯空白不提交、initialValue 回填、折叠切换、可见性、哈希稳定与分布、token 光晕）、
  `src/__tests__/shell/shellMechanisms.test.ts`（16 例：sonner 分发与位置/时长映射、
  滑动阈值 48 边界与垂直主导、平台探测四态、pan 守卫五分支含 dispose）。

## 3. 与旧实现的差异（全部显式记账，未记账的差异即 bug）

### 3.1 token 化（L6 锁 + frontend-styles 纪律要求，行为不变）

| 件 | 旧写法 | 壳内写法 | 理由 |
| --- | --- | --- | --- |
| ConfirmDialog 转圈 | `border-white/30 border-t-white` | `color-mix(--mobile-text-on-accent 30%)` + `border-t-[--mobile-text-on-accent]` | 白/黑是 Tailwind 调色板值，主题反转后失去对比 |
| QuickActionButton 光晕 | `rgba(34,211,238,.08)` | `--shell-action-glow`（`styles/shell.css`，强调色派生） | 硬编码青色与用户色板脱节 |
| LetterAvatar 渐变 | 组件内 6 组 hex | `styles/shell.css` 的 `.shell-avatar-g0..g5` | 颜色落点只能是样式表（L6 排除该文件） |

### 3.2 usePlatform 三处差异

1. 类型（`PlatformInfo` / `Platform` / `Arch`）内联——壳不依赖旧类型集合。
2. **不再读 `window.__TAURI__`**（L1 锁禁止裸读 Tauri 全局）：改为直接调 `plugin-os`，
   且**返回值必须落在已知平台名集合内**才认；否则（IPC 桥缺失 / 返回 undefined / 抛错）
   一律回到浏览器模拟模式。真值是插件调用本身，不依赖任何注入全局。
3. 探测失败补 `logger.warn` / `logger.log`——旧实现靠内联判断，失败不可观测；
   回退值与旧实现相同（桌面 / 非移动）。

## 4. 待迁清单（本批未复制，理由逐条）

| 待迁 | 归属判定 | 为什么不在本批 |
| --- | --- | --- |
| `Terminal*`（Header / InputBar / Settings / Shortcut* / InputAssistant / InputBar） | 终端业务 | 票 15 终端 UI 域下沉插件工程；复制进壳 = 第二份业务真源 |
| `DeviceCard` / `Session*` / `ScanPanel` / `PairingInput` / `BiometricAuthDialog` | 设备与会话业务 | 同上，归各应用运行面 |
| `FileExplorer` / `FileSidebar` / `FileTreeItem` / `FileViewerModal` / `icons/*` | 文件业务 | 同上 |
| `TaskPickerModal` / `TaskEditDialog` / `PresetTaskCard` / `RepeatableToggle` | 任务业务（文案即任务语义） | 任务域已并入 terminal-session 插件前端 |
| `PluginIcon` | 插件形态细节 | 壳内已有 `ShellAppIcon`（四级回退同形） |
| `MobileLayout` / `MobileNav` / `MobileSwipeContainer` | 布局导航 | 壳自带屏幕栈 + Tabbar，形状不同（不复用旧导航语义） |
| `SplashScreen*` / `FgService` / `Notification` / `EdgeToEdge` / `AndroidFeatures` | 平台机制（全局单例 / 启动时序） | **全局单写者**：现由 `App.vue` 等旧壳单点持有，复制会造成双写（主题 / 字号 / 通知 / 前台服务），退役旧界面时随批迁入 |
| `SettingsSubPage` | 布局脚手架（依赖 `vue-router` 返回） | 等壳内二级页导航语义（屏幕栈）定型后再落，避免复制一个走旧路由的脚手架 |
| `utils/clipboard` / `stores/settings` / `utils/frontendLogger` | 共享基础设施 | 跨新旧共用；复制会产生双攒批 / 双设置真源，属「共享基础设施例外」 |

## 5. 验证证据

- 壳内定向：`pnpm exec vitest run src/__tests__/shell` → **7 文件 / 103 用例全绿**（含新增 2 文件 43 例）。
- 全量：`NODE_OPTIONS=--max-old-space-size=6144 pnpm run test:run -- --maxWorkers=2` →
  **67 文件 / 725 用例全绿**（首轮 1 例 `useMdnsDiscovery` 高负载 flaky，单跑与复跑均绿，与本次改动无关）。
- 类型：`vue-tsc --noEmit` 新增文件 0 error（`EgressSettingsView.vue` 6 处为并行会话在途基线）。
- Lint：`pnpm exec eslint bedcode-mobile/src/shell` → **0 error 0 warning**；
  根 `pnpm exec eslint .` → 唯一 error 位于 `plugins/terminal-session/src/terminal/composables/useTuiCompat.ts:87`
  （并行在途文件，非本任务范围）。
- **变异自检 5/5**（注入 → 转红 → 还原 → 复绿）：
  1. 壳内 import `@/components/Button.vue` → L7 红（1 failed）
  2. `ConfirmDialog` loading 守卫取反 → 组件用例红
  3. `useSwipeTabs` 阈值 48 → 0 → 边界用例红
  4. 壳内追加 `#ff0000` 字面量 → L6 红
  5. 删除 `LetterAvatar.vue` → L7「复制面在场」守卫红
  还原后复跑：103/103 绿，且 `grep MUT_COLOR|LegacyButton` 零残留。
- 未跑：`cargo test` / `cross-end-tests` / gradlew（零 Rust / 协议 / Kotlin 改动）、真机核验
  （壳内组件尚未被真机入口大量使用，留随壳入口切换时验收）。

## 6. 后续（规则生效后的默认动作）

1. 移动端前端新功能 / 重构 → 落 `src/shell/**`，缺件先复制进 `ui/` 或 `composables/`。
2. 旧界面只修缺陷；修到共用能力时同步把该能力复制到壳（保持两份同形）。
3. 旧页面退役前提：新壳等价物可用；并存期以新壳为主入口（设置页 `shell.entry.*`）。
4. 新复制件三件套：`ui/index.ts` 映射表登记 + L7 在场清单登记 + code-map 锁索引更新。
