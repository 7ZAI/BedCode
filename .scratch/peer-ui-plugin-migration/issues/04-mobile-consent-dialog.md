# 04 — 移动插件·首连确认（全局对话框 + 配对迁移规则）

**What to build:** 移动端陌生节点首次连接本机时，用户无论身处哪个页面都会收到全局确认对话框（设备名/短指纹 + 超时自动拒绝提示）；已通过终端配对的同用户设备命中迁移规则——静默自动互信并以 toast 告知，不弹窗。编排在插件激活期常驻。本票只做插件前端，consent 事件由 devMock / dev-shell 触发演示。

**Blocked by:** None — can start immediately

**Status:** resolved

- [x] consent 事件到达 → 经插件对话框 API 全局弹出确认框：标题 + 设备名（无名用短指纹兜底文案）+「{seconds} 秒内未处理将自动拒绝」提示
- [x] 信任 / 拒绝经应答命令结算；对话框关闭视为拒绝；30s 超时后迟到的应答未命中待确认项时静默无害（与宿主 sweeper 结算语义对齐）
- [x] 迁移规则：请求方设备名命中终端配对名单（本地持久化的 paired_devices）→ 静默自动互信 + autoTrusted toast；匹配为无头纯函数，「宁可多弹勿误信」（无名记录不参与匹配）
- [x] 配对名单读取容错：存储缺失/损坏时回退空名单（退化为正常弹窗，不报错）
- [x] 迁移规则纯函数单测；编排单测覆盖自动互信路径与弹窗路径的分叉
- [x] devMock consent 种子可在 dev-shell 触发弹窗与自动互信两路演示
- [x] 只改插件前端源码，不触碰任何 Rust 与宿主文件；i18n zh-CN / en 同步


## Comments

### 实现记录（2026-08-24，ticket 04）

**新增文件**
- `bedcode-mobile/plugins/file-transfer/src/composables/useConsent.ts` — 首连确认编排单例。迁移规则先行（`matchesTerminalPairedDevice` 纯函数，无名记录不参与），命中即 `respond-consent(accepted:true)` + success toast、不占闸门；未命中的走单闸门队列，经 `context.dialogs.showConfirm` 全局弹出（warning 变体，`dismissible: true` 关闭=拒绝）。结算三路（确认 / 拒绝或关闭 / 30s 超时）由 `settled` 标志保证幂等、恰好发送一次应答；settle 定时器与宿主 `CONFIRM_TIMEOUT` 同值先行结算释放闸门。配对名单读插件存储键 `paired_devices`，`normalizePairedNames` 容错缺失/损坏/异形条目回退空名单。对话框倒计时为静态提示——通用对话框 API 不支持内容动态刷新，精确结算不受影响。
- `bedcode-mobile/src/__tests__/plugins/file-transfer/useConsent.test.ts` — 17 例：纯函数 3（迁移规则匹配 / 名单归一化容错 / 展示名兜底）+ 编排 14（自动互信且弹窗占用时插队放行、名单损坏退化弹窗、确认/拒绝命令路由带 requestId、单闸门排队顺序展示、超时按拒绝结算 + 迟到对话框结果静默无害、编程式 deny、无展示时 accept/deny 无害、重复事件幂等、畸形载荷丢弃、无名不参与迁移规则、stop 忽略在途结果 + 重启恢复、start 幂等）。

**修改文件**
- `plugins/file-transfer/src/index.ts` — activate 常驻启动 `useConsent(context).start()`，deactivate 对称 stop。
- `plugins/file-transfer/src/devMock.ts` — 新增本地扩展字段 `consent: ConsentDevSeed`（SDK PluginDevMock 协议未收录，类型本地声明向后兼容）：`pairedDevices: ['小米 14 Pro']` 名单种子 + 三条延迟推送的演示请求（2s 配对设备 → 自动互信 toast；6s 陌生设备 iPad Pro → 弹窗；14s 无名设备 → 短指纹兜底 + 核对提示）。
- `plugins/file-transfer/src/i18n/{messages,zh-CN,en}.ts` — 新增 `transfer.consent.*` 八个 key（title/body/fingerprint/timeoutHint/namelessHint/trust/deny/autoTrustedToast），schema 与 zh-CN/en 三文件同步。
- `packages/plugin-sdk-mobile/dev-shell/src/mock/file-transfer.ts` — 通用接线：consent 种子存在时写入插件存储配对名单、注册 `file-transfer.respond-consent` mock handler（返回 `{hit:true}` 并记日志）、按 delayMs 推送 consent-requested 事件；无种子的插件零影响。

**验证**：定向 vitest `useConsent.test.ts` 17/17 绿（同目录 3 文件共 37 例全绿）；`npx vue-tsc --noEmit -p plugins/file-transfer/tsconfig.json` 干净；dev-shell `vite build` 通过；eslint 0 error。未触碰任何 Rust / 宿主 src / gen/android。

**遗留提示（阶段二/三衔接）**：① 插件存储的 `paired_devices` 目前仅 devMock 种子写入，真实链路的名单同步（宿主终端配对 → 插件存储）属阶段三后端接线；② 对话框倒计时为静态「{seconds} 秒」提示，如需逐秒动画需宿主对话框 API 扩展或插件自绘弹窗，留待阶段二截图评审再议。
