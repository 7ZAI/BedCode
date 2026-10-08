# 移动端前端宿主壳（Host Shell）落地说明

> 状态：已实现（2026-10-08）· 与既有 `/mobile/**` 实现并存，未替换任何既有页面
> 原型：`.scratch/2026-10-07-mobile-wasm-platform/prototype.html`（交互原型 v0.3）
> 代码：`bedcode-mobile/src/shell/**`

---

## 0. 本次裁决：应用内页面只做预留

原型里的 **终端 / AI Chatbox / 文件传输三个界面属于各 wasm-app 自己的页面**，
本次**不实现**，只保留挂载位。理由：这些界面是产品语义，按 AGENTS.md §5.1 应归应用自持；
平台提前实现一份，等真实 wasm-app 接入时会变成要拆的第二份真源。

对应的落点：

| 原型屏 | 本次实现 | 将来由谁填 |
| --- | --- | --- |
| s-terminal / s-aichat / s-files（应用内页） | `ShellPlaceholderSurface.vue` 预留位 + 说明 | wasm-app 注册 surface 后自动顶掉 |
| 首页快捷卡片（会话数 / 传输进度 / 引用句） | `ShellDefaultSlotCard.vue` 兜底（只显示平台掌握的事实） | wasm-app `registerSlot` 后自动让位 |
| 多任务卡内的终端输出 / 气泡 / 进度条 | 不渲染（平台无应用内部状态） | 应用若提供快照贡献点再扩展契约 |

预留位写明了「这不是加载失败，是等待应用注册运行面」，避免被当成 bug。

---

## 1. 分层（高内聚低耦合）

```text
components/  壳内 UI（屏 / 分组卡 / 胶囊 / 抽屉 / 图标）
    ↓ 只依赖
composables/ 视图模型（应用清单投影、屏幕栈、覆盖层、最近使用）
    ↓ 只依赖
registry/    扩展点总线（注册 / 撤销 / 查询，无 UI、无数据源知识）
    ↑ 写入
adapters/    数据源适配（当前：插件系统；将来：wasm-app 清单）
    ↓ 依赖
types.ts     契约（ShellApp / ShellAppSource / 贡献点）
```

三条硬边界：

1. **壳不认识插件**：`src/shell/**` 除 `adapters/pluginAppSource.ts` 外，不 import 任何 `@/plugin/**`。
2. **插件不认识壳**：插件系统零改动，壳只是它的一个新消费者。
3. **UI 不认识数据源**：组件只调 `useShellApps()` 的动作，不知道背后是谁。

---

## 2. 扩展点（面向未来的 wasm-app）

统一入口 `src/shell/index.ts`，返回值都是 `Disposable`（应用停用时 dispose 即摘除）：

```ts
import { registerSurface, registerSlot, registerCapsuleItem, registerSettingsEntry } from '@/shell'

// ① 运行面：应用在壳内被打开时渲染的界面（应用内页面全在这里）
registerSurface(appId, { component: MyAppView, accent: '#8BE9FD' })

// ② 首页快捷卡片：应用自持内容（如「N 个活跃会话」）
registerSlot(appId, { id: 'status', component: MyStatusCard, order: 10 })

// ③ 胶囊附加项：平台项 order 为 权限 10 / 停用 30，应用可用中间值插入
registerCapsuleItem(appId, { id: 'settings', label: '应用设置', order: 20 })

// ④ 平台设置入口
registerSettingsEntry(appId, { id: 'account', label: '账号', onSelect: open })
```

换真源的唯一改动点：实现 `ShellAppSource` 并在 `ShellView.vue` 换一行注册。

```ts
interface ShellAppSource {
  readonly id: string
  list(): Promise<ShellApp[]>
  launch(appId): Promise<void>
  stop(appId): Promise<void>
  setPermissionGrant?(...)   // 不支持返回 false
  resolveSurface?(appId)     // 延迟解析运行面
  installFromLocalPackage?() // 不支持则不渲染入口
  remove?(appId)
}
```

**能力不撒谎**：可选能力未实现时返回 `false` / `undefined`，UI 置灰并说明，
不做「点了就算通过」的假成功（权限尤其如此）。

---

## 3. 与既有实现的关系

- 新增路由 `/mobile/shell`（`mobile-shell`），既有路由与页面一行未改。
- `MobileLayout.vue` 只新增了一条「自带底部导航的路由」名单（terminal + shell），
  让平台底部导航在壳内让位。
- 入口：既有设置页「系统」分组新增一行「WASM 应用平台（宿主壳）」跳转到壳；
  文案放在 `locales/{zh-CN,en}/shell.ts` 的 `shell.entry.*`（不写进 settings.ts，
  该文件有并行会话在途改动）。切默认入口前，旧 UI 完全不受影响。
- 应用真源当前是插件系统：启动走 `pluginPreauthorize` → `pluginLoader.activate`，
  停止走 `pluginLoader.deactivate` + 后端停用，与既有插件页口径一致。
- 外观 / 通知 / 关于沿用既有设置页路由，壳不另造一套主题配置。

---

## 4. 已知缺口（不是 bug，是待裁决）

| 项 | 现状 | 需要的后端能力 |
| --- | --- | --- |
| 权限逐项开关 | 置灰 + 「后端不支持」说明 | per-permission grant 命令 |
| 授权记录 | 空态（不编造示例） | grant log 查询 |
| 应用数据占用 / 缓存 | 数据占用来自插件目录大小，缓存显示「—」 | 缓存统计 |
| 应用级连接 | 只显示运行态，注明「连接由应用独立持有」 | 应用持有连接的平台视图 |
| 「发现更多应用」 | 未渲染（无数据源能力） | 应用市场/仓库接口 |

原型「待裁决问题」里的首页槽位上限、高风险权限二次确认、浅色模式 app accent
归属，均未在本次拍板，待评审。

---

## 5. 验证

- `vue-tsc --noEmit`：新增代码 0 error（`EgressSettingsView.vue` 的 6 处在途 error 为基线，非本次引入）
- `vitest run`（端目录）：64 files / 670 tests 全绿，其中 `src/__tests__/shell/**` 新增 48 例
- 变异自检：注册表回收、权限「其他」组兜底、导航回退到既有层 三处注入变异均被杀死
- `eslint`：新增文件 0 error
