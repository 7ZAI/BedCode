# 08b: 真机六链路冒烟（从 08 拆出，ready-for-human）

**What to build:** 桌面端真实起服，同局域网浏览器/移动端逐条验整条 server 拆分对用户可见行为零影响。这是九票里唯一无法由编译器/单测担保的部分——08a 的结构锁只证明「依赖方向没腐」，六链路证明「端到端还能用」。

**Blocked by:** 08a（结构锁已落；锁红说明分层被改坏，先修锁再冒烟）。

**Status:** ready-for-human

**来源:** `../spec.md` §7 六条链路 + `08-dependency-direction-lock.md` 冒烟清单。08a 已把本机可独立完成的 HTTP 子集跑完（见 08a Comments 第 5 条），本票只列**剩余**项与需真机/插件激活的项；也可整单复跑一遍作总验收。

## 已由 08a 覆盖（可复跑确认，不必重测）

- [x] `GET /api/health` → 200 `{status,port,uptime_secs}`（08a 2026-09-23，:8767）
- [x] `GET /static/terminal-bg` 未设背景图 → 404（08a 同日）
- [x] `GET /api/sessions` 无 JWT → 401 `code:1007`（08a 同日）
- [x] `GET /api/configs` 无 JWT → 401（08a 同日，证明网关在验签之后）

## 本票待验

- [ ] `GET /api/sessions` **带** JWT 正常返回（`/api` scope 与验签中间件层级未变——配对拿 token 后验）
- [ ] 移动端配对后 `/ws/event` 连接建立、设备在线判定与同步广播正常（事件通道 + 首消息认证）
- [ ] `/ws/terminal/session/{id}` 输出帧可见、resize / 输入可达（终端链路 + TB v3 二进制帧未变）
- [ ] 插件 WS `/ws/plugin/com.bedcode.terminal-session/{path}` 连接与收发正常；未注册路径 **404**、入站超上限 **503**（票 07 拒绝时序未变）
- [ ] **链路加密开关开/关各跑一遍**上述 WS 两条（`filter` / `link_crypto` 已进 `core/`，两域共用责任链是最易被静默改坏的共享面）
- [ ] `GET /api/configs` PluginRequired 档：插件激活时**转发**、未激活时**明确报错**（证明票 01/04/05/07 四次重指向后的网关锁真干活；08a 只验了无 JWT 401 半边）
- [ ] 浏览器（可选加验）：`/static/terminal-bg` 设过背景图时返回图片二进制（08a 只验了未设 404 半边）
- [ ] 冒烟结果与日期写回本票 Comments；跑完清理后台进程与监听端口（AGENTS §3；GTK 子进程要 `kill -9` 复核，见 07 Comments）

## 归属与真源

来源：`../spec.md` §7、`08-dependency-direction-lock.md` 冒烟清单与「拆 08a/08b」Comments 预案；真机门禁历史挂 `.scratch/2026-09-18-ws-base-service/spec.md` §6。

## Comments

- 2026-09-23 立项：08a 落锁后按票面预案 + 用户裁决「拆 08a+08b」拆出。08a 已完成 HTTP 四条本机子集（health / terminal-bg 404 / sessions 401 / configs 401），本票承接需真机配对、JWT 正向、插件激活转发、WS 三条与加密双跑的剩余链路。
