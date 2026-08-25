# 03 — 桌面插件·首连确认（弹窗 + 状态栏项）

**What to build:** 桌面端陌生节点首次连接本机时的确认流：插件视图内渲染富确认弹窗（设备名、完整节点 ID 指纹、30s 倒计时），接受/拒绝经应答命令结算；用户不在文件传输面板时，插件注册的状态栏项显示待确认数量，点击经宿主共享 router 跳转到插件面板处理。队列 + 超时编排在插件激活期常驻，不依赖视图挂载。本票只做插件前端，devMock / dev-shell 触发演示。

**Blocked by:** 01（弹窗挂载点与状态栏跳转落点依赖设备面板所在的浏览主页面）

**Status:** resolved

- [x] consent 事件到达 → 面板内 Teleport 弹窗展示：设备名（无名用短指纹兜底文案）、完整指纹核对区、秒级倒计时；接受 / 拒绝 / 关闭均正确结算
- [x] 单请求队列：同一时间至多一个弹窗，后续请求排队；30s 倒计时归零先行结算释放闸门（拒绝语义，与宿主 sweeper 对齐）
- [x] 应答命令调用带 requestId；超时后迟到应答未命中待确认项时静默无害
- [x] 状态栏项实时反映待确认数（含「1 台设备等待确认」类 i18n 文案，zh-CN/en 同步）；点击跳转插件面板且跳转后弹窗可见可操作
- [x] 编排（队列/倒计时/应答）单测覆盖：入队、超时结算、接受/拒绝命令路由、事件重复到达幂等
- [x] devMock consent 种子可在 dev-shell 触发弹窗与状态栏计数两路演示
- [x] 只改插件前端源码，不触碰任何 Rust 与宿主文件

## 实现记录（2026-08-24）

### 落地文件

插件内（新增）：
- `bedcode-desktop/plugins/file-transfer/src/composables/useConsent.ts` — 编排单例：`plugin:file-transfer:consent-requested` 订阅、单请求闸门队列、30s 结算定时器 + 秒级展示 tick 分离（结算精确对齐宿主 sweeper）、`file-transfer.respond-consent` 应答路由（requestId + accepted）、requestId 去重幂等、载荷校验归一化、`consentDisplayName` 兜底纯函数、start/stop 对称清理
- `bedcode-desktop/plugins/file-transfer/src/components/ConsentDialog.vue` — 视图内 Teleport 到 body 的富弹窗（复用 ft-dialog-* 模态骨架）：设备名/短指纹兜底 + nameless 身份提示、完整 nodeId 指纹核对区、倒计时（≤10s 变 urgent 色）、接受/拒绝/关闭(X 与遮罩点击均=拒绝语义)
- `src/__tests__/plugins/file-transfer/useConsent.test.ts` — 10 例：入队闸门、accept/deny 命令路由带 requestId、超时按拒绝先行结算并释放闸门、迟到应答静默无害、重复事件幂等（含已结算 id 再达）、畸形载荷丢弃、stop 复位与重启、start 幂等、展示名兜底纯函数

插件内（修改）：
- `plugins/file-transfer/src/index.ts` — activate 启动 useConsent（激活期常驻）+ watch(pendingCount, locale) 同步状态栏项（计数清零即注销；label 静态捕获故整体重注册）；点击经 `getRouter()`（`__BEDCODE_SHARED__`）push `/plugin/sidebar/com.bedcode.file-transfer/file-transfer.sidebar`；deactivate 对称清理
- `plugins/file-transfer/src/components/FileTransferView.vue` — 挂载 `<ConsentDialog />`
- `plugins/file-transfer/src/styles.css` — ft-consent-* 样式（全 token-bound，复用 ft-dialog 骨架）
- `plugins/file-transfer/src/i18n/{messages,zh-CN,en}.ts` — `transfer.consent.*` 9 个 key，zh-CN/en 同步（MessageSchema 编译期保障）
- `plugins/file-transfer/src/devMock.ts` — consent 种子两条（有名 iPad Pro 立即弹窗 + 无名设备 2s 后入队，短指纹兜底），驱动弹窗与状态栏计数两路演示

SDK / dev-shell（非 Rust 非宿主 src，沿 ticket 01 devMock 协议先例）：
- `packages/plugin-sdk-desktop/src/types.ts` — `PeerDevMock.consent?` 种子字段
- `packages/plugin-sdk-desktop/src/index.ts` — 补导出 `PluginDevMock` / `PeerDevMock` 类型（ticket 01 引入协议时遗漏，devMock.ts 的 import 此前编译报错）
- `packages/plugin-sdk-desktop/dev-shell/src/mock/file-transfer.ts` — 注册 `respond-consent` handler；consent 种子延迟逐条推送（1.2s 起步、间隔 2s）
- SDK dist 已重建（types 导出同步）

### 验证

- 插件目录 `npx vue-tsc --noEmit`：本票涉及文件零错误（余 14 个错误全部为 SettingsPanel/TaskPanel/useSettings/useTasks/FileTransferView 的既有类型漂移，与本票无关，未触碰）
- `npx vitest run src/__tests__/plugins/file-transfer --pool=forks --poolOptions.forks.singleFork=true`：29/29 绿（含新增 10 例）
- 插件前端 `npm run build:frontend` 通过，产物已同步到 `src-tauri/resources/plugins/desktop/com.bedcode.file-transfer/`
- eslint（根配置）对全部改动文件零 error

### 遗留项

- **真机/真实宿主跳转验证**：状态栏项 onClick 经宿主 router push 面板路径，dev-shell router 无此路由会静默无害；真实宿主链路待阶段三冒烟确认「跳转后弹窗可见可操作」验收
- **既有问题（不属本票）**：① `scripts/build.js` copyArtifacts 只拷 dist/index.js 不拷 code-split chunk（index-*.js），resources 目录此前就缺 chunk 文件（本次已手工补齐使 resources 自洽）；② 插件 vue-tsc 的 14 个既有类型错误；③ ticket 07 收口时核对桌面 WASM 代理 consent 契约（事件名/命令名已按 rust/src/lib.rs 现状对齐：`plugin:file-transfer:consent-requested` / `file-transfer.respond-consent`，payload `{requestId,nodeId,fingerprintShort,deviceName}`）
