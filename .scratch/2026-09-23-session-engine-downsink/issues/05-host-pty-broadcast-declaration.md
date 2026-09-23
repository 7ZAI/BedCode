# 05: PTY 引擎级「宿主广播声明」+ 会话到句柄的只读映射

**What to build:** 让宿主 server 在**不跨 WASM 边界**的前提下知道「哪条插件 PTY 的输出允许被
广播给移动端」：插件 spawn 时按句柄 opt-in 声明，引擎侧据此维护一份只读的
「会话 id → PTY 句柄」映射。做完之后，06 才有东西可读——而这份声明本身就是安全边界：
**没声明的句柄任何宿主广播面都读不到**。

**Blocked by:** 无（可以立即开始）。

**Status:** done（2026-09-24 landed）

## 为什么这是「声明」而不是「默认开」

插件私有 PTY 的字节默认是插件的私有数据（历史上宿主广播只对内核会话开放）。
业务会话现在也是插件 PTY，所以「能不能被宿主读走」必须是**插件自己的决定**——
既然后续移动端必须能看，就让它显式声明，而不是给所有句柄默认开后门。

## 验收标准

- [ ] 声明走 spawn 的配置参数（函数级/字段级**追加**，按既有惯例不 bump ABI），
      参数名与语义在 WIT 契约注释里写清「宿主可只读订阅本句柄输出」
- [ ] 引擎侧只读映射：登记与摘除**复用**既有的句柄生命周期单点（注册即在册、终态即摘除），
      不新增第二份表；提供按会话 id 取句柄的宿主内部访问器
- [ ] **反向锁**：未声明的插件 PTY 不得被任何宿主广播面读到（这条必须是真的行为用例，
      不是注释——用例构造「声明 / 未声明」两条句柄，只有前者可被读到）
- [ ] 声明的读侧不做订阅（本票只交付引擎事实与映射；订阅与帧编排在 06），
      但要把「同一条句柄被宿主与插件**同时**拉」这条并发路径的现有截断/游标语义写清楚，
      给 07 当输入
- [ ] ADR 0022 的 host-pty 第 2 条措辞**在此刻**修订：从「不注册业务输出总线」改成
      「不默认注册；按 spawn 声明 opt-in 只读订阅」。P1-b 已经做过事实更正，
      措辞修订必须与声明字段同时落地（这是 spec 裁决 1 的连带必做项）
- [ ] 门禁：宿主 lib + 集成全绿、插件 native 全绿、产物重建、`eslint .` 0 error

## 边界与不做

- 不接移动端通道（06）。
- 不做性能结论（07）。
- 不 bump ABI（函数级追加；整体会话 interface 退役的 bump 在 10）。

## Comments

**实施记录（2026-09-24）**：

- **字段定名 `hostBroadcastSessionId`**（spawn config-json 可选，camelCase，字段级追加
  不 bump——spec §4.2 的暂名 `hostBroadcast` 在实施时弃用：布尔名配字符串值会误导，
  语义同时承担「opt-in 声明」与「映射键」两个角色，名字必须自文档化）。
- **四端同步**：WIT 契约注释（header「不注册业务输出总线 → 不默认注册…opt-in 只读
  订阅」+ spawn config 文档 + ring-fetch 多消费者注记 + v16 版本演进补记）／SDK
  `PtySpawnConfig::host_broadcast_session_id` + 序列化用例／宿主 `SpawnConfig` +
  `PtyEntry` 新增 `broadcast_session_id` + `registered_seq`，访问器
  `broadcast_handle_for_session`／插件 `launch.rs::spawn_session` 每次 spawn 声明
  （重启同 id 重建路径经 `registered_seq` 取最新，旧句柄终态不影响）。
- **映射复用既有句柄注册表（PTYS）**，不新增第二份表：登记随 spawn、摘除随终态，
  生命周期单点不变；结构锁 `broadcast_mapping_reuses_the_single_handle_registry`
  钉实现单点（行为锁另有生命周期用例）。
- **反向锁**：`broadcast_declaration_is_opt_in_and_undeclared_handles_are_invisible`
  构造「声明 / 未声明」两条句柄（未声明那条用 env 注入相同的会话 id，证明
  「环境里有会话 id」不等于「默认开」），只有前者可被读到——行为用例，非注释。
- **失败可见**：空串 / 他属主撞 id 在 spawn 显性拒绝（不静默当作未声明）；
  跨属主冲突在副作用之前早失败 + 登记处同一把锁内复查（并发竞态权威判据）。
- **并发语义写进契约（07 的输入）**：fetch 纯读、游标各调用方自持、淘汰由产出量全局
  驱动（读不释放空间）；环级用例 `reads_by_one_consumer_do_not_extend_another_
  consumers_window` 锁定；宿主直读不受 `PLUGIN_PTY_RING_FETCH_MAX_BYTES` 约束
  （那是 WASM 边界拷贝限额）。
- **ADR 0022 v15** 随本票同步（措辞修订 + 变更记录）；CHANGELOG 双语一条。
- **门禁实测**：宿主 `cargo test --lib` **1172/0**（含新增 7 项锁）、集成 8 target
  逐个串行全绿（pty_session_chain / ws_session_route / broadcast_shutdown /
  server_integration / build_manifest_smoke / ws_auth_rules / http_auth_biometric /
  link_crypto_http）、SDK `--lib` 117/0、插件 native 287/0、根 `eslint .` 0 error
  （120 warning 不计门禁）；插件产物重建（wasmHash `7514caba0c…`）；跑测后无残留
  进程/端口。
- **协作事实**：开工时对侧票 04（host-connection）在途，期间其提交
  `d8dead75b` landed；本批 7 个文件提交时只 add 本票文件（WIT/ADR 与 04 线已各自
  落盘，无重叠 hunk）。
