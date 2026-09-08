# Spec: 桌面端 WebDriver E2E + CI 回归门禁

Status: ready-for-agent

词汇表依据：CONTEXT.md「配对」「会话」「PTY」「插件」「终端链路」等节；桌面端 code-map.md 的「服务器」「会话管理」「PTY 管理」模块职责。
前置 spec：`.scratch/desktop-integration-tests/spec.md`（L1 Rust 集成 + L2 前端集成，已完成）——本 spec 是它的 L3 延续。

## Problem Statement

BedCode 桌面端的回归测试存在两个空白，导致「dev 阶段改坏的东西只能靠人肉发现，到 uat/master 才爆雷」：

1. **CI 没有测试门禁**。当前唯一的合并门禁 `lint.yml` 只跑 `pnpm exec eslint .`（前端风格校验），**完全不执行任何测试**。桌面端 Rust 572 个测试（含 8 个集成测试）、前端 vitest 358+、移动端 Rust 250 个测试 + 前端 vitest，全部依赖开发者本地「Done When」自觉执行——没有任何自动化兜底，漏跑即漏防。

2. **E2E 层纯预留、从未落地**。`package.json` 早已声明 `@playwright/test` + `test:e2e` 脚本，但无 config、无用例、无驱动，且方向本身是错的——Playwright 不支持驱动 Tauri 的 webkit2gtk webview（它走自家 CDP/内部协议，不是 W3C WebDriver 协议）。结果是「前端 invoke 封装 → Tauri IPC → Rust 命令」这条**真实进程内的完整链路**，以及**真实 UI 的用户操作路径**，两条回归线都没有自动化覆盖。

## Solution

用 Tauri v2 官方推荐的 **WebdriverIO + `@wdio/tauri-service`** 落地 E2E，并把 Rust 测试 + 前端 vitest 补进 CI 门禁：

1. **补全 CI 回归门禁**：在合并到 master/uat 时，CI 分层执行「lint → Rust 单测/集成（两端）→ 前端 vitest（两端）→ WebDriver E2E（桌面端）」，任一层失败即阻断合并。

2. **WebDriver E2E 分两翼**：
   - **API 功能验证**：通过 `browser.tauri.execute()` 在真实应用进程内调用 `invoke` 命令（`ping`、`list_sessions`、`start_session`、`generate_pairing_code` 等），断言返回形状与业务行为，覆盖 Rust 集成测试无法覆盖的「前端 invoke 封装 → IPC → 命令注册 → 后端执行」完整链路。
   - **UI 验证**：通过 W3C WebDriver 协议操作真实 webview（定位元素、点击、断言 DOM 与文案），覆盖关键用户路径的冒烟回归。

3. **驱动层用 external `tauri-driver`**（Linux 配 WebKitWebDriver + Xvfb 无头运行），不改应用的生产行为——测试专用 Rust 插件以 `cfg(debug_assertions)` 隔离。

## User Stories

### 开发者视角（回归门禁）

1. 作为开发者，我要 CI 在合并前自动跑桌面端与移动端的 Rust 测试，以便后端改动不会引入回归而不自知。
2. 作为开发者，我要 CI 在合并前自动跑两端前端 vitest，以便前端逻辑改动不会引入回归而不自知。
3. 作为开发者，我要 CI 在合并前跑 WebDriver E2E，以便真实进程内的 IPC 链路与 UI 交互得到验证。
4. 作为开发者，我要 E2E 能无头运行（不依赖真实显示器），以便 CI 服务器上稳定执行。
5. 作为开发者，我要 CI 失败时能一眼区分是 lint / Rust 测试 / 前端测试 / E2E 哪一层失败，以便快速定位。

### API 功能验证视角（browser.tauri.execute）

6. 作为测试者，我要通过 WebDriver 在真实应用内调用 `ping` 命令并断言返回，以便验证 IPC 链路通、应用可被驱动。
7. 作为测试者，我要通过 WebDriver 调用 `list_sessions` / `list_session_configs` 等查询命令并断言返回形状，以便验证命令在真实进程内正确注册且序列化正确。
8. 作为测试者，我要通过 WebDriver 走「创建会话配置 → 启动会话 → 断言运行状态 → 终止会话」完整链路，以便核心业务命令在真实应用内可用。
9. 作为测试者，我要通过 WebDriver 走「生成配对码 → 断言码结构 → 清除配对码」链路，以便配对能力回归可自动化。
10. 作为测试者，我要通过 WebDriver 调用服务器状态命令（`get_server_status` / `server_start` / `server_stop`）并断言状态流转，以便服务器生命周期回归可自动化。
11. 作为测试者，我要能 mock 某个命令的返回值，以便隔离测试前端逻辑而不依赖后端真实状态。

### UI 验证视角（WebDriver 协议）

12. 作为测试者，我要通过 WebDriver 启动应用并断言主界面关键元素渲染，以便确认应用能正常启动与加载。
13. 作为测试者，我要通过 WebDriver 点击 UI 元素并断言页面路由/DOM 变化，以便验证关键用户操作路径。
14. 作为测试者，我要通过 WebDriver 断言关键文案（i18n 标题、按钮、状态提示）存在且正确，以便 UI 文案回归可自动化。
15. 作为测试者，我要在 E2E 失败时能拿到失败时刻的页面快照/截图与前端控制台日志，以便定位 UI 问题。

### 维护者视角（CI 工程）

16. 作为维护者，我要 Rust 测试与前端测试分层、E2E 单独成 job，以便 CI 时间可控、失败可并行定位。
17. 作为维护者，我要 E2E job 只在相关路径变更时触发，以便避免无谓的 CI 开销。
18. 作为维护者，我要 E2E 依赖（WebKitWebDriver / Xvfb / tauri-driver）在 CI 里可缓存，以便缩短流水线耗时。

## Implementation Decisions

1. **E2E 技术栈采用 WebdriverIO + `@wdio/tauri-service`**，替代 `package.json` 中预留的 `@playwright/test`（Playwright 无法驱动 Tauri 的 webkit2gtk webview，因为它不是 W3C WebDriver 客户端）。这是 Tauri v2 官方推荐、跨 Win/Linux/macOS 的方案。

2. **引入 `tauri-plugin-wdio`（Rust 插件）**：提供 `execute`（执行 JS 并携带 Tauri API）、`mock`（invoke 拦截）、日志转发三类能力，是 `browser.tauri.execute()` API 功能验证的必需前提。它以 `cfg(debug_assertions)` 条件注册，**不进入 release 构建**，不增加生产攻击面。

3. **驱动层选 external `tauri-driver`**（Linux 上 `tauri-driver` + WebKitWebDriver 二进程驱动 webview），而非 embedded（`tauri-plugin-wdio-webdriver` 内嵌 server）。理由：遵循「用 tauri-driver 验证 UI」的明确要求，且 external 不改变应用进程内建行为、无需再引入第二个测试插件。embedded 方案（免外部 driver、原生 macOS 支持）记录为后续可切换项。

4. **前端注入 `@wdio/tauri-plugin`**：在 dev 入口导入（初始化 `window.wdioTauri`、invoke 拦截、前后端日志转发），仅测试环境生效，生产构建不引入。

5. **capabilities 增加 `wdio:default` 权限**（或最小化 `wdio:allow-execute`），使 execute 命令对测试可用。应用已启用 `withGlobalTauri`，无需额外改动。

6. **CI 结构**：保留 `lint.yml`（eslint 风格门禁）不变；新增测试 workflow 拆分为独立 job——
   - Rust 测试 job：桌面端 + 移动端各跑 `cargo test`
   - 前端测试 job：两端各跑 `pnpm run test:run`
   - E2E job：桌面端，`xvfb-run` 包裹，先 `cargo install tauri-driver --locked`，安装 `webkit2gtk-driver` + `xvfb` 系统依赖，再运行 WebdriverIO 套件
   - E2E 通过 path filter 触发；Rust/前端测试不设 filter（全量回归）

7. **测试目录结构**：桌面端新增独立 e2e 目录（含 `wdio.conf` + spec 文件），与现有 `src-tauri/tests/`（Rust 集成）分离，避免 cargo test 误纳入前端测试产物。

8. **`tauri-driver` 版本独立于 Tauri 版本**：按官方 CI 实践 `cargo install tauri-driver --locked`，锁定与其余 Tauri 组件解耦。

## Testing Decisions

### 测试接缝（seam）——三层，越靠前越优先

- **Seam A（Rust HTTP/WS API seam，已有）**：进程内真实启动 Actix HTTP+WS 服务器，用真实 reqwest / tokio-tungstenite 从外部连入，走完整「中间件 → 路由 → service → actor → PTY」链路。这是最高 seam，无头、快、稳，是后端核心业务回归的主战场。
- **Seam B（前端 vitest seam，已有）**：composable / store / 组件逻辑单测，覆盖纯前端逻辑。
- **Seam C（WebDriver seam，本次新增）**：通过 `@wdio/tauri-service` 的 `browser.tauri.execute()` 在**真实应用进程**内调 `invoke` 命令（API 功能验证）+ W3C WebDriver 协议操作真实 webview（UI 验证）。这是唯一覆盖「前端 invoke 封装 → Tauri IPC → Rust 命令」完整链路的 seam，与 Seam A 互补（A 验证后端对外协议，C 验证应用内 IPC 到命令的端到端）。

### 好测试的定义

只测**外部可观察行为**：命令的返回形状与业务副作用、UI 的可见状态与文案、事件的可观测流转。不测实现细节：不直接断言内部函数、不依赖中间状态、不对私有结构做白盒断言。命令 mock 只用于隔离「前端逻辑」与「后端真实状态」，不用于伪装后端正确性。

### 分层分工

- **Seam A（cargo test）**：后端业务回归的主力，覆盖配对、会话、PTY、文件、链路加密、WS 路由——已有 `server_integration`、`http_auth_biometric`、`ws_session_route`、`pty_session_chain` 等先例。
- **Seam B（vitest）**：前端逻辑回归，已有 358+ 用例。
- **Seam C（WebDriver E2E）**：以 API 功能验证（execute）为主、UI 验证为辅。E2E 只做冒烟级关键路径（应用能启动、IPC 通、核心命令链路通、关键 UI 能点通），不追求全量 UI 覆盖——细粒度回归仍由 Seam A/B 承担。

### Prior art

- Rust 集成测试：桌面端 8 个集成测试文件（`server_integration` 真实启动 + 外部连入模式）；移动端 4 个（对称的 WS 客户端 + 本地 mock 服务器模式）。
- 无头驱动：移动端 dev-shell 曾用 headless Chrome CDP 冒烟脚本模式，本 spec 的无头思路（Xvfb + 真实应用）是其 Tauri 桌面端的对应物。

## Out of Scope

- **移动端 WebDriver/E2E**：移动端是 Android，其 UI 自动化走 Appium/移动 WebDriver，属另一套栈，不在本 spec。
- **iOS**：当前无 iOS 目标，embedded provider 的 macOS 能力暂不启用。
- **移动端 Kotlin 编译进 CI**：`gen/android` 的 gradlew 编译验证维持本地执行（AGENTS.md 约定），不进本次 CI 门禁。
- **性能/负载测试**、**覆盖率硬性门槛**：不在本次范围。
- **生产构建剥离**：`tauri-plugin-wdio` 的 `cfg(debug_assertions)` 隔离已在本 spec 决策内，不另设生产剥离任务。

## Further Notes

- **Playwright 依赖处置**：`@playwright/test` 与本方案（WebdriverIO）冲突，应在实现时从依赖与 `test:e2e` 脚本中移除或明确标注弃用，避免误导后续维护者。
- **是否新增 ADR**：本 spec 确立的「三层 seam 测试分层 + CI 门禁」是长期约定，建议实现落地后补一份 ADR（测试分层与门禁策略），供后续测试相关工作引用。
- **driverProvider 切换成本**：若未来需要 macOS E2E 或想去掉外部 driver 依赖，可切到 embedded（加 `tauri-plugin-wdio-webdriver` 并改 `driverProvider`），`browser.tauri.execute()` 与测试用例本身不变。
- **tauri-driver 与 WebKitWebDriver 的版本匹配**：Linux 上 WebKitWebDriver 须与系统 webkit2gtk 大版本匹配，CI 依赖安装需显式指定（`webkit2gtk-driver`），避免驱动与 webview 引擎版本错配导致的会话创建失败。
