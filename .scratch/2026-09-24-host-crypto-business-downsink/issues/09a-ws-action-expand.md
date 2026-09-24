# 09a: WS 动作词表声明式化 · expand（接入声明式路由）

**What to build:** WS 会话/终端动作（启动/停止/删除/调整大小/订阅输出等）的**来源**从「宿主硬编码」改为「插件声明贡献」——对齐 `_http_endpoint` 模式：插件经 manifest 声明它支持的 WS 动作词表，宿主提供通用路由（收到帧 → 按声明的属主转发 → 回包）。本票只做 **expand**：新增声明式路由与旧硬编码分发**并存**，两类都工作，行为零回退。宿主不再内置「某动作是什么业务」的解释，只做声明驱动的转发。

**Blocked by:** P1-b land

**Status: ✅ done（2026-09-24；09b/09c 同批落地，见 09b/09c 票据）**

## 🔸 已落地（expand 声明面 + 注册）

- SDK `PluginContributes` 增 `ws_endpoints: Vec<WsEndpointContribution>`（两形态：`"path"`
  或 `{path, auth}`，与 httpEndpoints 同构），含 `WsEndpointContribution` 类型与四个单测
- manifest-validate.js：`contributes.wsEndpoints` 校验（形态 / path 非法 / auth none|jwt / 去重），线性 PS 测试
- 宿主 `register_declared_ws_endpoints`（register.rs）：激活成功时登记进 `server::websocket::endpoint`
  注册表（挂载 `/ws/plugin/<id>/<path>`），路径校验同 `ws_register_endpoint`、缺省档 none；
  **登记时机锚定激活期**——deactivate 会 `purge_for_plugin` 回收 ws 端点，只有激活期重登记才
  能在 deactivate→activate 循环后不丢（与 httpEndpoints 的 load 期登记不同，ws 生命周期随激活）
- 门禁：宿主 lib 一个目标测试（declared ws 端点注册 / 缺省 none / 显式 jwt / 非法 path 不登记）+
  SDK types + manifest-validate + 前端 fixtures drift 全绿
- "未声明动作为宿主不解释"：未登记的路径由 `find_by_mount` 查不到 → 404（既有通用路由行为）

- [x] 存量会话控制动作 **接入**声明式路由并验证帧时序（见 09b：`pty_session_chain` 经转发层全绿）
- [x] 未声明插件对应动作不可达（声明闸门显性报错，session_e2e 转发用例反例断言）
- [x] 验收清单其余项随 09b / 09c 收口

**收口（09b/09c 已落地）**：声明式路由与既有硬编码 switch 的并存运行（集成断言零回退）→
宿主 /ws/event 改走转发层、旧 switch 删除（09c）。

- [ ] 插件可声明 WS 动作词表（静态路由），宿主通用路由对声明动作转发到插件；未声明动作为宿主不解释（显式路径）
- [ ] 声明式路由与既有硬编码 switch 并存，存量动作走旧路径仍工作（集成测试断言零回退）
- [ ] 无权限位/未声明插件对应动作不可达（fail-visible，不静默）
- [ ] SDK/manifest 同步点：插件声明相关字段的校验与打包链一致