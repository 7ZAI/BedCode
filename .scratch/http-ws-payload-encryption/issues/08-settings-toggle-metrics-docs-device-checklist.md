# 08 — 双端设置开关、metrics、文档与真机验证清单

**What to build:** 收尾集成。桌面设置页新增「链路加密」配置域（spec §6 全部参数：主开关 `enabled` **默认关** + `encryptHttp`/`encryptWsTerminal`/`encryptWsEvent` 三个通道子开关 + `allowPlaintextFallback`，调 01 的 get/set 命令）并展示本机 Kd 指纹供人工核对；移动端设置（连接/认证子页）对称配置域（`enabled` 默认关 + 通道子开关 + `strictMode` 默认关）+ 对端指纹展示；无 pin 时开启主开关引导先配对。metrics 埋点：`server/metrics.rs` 新增 encrypted_frames / decrypt_failures 计数（加密与失败路径递增）。文档：desktop `docs/code-map.md` server 节补 link_crypto 模块职责描述。真机验证清单在本 issue 内固化并执行归档：默认态回归（全开关关=与现状一致）、弱网长会话、后台杀进程重连（reauth + rekey）、双端开关四象限（spec 兼容矩阵 + 逐通道子开组合）、指纹人工核对流程、Wireshark 抓包抽查（开启后无明文 JWT/终端内容/配对码；默认关时确认行为不变）。

**Blocked by:** 06, 07

**Status:** ready-for-agent

- [ ] 双端开关四象限行为符合 spec 兼容矩阵；逐通道子开关独立生效（关 HTTP 只影响 REST，WS 不受影响）
- [ ] **默认配置（全部关）下双端行为与现状完全一致**（回归既有测试全绿）
- [ ] 无 pin 时移动端开启主开关被引导先配对，不产生半启用状态
- [ ] 设置 UI 通过 frontend-styles 自查（token-bound、无原生控件外观）
- [ ] i18n key zh-CN / en 全部成对
- [ ] metrics 计数在加密成功/解密失败路径均可观测
- [ ] code-map 更新；`npm run test:run` + `cargo test --lib` 全绿；改动涉及 Kotlin 则跑 `./gradlew :app:compileUniversalDebugKotlin`
- [ ] 真机清单执行记录（截图/日志）归档至本目录
