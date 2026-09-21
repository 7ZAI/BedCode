# 终端弹窗迁入 session 插件 · 实施票

> 依据：`.scratch/2026-09-21-terminal-into-session-plugin/spec.md`（方案 1：渲染组件整体
> 下沉 + 插件改名 terminal-session）。每票独立可验收，依赖关系见各票 Blocked by。

## 票 01 · A1 渲染管线整体下沉插件前端（最大块，拆 3 子票）

**范围**：`TerminalPreview.vue`（~760 行）+ 7 个 `composables/terminal/*` + 5 个
`utils/terminal*` 从宿主迁入插件 `src/`，xterm 实例改由插件创建。
**拆 3 子票**：
- 01a：terminalKernel + useTerminalWritePipeline（xterm 初始化、UTF8/WebGL addon、写入管线）
- 01b：useTerminalRenderer + useTerminalScroll + useTerminalResize（renderer、滚动、resize 裁决）
- 01c：useTerminalSettingsSync + terminalThemes/InitialSize/Scrollback + IME 守卫
  （settingsSync/IME/themes）

**前置**：需先确认插件 PluginContext 可提供等价能力（logger/platform/toast/UI 组件）。
**验收**：插件内终端组件可实例化渲染 xterm（宿主内预览路径不变的前提下）；
`pnpm run test:run` + eslint 0 error。

## 票 02 · A1 xterm 依赖进插件构建链

**范围**：`@xterm/xterm@6` + addon-fit/unicode11/web-links/webgl 进
`plugins/session/package.json`；构建 external 清单确认不含 xterm；xterm.css 经
inlinePluginCss/injectStyle 自带；产物体积核查。
**验收**：插件构建产物含 xterm，宿主运行时无冲突（无第二份 vue 已由 bedcodePlugin
保证；xterm 独立无共享冲突）。

## 票 03 · A2 终端窗口壳 + 编排迁入

**范围**：`TerminalWindowView.vue` 壳（标题栏/状态条/设置面板）+ `useSessionWindows`
（WebviewWindow 创建/定位/复用/关闭）迁入插件；宿主 `/terminal-window/:id` 路由改
插件视图宿主（PluginViewHost 或等效挂载点）。
**前置**：Blocked by 票 01（壳依赖渲染组件）、票 02。
**验收**：宿主点击会话 → 插件编排开窗；窗口定位/复用/关闭行为与迁移前一致。

## 票 04 · A3 输出消费二进制原语

**范围**：`host-session` 追加 `output-ring-fetch(session-id, from-offset, max-bytes)`
（list<u8> 直传 + next-offset + truncated，权限 `terminal:output`，开放点 2 定案）；宿主
`GlobalOutputManager` 会话 ring 向插件开游标（内核保有环、
插件注册游标 + 自有 ack 水位，背压语义沿用 2026-09-17）；WIT/SDK/ABI 五同步点。
**前置**：Blocked by 票 01（插件侧消费方就位）。
**验收**：插件经原语拉取会话输出字节不 JSON 化；慢消费只损失自己的 ring 历史；
`cargo test --lib` 全绿；移动端零改动（若新增 interface 则登记双端偏离）。

**状态：✅ done（2026-09-22，d0bd98d8a 之后）**——开放点①②③已定案（见 spec §5）；
- host-session 追加 `ring-fetch-result` record + `output-ring-fetch`（权限 `terminal:output` + 属主校验）；
- SDK 五同步点：WIT / guest trait（`HostSession::session_output_ring_fetch` + `SessionRingFetch`）/ wasm_host / 宿主 host_impl（`session_output_ring_fetch`）/ component（`RingFetchResult` map）；ABI 不 bump（函数级追加，desktop v22 不变；mobile 零改动）；
- `UnifiedOutputQueue::fetch`（游标钳位语义同 `PtyRing::fetch`：落后→truncated 重锚、未来→自愈追平）；常量 `PLUGIN_SESSION_RING_FETCH_MAX_BYTES`=16KiB；
- 插件侧：Rust 命令面 `session.output.pull`（output.rs，经原语拉取）+ 前端 TerminalPreview 撤 `caps.output.attachSink` 桥改自适应轮询（快档 100ms / 空闲退避 500ms，单 tick 8 批，truncated 清屏重锚）；
- 验证：宿主 lib 1100 全绿（含 4 个 host_impl 单测 + 4 个 fetch 单测 + e2e `test_session_output_ring_fetch_closed_loop`——真实 bash 输出经插件命令面原语拉回字节）；SDK 93 绿；插件 vitest 76 文件 741 绿；eslint 0 error；wasip3 产物重建成功。

## 票 05 · A5 宿主兜底摘除

**范围**：宿主 `TerminalPreview.vue` / `composables/terminal/*` / `utils/terminal*`
移除或留空壳；禁用插件 → 终端入口消失 + 宿主命令面显性报错（同配对/QR 退役后模式）；
宿主不留降级终端实现。
**前置**：Blocked by 票 01-04（插件侧完整接管后宿主才可摘除）。
**验收**：禁用插件后终端不可用且不破宿主；启用后全功能一致。

**状态：✅ done（2026-09-22，票 05 提交）**：
- **宿主前端摘除**：`TerminalPreview.vue` / `TerminalWindowView.vue`（旧壳）/
  `composables/terminal/*`（6）/ `utils/terminal*`（7，仅留 `terminalInitialSize`——
  `context.session.predictTerminalSize` 依赖的窗口几何原语）/ `useTerminalOutputStreamChannel.ts` /
  `PluginTerminalToolbar.vue`（宿主渲染件，插件已复刻）整体删除；
- **attachSink 契约收口**：插件 `terminalHostCapabilities.ts` 与宿主
  `terminal-host-capabilities-contract.ts` 同步移除 `output.attachSink` 字段与
  `TerminalOutputSink` 类型；`TerminalWindowHostView.vue` 撤输出桥（provide 只剩
  settings/bgImage/extensions 三面）；输出唯一路径 = 插件命令面 `session.output.pull`
  （→ `host-session.output-ring-fetch` 原语）；
- **禁用插件 → 显性报错**：`context.session` 的 `openTerminal / closeTerminal /
  isTerminalOpen` 过激活门禁（`registry.isContributionActive('com.bedcode.session')`，
  停用/Error 时 throw 显性错误），宿主不留降级终端代办；测试 C7 锁定；
- **宿主 Rust 输出命令面摘除**：`commands/terminal_stream.rs`（subscribe_terminal_channel /
  unsubscribe_terminal_channel / terminal_channel_ack）删除 + 注册点清理（lib.rs / commands.rs），
  宿主不留降级输出传输；
- **测试迁移**：7 个宿主纯逻辑测试（terminalDimensions / ImeStateMachine / LinuxImeGuard /
  RendererPolicy / ResizeDebouncer / ResizePolicy / Scrollback）迁入插件
  `plugins/session/src/__tests__/`；删宿主 terminal-flow 集成测试（被测 Channel 传输已摘除，
  行为由插件 terminalPreview.test.ts 覆盖）——迁移暴露真实 bug：
  `getXtermScaledDimensions` 未钳位 NaN/≤0 dpr（原宿主测试靠缺容器字段的退化入参误过），
  已修复入 `terminalDimensions.ts` 守卫；
- 验证：宿主 vitest 75 文件 735 全绿；插件 vitest 同批全绿；eslint 0 error
  （120 warning，较 125 基线减 5）；`cargo check --lib` 干净。
  ⚠ Rust 全量测试暂红（1103 过 / 10 败）：全部 10 个失败属并发在途的
  wasm-core-audit 票线改动（bus topic ACL / pty topic 形状漂移，正在推进中），
  与票 05 改动无交集（票 05 只删 terminal_stream 命令面，无残留引用）；等待该线落地后复验。

## 票 06 · B1 插件 id 改名全链

**范围**：目录 `plugins/session/` → `plugins/terminal-session/`；plugin.json id/name/
rustLibrary；Rust crate 名 + `WasmPlugin::ID`；宿主 Rust 26 文件常量集中化
（auth_center/gateway/fs_auth/host_impl/server/events/wasm_runtime fixture）；
互调 api `com.bedcode.session.*` → `com.bedcode.terminal-session.*`；前端路由/侧边栏/
settings/i18n/fixture；SDK/打包 CLI fixture 常量。
**前置**：Blocked by 票 01-05（终端域已并入后改名，一次 ABI 面收口）。
**验收**：改名后全部构建/测试/运行路径引用新 id；移动端零改动。

## 票 07 · B2 私有库真源迁移 + 双投窗口

**范围**：`…/plugins/com.bedcode.session/plugin.db`（会话配置真源 + 任务历史 +
迁移账本）原子搬运到 `…/com.bedcode.terminal-session/plugin.db`（`VACUUM INTO`
复制 + 校验 + 切换，或目录 rename + 旧名软链兼容期；幂等版本戳沿用
task_data_migration 模式）；旧 HTTP 前缀 `/api/plugin/com.bedcode.session/*` 与旧
api 名双投窗口。
**前置**：Blocked by 票 06。
**验收**：升级后用户既有会话配置/任务/配对完整（有断言）；旧前缀/旧 api 窗口内可用。

## 票 08 · C 文档与登记

**范围**：AGENTS.md §7（插件清单、HOST_PRIMITIVE_CAPABILITIES 计数）、code-map、
CHANGELOG、ADR 0022（插件 id 变更登记 + v23 输出原语落地引用）、roadmap 阶段 3
状态、`.scratch/2026-09-19-terminal-session-plugin/spec.md` 状态同步。
**前置**：Blocked by 票 01-07。
**验收**：文档与实际一致，无残留旧 id 文档引用（除迁移/兼容说明）。
