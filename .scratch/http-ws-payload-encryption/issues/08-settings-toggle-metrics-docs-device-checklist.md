# 08 — 双端设置开关、metrics、文档与真机验证清单

**What to build:** 收尾集成。桌面设置页新增「链路加密」配置域（spec §6 全部参数：主开关 `enabled` **默认关** + `encryptHttp`/`encryptWsTerminal`/`encryptWsEvent` 三个通道子开关 + `allowPlaintextFallback`，调 01 的 get/set 命令）并展示本机 Kd 指纹供人工核对；移动端设置（连接/认证子页）对称配置域（`enabled` 默认关 + 通道子开关 + `strictMode` 默认关）+ 对端指纹展示；无 pin 时开启主开关引导先配对。metrics 埋点：`server/metrics.rs` 新增 encrypted_frames / decrypt_failures 计数（加密与失败路径递增）。文档：desktop `docs/code-map.md` server 节补 link_crypto 模块职责描述。真机验证清单在本 issue 内固化并执行归档：默认态回归（全开关关=与现状一致）、弱网长会话、后台杀进程重连（reauth + rekey）、双端开关四象限（spec 兼容矩阵 + 逐通道子开组合）、指纹人工核对流程、Wireshark 抓包抽查（开启后无明文 JWT/终端内容/配对码；默认关时确认行为不变）。

**Blocked by:** 06, 07

**Status:** ready-for-human（代码/UI/metrics/文档已完成；真机清单待真机环境执行）

- [x] 双端开关四象限行为符合 spec 兼容矩阵；逐通道子开关独立生效（关 HTTP 只影响 REST，WS 不受影响）（单测覆盖：桌面 config/filter 矩阵 + 移动 isChannelEncryptionActive）
- [x] **默认配置（全部关）下双端行为与现状完全一致**（回归既有测试全绿：桌面 cargo test 全目标 + 移动 vitest 296）
- [x] 无 pin 时移动端开启主开关被引导先配对，不产生半启用状态（ConnectionSettingsView onToggleLinkEncryption 守卫）
- [x] 设置 UI 通过 frontend-styles 自查（token-bound、无原生控件外观；沿用两端既有 section/row/Toggle 蓝图）
- [x] i18n key zh-CN / en 全部成对
- [x] metrics 计数在加密成功/解密失败路径均可观测（encrypted_frames / decrypt_failures，过滤器 Ok/Err 分支埋点）
- [x] code-map 更新；`npm run test:run` + `cargo test` 全绿（无 Kotlin 改动）
- [ ] 真机清单执行记录（截图/日志）归档至本目录 —— **待真机环境**，清单见下节

## 真机验证清单（待执行归档）

> 执行环境要求：真机 + 桌面端同一 WiFi；每项完成后将截图/日志归档至本目录（命名 `08-artifact-<编号>-<简称>.{png,log}`）。执行前置：双端安装包含 issue 01-08 的构建。

### 1. 默认态回归（全开关关 = 与现状一致）
- [ ] 双端全新安装、不做任何加密配置 → 配对/终端/文件传输全流程正常
- [ ] 桌面抓包确认默认关时报文明文（行为与旧版一致）

### 2. 开启后基础链路
- [ ] 双端开启主开关（其余默认）→ 重连后终端流可用、延迟无可感知劣化
- [ ] Wireshark 抓包抽查：无明文 JWT / 终端内容 / 配对码（过滤 TCP payload 人工检视 + `strings` 抽查）

### 3. 弱网长会话
- [ ] 真机弱网（热点限速/隔墙）持续终端会话 ≥30min：无卡死、重连后自动恢复加密

### 4. 后台杀进程重连（reauth + rekey）
- [ ] 移动端后台杀进程 → 重开 → 自动重连 → reauth 成功且重新协商新会话密钥
- [ ] 杀进程期间桌面侧推送会话输出不崩溃，恢复后继续收到

### 5. 双端开关四象限（spec 兼容矩阵）
- [ ] 桌面开+移动开：全链路加密
- [ ] 桌面开（allowPlaintextFallback=false）+移动关：移动连接被拒（strict 服务端策略）
- [ ] 桌面关+移动开：协商不命中，按 allowPlaintextFallback 语义回退/提示
- [ ] 桌面关+移动关：明文（现状）

### 6. 逐通道子开关组合
- [ ] 关 encryptHttp：REST 明文、WS 终端流仍加密
- [ ] 关 encryptWsTerminal：终端流明文、REST 仍加密
- [ ] 关 encryptWsEvent：同步事件明文、其余不受影响

### 7. 指纹人工核对流程
- [ ] 桌面设置页展示本机指纹 ↔ 移动端连接设置页对端指纹逐字符一致
- [ ] 删除桌面身份文件重启 → 指纹变化 → 移动端 pin 失效表现符合 spec（拒绝/重新配对引导）

### 8. strictMode（移动端）
- [ ] 移动 strictMode 开 + 桌面降级（关子开关）→ 移动端断连并报错，不静默明文续跑
