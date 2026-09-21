# 终端弹窗迁入 session 插件（方案 1：渲染组件整体下沉）+ 插件改名 terminal-session

> 状态：**ready-for-agent**（2026-09-21 立项，用户决策：① 终端不单独成插件，并入
> `com.bedcode.session`；② 插件 id 改名 `com.bedcode.terminal-session`；③ 渲染迁移形态
> 选**方案 1**——xterm/渲染管线/写入管线/IME 守卫整体下沉插件，宿主只留 PTY 引擎与
> 输出 ring）。
> 上游：`.scratch/2026-09-21-terminal-output-consumer-perf/report.md` + ADR 0022 v23
> （输出经 WIT 二进制原语可进 WASM，~2.6 ms/MB，风暴 0.26% 单核；禁止 JSON 通道搬运）；
> `.scratch/2026-09-19-terminal-session-plugin/spec.md`（session 插件本体，D1–D7 口径沿用）。
> 关联红线：AGENTS §7（插件检查清单 / 双端偏离 / ABI 批次 / 同实例串行）、§9（双端同步）。

---

## 1. 定位与划界

### 1.1 目标

把桌面端终端弹窗（`TerminalWindowView` + `TerminalPreview` + 渲染管线）从宿主前端
整体迁入 session 插件，作为该插件的**终端会话域**（第四域）；输出消费按 ADR 0022 v23
口径经 **WIT 二进制原语**拉取（宿主保有输出 ring，插件游标消费）。随后插件 id 改名
`com.bedcode.terminal-session`（全链）。**不新建独立插件。**

### 1.2 边界（留内核，红线）

- PTY 引擎（进程状态 / slave fd / 终止汇聚 / 裸命令 exec / 特殊键写入）——不动；
- 输出分发管道**内核保有**：`UnifiedOutputQueue`（环+游标+ack+快照）仍在内核，
  订阅者执行体（拉取 + 私有 ack 水位）仍在内核——**只把消费端从「Tauri Channel
  直推前端」换成「二进制原语供插件拉取」**（v23 裁决形态：list<u8> 直传线性内存）；
- WS 连接骨架 / 认证 / 过滤链 / 注册表——不动；
- 前端宿主布局骨架（侧边栏容器 / 标题栏 / 页面工具栏 / 设置页外壳）——不动；
- 移动端——零改动（双端偏离既有口径，session 插件为桌面独有）。

### 1.3 现状盘点（2026-09-21 实测）

- **插件**：`plugins/session/`（rust-ts，sandbox=inline，kind=Application），
  rustLibrary=`bedcode_plugin_session`，Cargo name=`bedcode-plugin-session`。
- **宿主渲染侧**：`src/views/TerminalWindowView.vue`（WebviewWindow 路由
  `/terminal-window/:id`）+ `src/components/TerminalPreview.vue`（~760 行）+ 7 个
  `composables/terminal/*` + 5 个 `utils/terminal*` + `PluginTerminalToolbar.vue`
  （终端工具栏扩展点，插件已在贡献）。xterm 依赖宿主 `package.json`：
  `@xterm/xterm@6` + addon-fit / unicode11 / web-links / webgl。
- **输出通道现状**：`commands/terminal_stream.rs`（`subscribe_terminal_channel`，
  Tauri Channel Raw 字节直推，4ms 合并窗口）——**插件不可达**；`terminal-hooks`
  `on-terminal-output`（WIT `string` 钩子）宿主侧**只有测试调用点、无生产接线**。
- **输出原语现状**：host-pty `ring-fetch`（v16，`list<u8>` 直传 + 游标 +
  truncated/resync）已落地——方案 1 的输出消费面**复用它**（宿主为业务会话保有
  ring，插件按会话 id 拉取），不自造平行原语。
- **插件前端运行时**：`bedcodePlugin()` 把 vue/i18n/pinia/sonner 外部化，改读
  `window.__BEDCODE_SHARED__`（插件不得自带第二份运行时）；`inlinePluginCss()`
  把 CSS 内联（迁入视图组件时必需，先例：票 13）。**xterm 不在共享运行时清单**——
  方案 1 需决策 xterm 的归属（见 A1）。
- **id 引用面**：宿主 Rust 26 文件 + 前端/脚本/测试 50+ 文件 + 移动端 0；
  私有库路径 `app_data_dir/plugins/<plugin_id>`（改名即换目录，需数据迁移）；
  迁移先例 `task_data_migration.rs`（auto-task→session，`LEGACY_PLUGIN_ID`/
  `TARGET_PLUGIN_ID` + `plugin_meta` 幂等版本戳）。

---

## 2. 阶段 A · 终端渲染整体下沉 session 插件

### A1 渲染管线迁入（方案 1 主体）

**迁移清单**（从宿主 → 插件 `src/components/terminal/` + `src/composables/terminal/` +
`src/utils/terminal/`）：

| 宿主现状 | 迁入插件后 |
| --- | --- |
| `TerminalPreview.vue`（xterm 实例 + 渲染 + 头部工具栏 + 设置面板） | `components/terminal/TerminalPreview.vue`，xterm 实例由插件创建 |
| `composables/terminal/terminalKernel.ts`（createTerminalKernel） | 插件侧 kernel（xterm 初始化、UTF8/WebGL addon、字体/主题） |
| `composables/terminal/useTerminalWritePipeline.ts` | 插件侧写入管线（输入分块 + 防抖 + IME 协作） |
| `composables/terminal/useTerminalRenderer.ts` | 插件侧 renderer（WebGL addon 决策 + DOM 回退 + context-loss 恢复） |
| `composables/terminal/useTerminalResize.ts` + `utils/terminalResizeDebouncer.ts` | 插件侧 resize 裁决（正统端仲裁规则在插件，事实登记内核） |
| `composables/terminal/useTerminalScroll.ts` + `utils/terminalScrollback.ts` | 插件侧滚动/回退 |
| `composables/terminal/useTerminalSettingsSync.ts` + `utils/terminalThemes.ts` / `terminalInitialSize.ts` / `terminalImeStateMachine.ts` / `terminalLinuxImeGuard.ts` | 插件侧设置同步 / 主题 / 初始尺寸 / IME 守卫（Linux WebKitGTK IME 防护接线） |
| `utils/frontendLogger` / `usePlatform` / `useToast` / `Select`/`Button`/`Modal` | 插件经 `PluginContext` 取等价物（logger/platform/UI 组件由共享运行时或宿主注入） |

**xterm 依赖归属（关键决策）**：
- 插件构建链是独立 vite 库模式；`@xterm/xterm` 等 5 个包约几百 KB，**不进**
  `__BEDCODE_SHARED__`（那是 vue 等核心运行时，xterm 无跨插件共享价值）。
- **定案：xterm 作为插件自身依赖**（`plugins/session/package.json` devDependencies
  + 构建时打进产物，`bedcodePlugin()` 的 external 清单**不包含** xterm）。
  唯一宿主耦合点是 CSS 加载（xterm.css）——插件经 `inlinePluginCss` 或动态
  `injectStyle` 自带（`session` 插件已有 `injectStyle` 先例：task-modal-css）。

### A2 终端窗口编排迁入

- `useSessionWindows`（WebviewWindow 创建/定位/复用/关闭）迁入插件 composable；
- 宿主 `context.session.openTerminal` 保持为**引擎原语**（窗口创建无业务语义，
  ADR 0022 裁剪线），插件持有「何时开、开哪个、初始尺寸、复用、关闭」编排；
- `TerminalWindowView.vue` 的壳（标题栏/状态条/设置面板）迁入插件
  `views/terminal/TerminalWindowView.vue`，宿主 `/terminal-window/:id` 路由改为
  **插件视图宿主**（`PluginViewHost` 或等效的插件挂载点）。

### A3 输出消费原语（v23 落地）

- **复用 host-pty `ring-fetch` 形态**，为业务会话开输出面：`host-session` 追加
  `output-ring-fetch(session-id, from-offset, max-bytes)`（二进制 `list<u8>` 直传 +
  next-offset + truncated，权限 `session:read`）——**或**：把 host-pty 的 ring
  fetch 推广为通用「会话输出游标拉取」。二选一在实施票定（见 §5 开放点 1），
  契约必须与 v23 裁决一致：list<u8> 直传、禁 JSON 数组化、游标 + truncated/resync。
- 宿主侧：`GlobalOutputManager` 的会话 ring 向插件开放游标订阅（内核保有环，
  插件注册游标 + 自有 ack 水位，慢消费只损失自己的 ring 历史——2026-09-17 背压
  语义直接沿用）；逐帧输出仍不进 JSON。
- **否决面**：不新增 push 回调（wasmtime 不可重入，host-pty D3 两条理由不变）；
  不把输出经 `terminal-hooks.on-terminal-output`（string 文本钩子）搬运——它留作
  观察型小载荷钩子（现状测试接入），大数据走二进制原语。

### A4 权限与 manifest

- 新增 `terminal:output`（输出消费：从会话 ring 拉取字节）——若 A3 走 `session:read`
  则复用不新增（沿票 17「多一项就是审计噪音」惯例，实施时按实际权限面定）；
- `terminal:input` / `terminal:observe`（现状已有）保持；
- manifest 描述、api 面（终端打开/关闭命令入 `api` 白名单）。

### A5 宿主兜底

- 插件禁用后：终端弹窗不可用（宿主命令面显性报错，同配对/QR 退役后模式）；
  宿主**不保留降级终端实现**（红线：不留僵尸路径）——侧边栏终端入口由插件贡献
  的目录项承载，插件停用即整组摘除（error 态摘除机制既有）。

**阶段 A 验收**：
- 终端弹窗全部由 session 插件贡献（宿主 `TerminalPreview.vue` /
  `composables/terminal/*` / `utils/terminal*` 移除或留空壳）；
- 禁用插件 → 终端入口消失、宿主不破；启用 → 全功能（输入/输出/滚动/IME/resize/
  主题/字体）与迁移前一致（前端集成测试接缝不变，票 34「终端流程测试接缝」约束）；
- 输出经二进制原语（不 JSON 化）；`cargo test --lib -- --test-threads 1` +
  `pnpm run test:run` + 根 `pnpm exec eslint .` 0 error；移动端零改动。

---

## 3. 阶段 B · 插件 id 改名 com.bedcode.terminal-session（依赖 A）

### B1 全链改名面

| 面 | 改动 |
| --- | --- |
| 插件工程 | 目录 `plugins/session/` → `plugins/terminal-session/`；构建白名单 `scripts/plugin-build.js` / `dev-run.js` / `package.json` 同步 |
| `plugin.json` | `id` = `com.bedcode.terminal-session`；`name` 同步（"Terminal Session Center"）；`rustLibrary` = `bedcode_plugin_terminal_session` |
| Rust crate | `bedcode-plugin-session` → `bedcode-plugin-terminal-session`；`WasmPlugin::ID` 常量；wasm 产物名 |
| 宿主 Rust（26 文件） | 常量集中化：`auth_center.rs` `SESSION_PLUGIN_ID` / `SESSION_MARKER_API`、`gateway.rs` `SESSION_PLUGIN` + `BUSINESS_PLUGINS` + `/api/plugin/*` 前缀、`fs_auth.rs` 白名单、`host_impl/session.rs` 属主断言、server DTO、events 注释、wasm_runtime fixture 常量——全部改从统一常量导入（禁散落字符串） |
| 互调 api | `com.bedcode.session.*` → `com.bedcode.terminal-session.*`（`quick_actions_migration.rs` 的 `API_QUICK_ACTIONS_IMPORT`、manifest api 白名单、调用方 manifest） |
| HTTP 网关 | `/api/plugin/com.bedcode.session/*` → `/api/plugin/com.bedcode.terminal-session/*`；**旧前缀双投**一段窗口（同票 16 先例，老移动端/老前端不受损；切断并入移动端适配专项） |
| SDK / 打包 CLI | manifest 校验（id 非权限，不改合法权限集）；`plugin-sdk-test` fixture 常量 |
| 前端 | 路由 pluginId、侧边栏贡献、settings 分组 key、i18n、fixture、测试 |
| 测试 | wasm_runtime fixture 常量、host_impl/session.rs 属主断言、session_e2e / a03_probe |

### B2 私有库数据迁移（最重风险项）

- 旧库 `…/plugins/com.bedcode.session/plugin.db`（会话配置真源 + 任务历史 + 迁移账本）
  在改名后**必须继续可读**——私有库按插件 id 分文件，改名即换目录。
- **方案（沿用 task_data_migration 模式 + 真源特判）**：
  1. 新插件 activate 首跑检测旧目录存在 → **一次性幂等搬运**（`plugin_meta` 版本戳，
     存在性即版本戳、best-effort 不阻断启动、可对旧库重跑）；
  2. **真源特判**：`session_configs` 与任务表是业务真源（非 auto-task 遗留的一次性
     导入）——搬运必须**原子且先搬后启**：候选 = SQLite `VACUUM INTO` 复制 + 校验
     + 切换，或目录 rename（同盘原子）+ 旧名软链兼容期；禁止「复制一半」
     （`INSERT OR IGNORE` 追加式只对非真源表可接受）；
  3. `pairings` / `connection_history` 是内核主库表，不受插件改名影响。
- **互调兼容**：外部调用方（若有）经新 api 名；旧 api 名在双投窗口内 alias 转发
  （同 HTTP 双投先例）或显性拒绝并记录——实施时按实际调用方定。

**阶段 B 验收**：升级后用户既有会话配置 / 任务历史 / 配对记录完整（有断言）；
新旧 id 双投窗口内旧调用可用；cargo test + vitest + eslint 全绿；移动端零改动。

---

## 4. 阶段 C · 文档与登记

- AGENTS.md §7（插件清单、HOST_PRIMITIVE_CAPABILITIES 计数——若 A3 新增输出原语
  则 +1 组且同步五同步点）、code-map、CHANGELOG、ADR 0022（插件 id 变更登记 +
  v23 输出原语落地引用）、roadmap 阶段 3（状态推进：终端本体迁入中）、
  `.scratch/2026-09-19-terminal-session-plugin/spec.md`（状态同步）。

---

## 5. 实施票拆分建议（开工时落 issues/）

- 票 01（A1）：渲染管线整体下沉插件前端（最大块，拆 3 子票：kernel/writePipeline、
  renderer/scroll/resize、settingsSync/IME/themes）
- 票 02（A1）：xterm 依赖进插件构建链 + CSS 加载 + 共享运行时核查
- 票 03（A2）：TerminalWindowView 壳 + 窗口编排迁入 + 路由改插件视图宿主
- 票 04（A3）：输出消费二进制原语（host-session 追加或 host-pty 推广，开放点 1 定案）
  + 宿主 GlobalOutputManager 向插件开游标 + 权限位
- 票 05（A5）：宿主兜底摘除 + 禁用路径显性报错
- 票 06（B1）：插件 id 改名全链（含私有库目录切换 + 数据迁移 B2）
- 票 07（B2）：私有库真源迁移 + 旧前缀/api 双投窗口
- 票 08（C）：文档 + 双端偏离 + 登记

**开放点（实施时定，不阻塞立项）**：
1. A3 输出原语形态：host-session 追加 vs host-pty 推广（推荐 host-session 追加
   `output-ring-fetch`，与 v23「host-session-output 若开设必须二进制直传」对齐；
   host-pty 只服务插件私有 PTY）；
2. A4 权限位：新增 `terminal:output` vs 复用 `session:read`；
3. xterm 打包体积：内置 vs 宿主 externals 清单扩展（推荐内置，见 A1 定案）。

---

## 6. 风险与控制

| 风险 | 控制 |
| --- | --- |
| 渲染管线迁入后手感/性能回退 | v23 已验证二进制原语 ~2.6 ms/MB；4ms 合并窗口沿用；前端测试接缝不变（票 34 约束） |
| 私有库改名致用户数据「消失」 | B2 原子搬运 + 软链兼容期 + 幂等版本戳；验收含「升级后数据完整」断言 |
| id 改名 26 处宿主引用漏改 | B1 常量集中化 + 全链清单 + fixture 断言新 id |
| xterm 打包/共享运行时冲突 | A1 xterm 内置定案 + inlinePluginCss 先例；实施时验证产物无第二份 vue |
| 与在途 host-rust-residue 票线交织 | 阶段 A/B/C 独立 commit；`git add -p` 精确隔离（探针已验证可行） |
| 输出原语新增 ABI | 若 host-session 追加函数 → 函数级追加不 bump（v19 同批次惯例）；若新 interface → ABI bump + 双端偏离登记 |
