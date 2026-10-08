# 票 16 · auto-task 并入终端 wasm app（D6 选项 A，阶段 3 收口票）

Status: **in-progress**（2026-10-08 实施）

## 1. 目标

移动端 `com.bedcode.auto-task` **不再作为独立插件存在**——任务队列面板 / 工具箱「任务记录 + 定时任务」/ `api.ts` HTTP 客户端 / i18n / 样式 / 激活编排整体并入阶段 3 新建的 **`com.bedcode.terminal-session`**（移动版 app，D6 选项 A 与桌面同名），与终端消费同属一个 app、一个 manifest、一个 ABI 窗、一份权限集（取并集）。

桌面先例（唯一参照）：`wasm-apps/terminal-session`（2026-09-20 并入 `com.bedcode.auto-task` + `com.bedcode.devices`，四域合一）。

## 2. 强制口径（spec §4 D6，不可放松）

- ① manifest 权限位与贡献点取并集，未用到的贡献点随迁即删（`contributes.lifecycle` 四钩子声明即属此类：插件未注册 handler、宿主 dispatch 空转 ⇒ 删）
- ② 按**域重组**而非逐文件平移；i18n key 双语同步并带 app 前缀（前缀 = 插件 id，自动前缀化）
- ③ 只切移动端基址（`/api/plugin/com.bedcode.auto-task` → `/api/plugin/com.bedcode.terminal-session`），桌面 `LEGACY_HTTP_PLUGIN_ALIASES` 保留不动
- ④ 旧 id `com.bedcode.auto-task` / 视图 id `auto-task.toolbox` / 命令 id `auto-task.*` 退役走 fail-visible + 防回接锁 + 变异自检
- ⑤ 五处硬引用同批同步（fs_auth 白名单 / 根 `plugin-package-list.json` / `dev-run.js` / dev-shell mock 基址 / `plugin-dev-mobile.md`）
- ⑥ 文案口径 = 「远程终端控制端」，不自称会话/终端权威

## 3. 落地面（实施清单）

### 3.1 前端域迁移（`plugins/terminal-session/src/task/`）

| 源（auto-task） | 目标（terminal-session） | 改动 |
| --- | --- | --- |
| `src/api.ts` | `src/task/api.ts` | 基址切 `com.bedcode.terminal-session`，头注释改「任务域」 |
| `src/state.ts` | `src/task/state.ts` | 平移 |
| `src/i18n.ts` | `src/task/i18n.ts` | 平移（相对 key；前缀随插件 id 自动变为 `com.bedcode.terminal-session.`） |
| `src/panel.css` / `src/toolbox.css` | `src/task/{panel,toolbox}.css` | 平移 |
| `src/env.d.ts` | `src/env.d.ts` | 平移（顶层；`*.vue` + `*.css?inline` 声明） |
| `src/components/*.vue`（4） | `src/task/components/` | 平移（相对 import 不变） |
| `src/composables/*.ts`（2） | `src/task/composables/` | 平移 |
| `src/index.ts` | `src/index.ts` | **重写**：入口改为「终端域（票 15 迁入前为空）+ 任务域激活」，工具条目 id `terminal-session.task-toolbar`、工具箱视图 id `terminal-session.toolbox`、style 注入 id 换 `terminal-session-task-*`、devMock（queueSeed）保留 |
| `rust/**`（极简壳：`invoke_command` 显式全拒） | 退役 | D6：随迁即删（业务全走 HTTP，无前端命令面消费者） |
| `contributes.commands`（4 条 `auto-task.*`） | 退役 | 同上（TS 从未 invoke，rust 全拒） |
| `contributes.lifecycle`（4 钩子） | 退役 | 未使用（无 handler 注册，宿主 dispatch 空转） |
| `contributes.terminal.toolbarItems` / `contributes.views` | 迁入（换 id） | `terminal-session.task-toolbar` / `terminal-session.toolbox` |
| `package.json` | 加 `@vuepic/vue-datepicker`（任务域定时表单用） |
| `tsconfig.json` | 换 auto-task 版（含 vue / paths 映射） |

### 3.2 硬引用同步（五处 + 连带）

1. `src-tauri/src/lib.rs` fs_auth 可信白名单：`com.bedcode.auto-task` → `com.bedcode.terminal-session`
2. 根 `scripts/plugin-package-list.json`：mobile 列表删 `auto-task`（`terminal-session` 已在）
3. `scripts/dev-run.js` PLUGIN_WATCH_CMDS：auto-task 条目替换为 terminal-session（`bedcode_plugin_terminal_session.wasm`）
4. `packages/plugin-sdk-mobile/dev-shell/src/mock/mobile-api.ts`：mock 基址切新前缀
5. `plugin-dev-mobile.md`：构建示例 / 自渲染浮层示例引用 / 内置插件列表

连带：`src-tauri/resources/plugins/mobile/com.bedcode.auto-task/`（旧产物目录）删除；`packages/.cargo/config.toml` 注释；两端 README 插件列表；CI 三处插件安装列表（auto-task → terminal-session）；dev-shell README/MockTerminalView 注释；`src/locales/en/mobile.ts` 措辞；`src/__tests__/plugin/pluginIcon.test.ts` 测试数据。

### 3.3 宿主测试夹具改造（被删插件目录的引用）

- `src-tauri/src/plugin/wasm_runtime/component.rs`：`build_auto_task_component` → `build_terminal_session_component`（真实 SDK 宏产物样本换成合并后的 app）；`test_sdk_macro_component_loads_and_activates` 传生产权限集（bus/auth/terminal:output/ws:client/storage），断言适配
- `src-tauri/src/plugin/loader.rs` 测试：临时插件目录换 terminal-session（manifest 声明生产权限集 ⇒ activate == 0）
- `src-tauri/src/plugin/validation.rs`：示例 id 换 `com.bedcode.terminal-session`

### 3.4 退役与锁（fail-visible 三形态）

- ① 旧读路径删除：插件目录 / 打包资源目录 / 旧基址 / 旧 mock 基址
- ② 旧产物实例化期点名：本票零 ABI 变更（v16 不动），无新增判据
- ③ 退役 id / 视图 id 加载即抛：新锁 `src-tauri/tests/retired_mobile_auto_task_plugin_lock.rs`
  - L1：`src/**/*.rs` 零 `com.bedcode.auto-task` / `auto-task.toolbox` 字样（跳过纯注释行）
  - L2：`invoke_handler(` 真实块零 auto-task 注册项
  - L3：宿主前端 `src/**` 零退役字面量
  - L4：反向断言——`plugins/auto-task` 目录不存在；`plugins/terminal-session/plugin.json` 含权限并集（auth/bus/session:read/storage/terminal:output/ui:input/ui:toolbox/ws:client）与视图 id `terminal-session.toolbox`；插件源码含 registerToolboxPage / registerTerminalToolbarItem；前端任务域基址 = 新前缀
  - 变异自检（旁路 → 转红 → 还原）

### 3.5 文档联动

- `bedcode-mobile/docs/code-map.md`（plugins 段 / 前端插件段 / Quick Navigation / 锁索引）
- spec 状态行 + 票 16 条目收口
- ADR 0018 双端偏离表（C8：两端同名 id 不同职责）+ ADR 0022 移动端批次条目 + ADR 0012 补条目（视图/命令 id 变更）
- `docs/knowledge/plugin-kernel-roadmap.md` M1 状态（基址已切 / 别名表待双端同批切断）
- `docs/diagrams/plugin-auto-task-mobile.html` 桌面节点端点标注换新前缀
- `CHANGELOG.md` / `CHANGELOG_zh.md` 双语

## 4. 门禁（AGENTS §10 两段式）

- 开发中：针对性单测（锁 + 改造测试）
- 收尾：
  - 宿主 `cargo test`（全部集成目标 + lib 基线）
  - 插件 native：`cd plugins/terminal-session/rust && cargo test`
  - **wasm32 真门禁**：`cd plugins/terminal-session && pnpm run build`（SDK CLI，native 绿 ≠ 可交付）
  - 前端全量：`cd bedcode-mobile && pnpm run test:run`
  - 根 `pnpm exec eslint .` 0 error
  - 新锁变异自检 4/4
- 未跑项逐项说明（真机三面验收留票 21：工具栏入口 / 工具箱两页签 / 队列面板 + ADR 0012 WS 刷新链路）

## 5. 实施记录（2026-10-08）

### 5.1 落地清单（对照 §3）

- 前端域迁移：`plugins/terminal-session/src/task/`（activate.ts / api.ts / state.ts / i18n.ts / devMock.ts / panel.css / toolbox.css / components×4 / composables×2 / env.d.ts 顶层）；入口 `src/index.ts` 重写为「域组合」；`package.json` 加 `@vuepic/vue-datepicker`（pnpm install 已更新 lock）；`tsconfig.json` 换 auto-task 版（vue + paths）；补 `.gitignore`、新建 `README.md`（域划分 + 权限/命令表）
- 退役：`plugins/auto-task/` 整目录删除、`src-tauri/resources/plugins/mobile/com.bedcode.auto-task/` 删除、4 条 `auto-task.*` 命令与 `contributes.lifecycle` 不再声明
- 硬引用五处 + 连带：`lib.rs` fs_auth 白名单 / 根 `plugin-package-list.json` / `dev-run.js`（id + wasm 产物名）/ dev-shell mock 基址 / `plugin-dev-mobile.md`（构建示例、浮层示例引用、内置插件列表）+ README 双语插件列表 / CI 三处安装列表（test.yml、release.yml×2）/ dev-shell README + MockTerminalView 注释 / `src/locales/en/mobile.ts` 措辞 / `useHttpApi.ts`、`useMobileCommands.ts` 注释 / `pluginIcon.test.ts` 测试数据 / `packages/.cargo/config.toml` 注释
- 宿主测试夹具：`build_auto_task_component` → `build_terminal_session_component`（真实 SDK 宏样本 + 生产权限集 8 位）；`loader.rs` 测试 manifest 换 terminal-session（声明生产权限集 ⇒ activate == 0）；`validation.rs` 示例 id 换
- 新锁：`src-tauri/tests/retired_mobile_auto_task_plugin_lock.rs`（4 例）

### 5.2 门禁实测

| 门禁 | 结果 |
| --- | --- |
| 新锁 `retired_mobile_auto_task_plugin_lock` | **4/4 绿** |
| 新锁变异自检（旧 id 常量 / 块内行 / 前端字面量 / 删权限位） | **4/4 注入转红 + 还原全绿** |
| 宿主 `cargo test --no-fail-fast` 集成 targets | **全绿**（新锁 4/4、票 12/14 三把锁复跑 4/4+4/4、`mobile_host_websocket_client_domain_lock` 2、`ws_protocol_integration` 1、`session_http_flow` 1、`http_auth_flow` 17、`http_proxy_flow` 7、`mock_plugin_ws_fixture` 14、`plugin_storage_db_backed_lock` 1、`build_manifest_smoke` 1） |
| 宿主 lib | 324 passed / **6 failed**（`egress.rs` 6 处 = host-authorization-policy 专项在途基线，与票 12/14 记录同数同款） |
| 组件样本测试（`test_sdk_macro_component_loads_and_activates`） | 绿（terminal-session 真实组件：实例化 + ABI v16 + activate==0（bus 订阅 4+1 topic + auth 探测）+ deactivate==0 + manifest id + 未知命令 error JSON） |
| loader 测试（`test_load_all_loads_component_plugin`） | 绿（terminal-session 组件产物 + manifest 权限集 ⇒ activate==0 + AOT 缓存） |
| validation 单测 | 3/3 绿 |
| 插件 native `cargo test`（terminal-session/rust） | **40/40 绿** |
| **wasm32 真门禁**（`pnpm run plugins:build -- --plugin com.bedcode.terminal-session`） | **通过**（组件化 491,552 字节；产物刷新进 `resources/plugins/mobile/com.bedcode.terminal-session/`：index.js 283KB / plugin.json / wasm） |
| 前端全量 `pnpm run test:run`（--maxWorkers=2） | **65 文件 / 680 用例全绿** |
| 根 `pnpm exec eslint .` | **0 error** / 115 warning（不计入门禁） |
| 未跑 | `cross-end-tests`（本票零跨端 wire 变更，桌面端零改动）；真机三面验收（工具栏入口 / 工具箱两页签 / 队列面板 + ADR 0012 WS 刷新链路）留票 21 |

### 5.3 偏差与记账（不隐藏）

1. **`terminal:input` 权限位误推导**：manifest-gen 的 `/\.terminal\b/` 规则命中 URL 字符串
   `com.bedcode.terminal-session`（`.terminal` + `-` 构成 word boundary）⇒ 构建时把 `terminal:input`
   并入 manifest permissions 并写回 `plugin.json`。生成器取并集且幂等（手工删会被下次构建加回），
   故**保留**；本 app 的终端输入走 host-websocket 直发帧、不经宿主 `terminal_send` 原语，该位当前无
   消费面。锁 L4 的权限并集断言按 contains 写（不受影响）。
2. **`pnpm run build`（插件目录内）不带 `--resources-dir`**：只构建不复制产物——刷新打包资源必须走
   `pnpm run plugins:build -- --plugin <id>`（`scripts/plugin-build.js` 注入参数）。已按后者构建。
3. 任务域平移文件的既有 eslint warning（`useTaskHistory.ts` 的未用 `limit`）随迁保留（warning 不入门禁，
   不顺手清理以守最小改动）。
4. `plugins/terminal-session/rust/src/`（终端/认证域）**零改动**；本票零 WIT / 零 ABI 变更（mobile 停 v16）。

