# BedCode Dev Shell（移动端插件浏览器开发环境）

空壳宿主 + 移动端页面骨架：在浏览器中运行插件前端源码，支持 HMR，
无需构建、打包、真机安装即可迭代 UI 与前端逻辑。

## 使用

在插件工程目录运行：

```bash
npm run dev                   # 推荐：npm 脚本自动解析 node_modules/.bin（Windows 必需）
npx bedcode-plugin dev        # 或 npx 方式（等效）
bedcode-plugin dev            # 仅 macOS/Linux（PATH 含 .bin 时）
```

> **Windows**：cmd / PowerShell 直接输入 `bedcode-plugin` 会报"不是内部或外部命令"，
> 因为 Windows 不把 `node_modules/.bin` 加入 PATH——请用 `npm run dev` 或 `npx`。

带参数示例：

```bash
npx bedcode-plugin dev --entry src/custom-entry.ts
npx bedcode-plugin dev --port 5180 --open
```

浏览器打开 `http://localhost:5173`（--open 自动打开）。

### 在手机上查看页面

```bash
bedcode-plugin dev --host     # 监听局域网（默认 0.0.0.0）
```

手机与电脑连同一 WiFi，手机浏览器打开 `http://<电脑IP>:5173/` 即可在真机浏览器中
查看页面（真实触控、真实视口）。建议：

- 点工具条「手机框：关」切换全宽渲染，去掉模拟手机框后的布局即真实移动端布局
- 查看电脑 IP：`ipconfig`（Windows）或 `ifconfig`（macOS/Linux）；也可用手机扫码
  工具条无二维码，需手动输入
- 手机与电脑不在同一网段、或公司防火墙限制时可能连不上，改用真机安装验证

> 注意：手机上运行的仍是 mock 宿主（WASM 后端命令、真实 WS 等依然不可用），
> 适合快速看布局/交互；完整能力验证仍需安装到 BedCode Mobile。

### 手动启动（不走 CLI）

```bash
BEDCODE_DEV_PLUGINS="<插件目录>[::<入口文件>]" npx vite --config <dev-shell>/vite.config.ts
```

多个插件用逗号分隔（如 `a::a/src/index.ts,b::b/src/index.ts`），
骨架的「工具箱」会并列展示所有插件的注册项。

## 骨架提供的能力

| 区域 | 说明 |
|---|---|
| 手机框 | 390×844 手机尺寸（工作台右上角开关），关闭后全宽便于 DevTools 模拟 |
| 底部导航 | 内置三项 + 插件 `ui.registerNavTab` 动态追加 |
| 工具箱 | 插件 `registerToolboxPage` 入口网格（含自定义 entry 卡片） |
| 模拟终端 | 输入发送（触发 `onTerminalInput`）、模拟输出（触发 `onOutput`/`onTerminalOutput`）、新建/停止会话、连接/断开、认证成功（触发对应 lifecycle）；插件终端工具栏项渲染在顶部；底部展示 mobileApi 任务队列 mock |
| 插件页 | 状态徽章（激活/错误）、激活/停用、设置区/路由/文件服务挂载一览 |
| 日志面板 | `context.logger` + 生命周期 + 加载错误，右下角浮层，warn/error 过滤 |
| 对话框 | `context.dialogs` 全量实现（dialog/confirm/prompt/toast），移动端样式 |

## Mock 边界（与真机差异）

| API | 浏览器行为 |
|---|---|
| `commands.execute` | 仅执行插件内 `register` 的前端 handler；**WASM 后端命令不可用**（记 warn 日志），需真机验证 |
| `terminal` / `session` / `lifecycle` | 由模拟终端页面驱动，事件名与宿主一致 |
| `storage` | localStorage 持久化（`bedcode-dev-shell:{pluginId}:{key}`） |
| `fileService` | mount 为内存注册表（插件页可见）；pick 系列弹输入框返回模拟路径；`getPeerInfo` 返回 null |
| `notifications` | 浏览器 Notification（未授权时降级 toast） |
| `getMobileApi()` | 完整实现，队列数据持久化到 localStorage |
| `getPresetTasks()` | tasks 本地持久化；`sendTask`/`executeTask` 需对端桌面端，浏览器不可用（记 warn） |
| 权限检查 | dev-shell 跳过（视为全部授予），权限逻辑需在真机复核 |

## 插件领域数据（devMock 协议）

上表中的 mock 都是**宿主能力**（会话/对话框/事件/HTTP 接口，固定在 dev-shell 内实现）；
各插件自己的**业务演示数据**（队列种子、SAF 目录树、目录浏览条目）由插件入口导出 `devMock`
（SDK 类型 `PluginDevMock`），dev-shell 加载插件时按 pluginId 注册、`createMockContext` 按需合并：

| 字段 | 消费方 | 插件 |
|---|---|---|
| `queueSeed` | `mobileApi` 初始任务队列（localStorage 无缓存时） | auto-task |
| `safTree` | `fileService.saf.listTree` 目录树 | file-transfer |
| `listDirEntries` | `fileService.listDir` 返回条目（uri 由 mock 宿主拼装） | file-transfer |

- 未注册 devMock 的插件访问对应能力时返回空列表/空树（不报错），适合仅调试单插件
- 真实宿主忽略 `devMock` 导出（`PluginModule` 多余字段对 `activate` 无影响），无需条件编译
- 停用插件时 devMock 随 `registerDevMock` 的 Disposable 一并清理

## 常见问题

- **插件样式缺失**：插件 SFC 使用宿主 Tailwind 工具类，dev-shell 的
  tailwind.config.js 已按 `BEDCODE_DEV_PLUGINS` 动态扫描插件源码，
  新加类名会自动生效（无需重启）。
- **`window.__BEDCODE_SHARED__` 未初始化**：确认经由 `bedcode-plugin dev` 或
  dev-shell 的 main.ts 启动，且未在入口前直接 import 插件模块。
- **SDK 报找不到模块**：插件工程的 `@bedcode/plugin-sdk-mobile` 依赖指向
  SDK 包（file: 或 npm），其 `dist` 需存在（`npm run build` 一次）。
- **真机专属能力**（WASM 命令、真实 WS、SAF 文件选择、系统通知）无法在
  浏览器模拟，发布前仍按 `../../../plugin-dev-mobile.md` 验证清单过真机。
