# 04 — 首连确认 UI + 可信对端管理 + 终端配对自动互信

**What to build:** 把 02 的确认闸门接到两端真实 UI：陌生节点发起连接时被连接方弹应用内确认框（展示对方设备名 + 短指纹），同意即建立信任、拒绝即本次失败；此后两节点间不再弹窗。设置面新增可信对端管理：列表（设备名/指纹/加入时间）、撤销单项。迁移规则落地：已有终端配对关系的手机与桌面首次对等接触免弹窗直接互信。

**Blocked by:** 02, 03

**Status:** in-progress（双端代码与自动化测试完成；AC#1/#4 待人工真机走查）

- [ ] 真机双端走通：A 点击连接 → B 弹窗 → 接受 → 双方信任落库；重连无弹窗
- [x] 拒绝后 A 收到失败态且可稍后重试（crate 层 Denied 帧 → `DialDeniedByPeer` 已在 ticket 02 harness 断言；本票前端拒绝路径接通）
- [x] 设置中可查看并撤销；撤销后对端重连重新弹窗
- [x] 已终端配对的设备对首连无弹窗自动互信（前端层按设备名匹配，见 Comments 设计说明）
- [x] 新增文案 zh-CN / en 同步；UI 遵循 frontend-styles 规范（token-bound、复用 Modal/ConfirmDialog、无原生控件）

## Comments

### 实现记录（ticket 04）

- crate `packages/peer-net/src/trust_store.rs`：格式升 v2（`{version, peers:[{node_id, name?, added_at?}]}`），v1 文件原位迁移（名称留空、时间取文件 mtime）；新增 `add_with_metadata`（新插入带名 / 缺名补注幂等）与 `list_entries()->TrustedPeerEntry`；`contains/add/remove/list` 语义不变，transport 零改动。新增单测：元数据 roundtrip + 补注不覆盖、v1 迁移、未来版本 fail-fast
- 两端宿主 `src-tauri/src/peer_net.rs`（镜像改动）：`drive_gate_auto_deny` 替换为 consent 桥——`ConfirmRequested` → 发现缓存解析设备名 → 登记 request_id↔oneshot → emit `peer-consent-requested`；新命令 `respond_peer_consent`（接受路径先带名落库再回执）、`list_trusted_peers`（持久名优先、在线缓存名兜底合并）、`revoke_trusted_peer`；`PeerNetState` 扩展 trust 句柄槽（节点停止后设置面仍可查看/撤销，惰性加载）
- 前端两端同构：`useTrustedPeers.ts`（命令封装）+ `usePeerConsent.ts`（事件监听/单条队列/30s 超时主动拒绝，超时值与 crate `CONFIRM_TIMEOUT` 对齐；迁移规则判定在此层）+ `PeerConsentDialogHost.vue`（桌面挂 DesktopLayout、移动挂 MobileLayout，Teleport z-50）
- **迁移规则设计说明**：peer-net 节点身份（Ed25519 随机）与终端配对指纹刻意分离（决策 D2），跨身份强映射属后续票；v1 以「请求方 mDNS 设备名 ∈ 本机终端配对名单」为匹配键，在前端判定（桌面 deviceStore.pairedDevices、移动 localStorage paired_devices——两端 Rust 侧均不持有对侧名单）。无名记录不参与匹配（宁可多弹一次窗）；自动互信即时回执、toast 告知，不占用弹窗通道。已知局限：同网攻击者可将设备改名冒充已配对名绕过弹窗（收益仅文件传输且仍受全局接收策略把关），ADR 0028 接受该便利性取舍，后续票引入跨身份密码学绑定后移除
- 设置面：桌面 SettingsView 新增「可信对端」section（名称+短指纹/完整 ID/加入时间 + 撤销确认 Modal）；移动端新增 TrustedPeersView 子页（安全分组入口，ConfirmDialog danger 确认撤销）；i18n `settings.peer.*` zh-CN/en 双语同步
- 验证：crate `cargo test` 全绿（45 lib + 9 harness）；desktop `cargo test --lib` 576 全绿、mobile `cargo test --lib` 393 全绿；desktop vitest 456 全绿（含 usePeerConsent 9 用例：迁移匹配/入队/结算/自动互信/超时/名单动态变化）、mobile vitest 222 全绿；两端 vue-tsc 无新增错误、ESLint 0 error。真机双端弹窗走通与重连静默待人工冒烟
