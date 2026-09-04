# BedCode Dev Shell（桌面端插件浏览器开发环境）

空壳宿主 + 桌面端页面骨架：在浏览器中运行插件前端源码，支持 HMR，
无需构建、打包、真机安装即可迭代 UI 与前端逻辑。

## 使用

### 前提

- Node ≥ 20
- 插件工程已声明 `@binblink/plugin-sdk-desktop` 依赖（`file:` 本地链接或 npm）
- SDK 构建产物存在：未构建时先在 SDK 目录执行 `pnpm install && pnpm run build`
  （报找不到模块多半是这一步没做）

### 启动（推荐走 CLI）

`dev-shell` 由 SDK 提供的 CLI 启动，统一用 **`pnpm exec bedcode-plugin-desktop dev`**：
在插件工程目录运行（bin 来自插件的 SDK 依赖，勿省略 `npx`）：

```bash
cd bedcode-desktop/plugins/<your-plugin>
pnpm exec bedcode-plugin-desktop dev            # 使用当前目录作为插件
pnpm exec bedcode-plugin-desktop dev --entry src/custom-entry.ts
pnpm exec bedcode-plugin-desktop dev ../my-plugin --port 5180 --open
pnpm exec bedcode-plugin-desktop dev --host     # 监听局域网（平板/其他设备浏览器访问）
```

也可在仓库任意位置指定插件目录：

```bash
pnpm exec bedcode-plugin-desktop dev bedcode-desktop/plugins/file-transfer
```

浏览器打开 `http://localhost:5173`（`--open` 自动打开）。首次运行会自动安装
dev-shell 自身依赖（vue / vite / tailwind 等，仅一次）。

#### 参数

| 参数 | 说明 |
|---|---|
| `[pluginDir]` | 插件工程目录，默认当前目录（须含 `plugin.json` 或入口文件） |
| `--entry <file>` | 插件入口文件，默认 `<pluginDir>/src/index.ts` |
| `--port <port>` | dev server 端口，默认 5173 |
| `--host [addr]` | 监听局域网（默认 `0.0.0.0`），手机/平板与电脑同一 WiFi 时访问 `http://<电脑IP>:<port>/` |
| `--open` | 启动后自动打开浏览器 |

环境异常时先自检：`pnpm exec bedcode-plugin-desktop doctor`（检查 Node / Rust /
wasm32 target / dev-shell 依赖 / SDK 构建产物）。

### 手动启动（不走 CLI）

```bash
BEDCODE_DEV_PLUGINS="<插件目录>[::<入口文件>]" pnpm exec vite --config <dev-shell>/vite.config.ts
```

多个插件用逗号分隔，侧边栏会并列展示所有插件的注册项。

## 骨架提供的能力

| 区域 | 说明 |
|---|---|
| 标题栏 | 插件 `registerTitleBarItem` 渲染在右上角 |
| 侧边栏 | 内置导航 + 插件 `registerSidebarPanel`（按 `order` 排序，与宿主一致） |
| 工具箱 | 插件 `registerToolboxPage` 入口网格 |
| 模拟终端 | 输入发送（触发 `terminal.onInput`）、模拟输出（触发 `onOutput`）、会话创建/停止、连接/断开；插件终端工具栏项 + 输入扩展渲染在顶部 |
| 插件页 | 状态徽章、激活/停用、全部注册项一览（文件处理器 / HTTP 端点 / 挂载等） |
| 状态栏 | 连接状态 + 插件 `registerStatusBarItem` |
| 日志面板 | 标题栏按钮开关，warn/error 过滤 |
| 主题 | 设置页切换 `html.dark`（与宿主类名机制一致，插件深浅色可直接验证） |

## Mock 边界（与真机差异）

| API | 浏览器行为 |
|---|---|
| `commands.execute` | 仅执行插件内 `register` 的前端 handler；**Rust 后端（WASM）命令不可用**（记 warn 日志），需真机验证 |
| `terminal` / `session` | 由模拟终端页面驱动，事件名与宿主一致 |
| `storage` | localStorage 持久化（`bedcode-dev-shell:{pluginId}:{key}`），`flush()` 空操作 |
| `http.registerEndpoint` | 仅登记展示（真实宿主由 Rust 服务端挂载，浏览器不可达） |
| `fileService` | mount 为内存注册表；pick 系列弹输入框返回模拟路径；`getPeerInfo` 返回 null |
| `i18n` | `getI18n()` 返回 dev-shell i18n 实例；`registerMessages`/`t` 自动加插件 ID 前缀（与宿主一致） |
| 权限检查 | dev-shell 跳过（视为全部授予），权限逻辑需在真机复核 |

## 常见问题

- **插件样式缺失**：dev-shell 的 tailwind.config.js 按 `BEDCODE_DEV_PLUGINS`
  动态扫描插件源码，新加类名自动生效。
- **`window.__BEDCODE_SHARED__` 未初始化**：确认经由 `pnpm exec bedcode-plugin-desktop dev`
  或 dev-shell 的 main.ts 启动。
- **SDK 报找不到模块**：插件工程的 `@binblink/plugin-sdk-desktop` 依赖指向
  SDK 包（file: 或 npm），其 `dist` 需存在（先构建一次 SDK）。
- **真机专属能力**（Rust 命令、真实 HTTP 端点、系统文件选择）无法在浏览器
  模拟，发布前需在真实宿主验证。
