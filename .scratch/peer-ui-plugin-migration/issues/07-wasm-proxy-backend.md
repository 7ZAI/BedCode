# 07 — WASM 代理命令面与事件桥落地（两端 Rust）

**What to build:** 让 01–05 的页面从 mock 切到真实数据：两端 file-transfer 插件的 WASM 薄代理补齐五条转发命令（拨号连接、断开会话、首连确认应答、可信对端列表、撤销信任），增订首连确认与连接态两个总线 topic 桥接为插件命名空间事件；manifest 显式声明 peer 权限；按既有构建脚本重建产物同步打包资源后，真实宿主中设备面板显示真实发现节点、可真实建立连接、对端能收到确认弹窗。

**Blocked by:** 06

**Status:** ready-for-human（2026-08-25 代码面全部完成；实机冒烟需双端同网环境，转人工执行）

- [x] 五条命令逐一映射到既有 host-peer 原语，DTO JSON 过界形状与既有命令一致；错误以可读字符串过界
- [x] consent topic → 插件首连确认事件、connection topic → 插件连接态事件，camelCase 载荷原样透传；activate/deactivate 订阅对称
- [x] 两端 manifest permissions 补声明 peer；contributes.commands 补登新命令
- [x] 两端插件重建（vite + cargo wasm + componentize）并同步打包资源目录，dev 与打包行为一致
- [ ] 实机冒烟（双端同网）：设备面板出现真实对端、可连接/断开、被连端弹确认、信任后可互传一批文件；撤销信任后重连重新出确认
- [x] 桌面侧定稿前已先行落地的桥接片段在本票内核对收口（不重复实现）

## Comments

### 执行记录（2026-08-25）

**移动端 WASM 代理补齐**（`bedcode-mobile/plugins/file-transfer/rust/src/lib.rs`）：

- 设备区新增 `dial-peer` / `disconnect-peer`，新设「信任层」分区 `respond-consent` / `list-trusted` / `revoke-trusted`——与桌面端逐行同构：参数键（nodeId/requestId/accepted）、返回 JSON 形状（existed/hit/removed/DTO 数组）完全一致。
- activate/deactivate 增订 `peer:consent` / `peer:connection` 订阅与对称退订；on_bus_message 将两 topic 原样透传为 `plugin:file-transfer:consent-requested` / `plugin:file-transfer:connection-changed`（宿主载荷已是 camelCase 契约形状，不过翻译层）。
- 模块文档同步补充透传语义说明。

**桌面侧先行片段核对收口**（`bedcode-desktop/plugins/file-transfer/rust/src/lib.rs`）：

- 五条命令与两 topic 桥接在 spec 定稿前已落地，逐项对照前端契约核对无偏差（usePeerDevices/useConsent/useTrustedPeers 所调命令名与事件名全部命中），未重复实现；仅修正一处过时注释（「issue 04/08 职责移交」→ 与移动端一致的现状描述）。
- 宿主侧链路核验：桌面 `peer_net.rs` emit_json 已桥接 peer-consent-requested → peer:consent、peer-connected/disconnected → peer:connection；移动端同构。移动 SDK WIT host-peer 接口与 src-tauri component.rs/host_impl 绑定齐备，`require_peer_permission` 门禁生效。

**Manifest 显式化**（两端 plugin.json）：

- permissions 按字母序插入 `"peer"`；contributes.commands 补登五条新命令。
- 移动端权限裁决点核实：APK asset 内置插件跳过审批门（manager.rs 仅对外部安装源校验），load 时 granted_permissions 直接取 manifest.permissions——声明即生效，无需重新审批。

**重建与资源同步**：

- 桌面 `node scripts/build.js`（vite + cargo wasm32 release + componentize + 拷贝 resources）；移动 `node scripts/plugin-build.js --plugin com.bedcode.file-transfer`。产物均已同步至各自 `src-tauri/resources/plugins/<端>/com.bedcode.file-transfer/`，resources 内 plugin.json 已含 peer 权限与 28 条命令。

### 附带修复：桌面打包产物缺 chunk（本票范围「dev 与打包行为一致」缺陷）

- 根因：切收口提交 20c2b81c 在桌面 useSettings.ts 引入动态 `import('@tauri-apps/plugin-dialog')` → vite lib 构建代码分割出额外 chunk，而 build.js 与 plugin-watch.js 只同步 index.js 到宿主资源目录——打包后入口 import 相对 chunk 缺失，真实宿主加载必失败（此前 resources 里遗留的 `index-CCTTTi_x.js` 即该问题的手工补救痕迹）。移动端插件源码无动态 import，不受影响。
- 修复：桌面 vite.config.ts 增加 `rollupOptions.output.inlineDynamicImports: true`，产物回归单文件 index.js 契约（与 auto-task 单产物设计同理），动态 import 语义由 rollup 内联保持；构建/watch 脚本无需改动。

### 验证

- 两端插件 crate `cargo test` 编译通过（薄代理无单测，行为真源在宿主 crate 既有集成测试，符合 spec Testing Decision 1）
- 桌面 `npm run test:run` 58 文件 / 532 用例全绿（首轮 1 例 worker OOM 为基础设施抖动，复跑通过）；移动 31 文件 / 306 用例全绿
- 产物核验：桌面 dist 单文件 176.5 kB 无相对 chunk 引用；两端 resources 目录各含 index.js + plugin.json + wasm（+ 移动 style.css），plugin.json 含 peer 权限与新命令

### 待人工：实机冒烟

需双端同网真机环境（桌面 + Android 手机），验证矩阵按 What to build：发现互见 → 连接/断开 → 被连端确认弹窗 → 信任后互传一批文件 → 撤销信任后重连重现确认。建议顺带覆盖：桌面状态栏待确认项跳转、移动终端配对自动互信 toast、拨号 denied/unreachable 行内错误三态。
