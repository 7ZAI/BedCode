# 10: 会话原语整 interface 退役 + ABI 25 → 26（契约一次定稿）

**What to build:** 宿主能力面上**没有「会话」这个原语域**了：整 interface 删除、ABI 一次性 bump、
所有下游同步点在同一张票里定稿。对插件作者是唯一一次破坏——按「旧产物需按新 SDK 重建」的
既有先例处理，失败要点名原因，不得静默断流。

**Blocked by:** 03（注册面/修饰链先裁定并退役）、04（连接清单先搬家）、08（宿主命令面先干净）、
09（事件面先不依赖内核登记）。**这四张全部完成后才能开工本票。**

**Status:** done（2026-09-24 落地；ABI 26 → **27**；人工核验项待 01 基线复跑，见票末）

## 为什么契约集中在这张

本专项只剩这一处会动 ABI（P1-b 的事件载荷是 JSON 增量、05 是字段级追加，都不 bump）。
把破坏性变更收敛成一个版本点，插件重建一次即可，而不是每张票 bump 一回。

**但它在仓库范围内不是「唯一的 bump」**：并发批次
`.scratch/2026-09-24-host-crypto-business-downsink/` 的票 03（`host-crypto` WIT 契约 + 权限位 + ABI）
也要 bump，且它的票 07 明文写着「随会话原语剩余函数退役（v26 批次）」。

**版本号定序（2026-09-24 用户裁定：串行，本专项先 bump）**：

- 本票携带 **25 → 26**（`host-session` 整 interface 退役 = 桌面 SDK 的 v26 批次）；
- 对侧 `host-crypto` 票 03 走 **26 → 27**，其 blocked-by 指向本票（它现在的票面按 v26 写，
  需在对侧批次内改记为 v27 —— 本会话不代改对侧票文件，只在此登记裁定，转告即可）；
- 对侧票 07「随会话原语剩余函数退役」的依赖方向与本裁定一致，无需调整。

版本号撞车的表现不是编译错，而是**产物摘要与 ABI 声明互相冒充**——两批插件产物重建会互相覆盖，
事后极难归因。开工本票时先 `git log` 确认没人抢先把 `abi.rs` 推到 26。

## 验收标准

- [ ] 开工前再点一次消费者：会话 interface 的每个函数**零生产调用方**
      （注册两条按 03 的裁定处置；连接清单已按 04 迁走；其余九条 P1-b 起零消费者），
      有任何一条仍有消费者就停手回报
- [ ] WIT 删除整 interface + 宿主实现摘除 + SDK 侧对应 trait/绑定同步删除
- [ ] ABI bump 四处同步：WIT、`abi.rs` 常量与断言、CHANGELOG、AGENTS §7
      （含「双端偏离」段的计数与「旧产物按新 SDK 重建」的说明）
- [ ] 权限词汇零漂移：因面消失而退役的位（会话读写/观察一类）按五同步点落
      ——SDK 词汇表 → 生成物 → 宿主能力清单 → 宿主权限门 → 构建链映射表 + 前端合法集；
      跑 SDK 的 `gen:permissions` 重出，词汇漂移锁必须绿
- [ ] 旧产物的失败形态可诊断：拿旧 SDK 产物加载时，激活期报**点明原因**的错误
      （哪个 interface 没了 / 要按哪个版本重建），不是 trap 或静默降级
- [ ] 门禁：宿主 lib + 集成 8 target 全绿、**四插件产物全部按新 SDK 重建**、
      插件 native 全绿、桌面前端全绿、根 `eslint .` 0 error
- [ ] 移动端零改动确认：本票动的都是桌面独有 interface（移动端 WIT 里没有），
      记一句确认结论

## 边界与不做

- 不删内核会话实现目录（11）——本票只删「宿主对外能力面」。
- 不顺手改其他 interface 的函数。
- 不与 06/07 的输出面改动混提交（各自独立可回滚）。

## Comments

### 2026-09-24 · 开工前必须先重裁版本号（§3b-1 的撞号已成代码事实）

本票原按「25→26」写。实测并发批次
`.scratch/2026-09-24-host-crypto-business-downsink/` 票 03 的**未提交**改动已把
`packages/plugin-sdk-desktop/rust/src/abi.rs::ABI_VERSION` 写成 **26**
（其 `host-crypto` 注释同标 v26），而 §3b-1 的裁决是「本专项 10 走 25→26，对侧走 26→27，
对侧票面自行改记」。也就是说：谁先提交谁占 26。

因此本票施工第一步是**确认当时 HEAD 的 `ABI_VERSION` 实际值**，再决定本票携带 `26→27`
还是 `25→26`，并把结论同步回 §3b-1 与本票标题（标题里的 `abi-26` 可能要改）。
连带影响：票 04 的 `host-connection` 追加刻意**不 bump、不写版本号**（见票 04 §3），
所以无论 10 最终落在 26 还是 27，04 都不需要重做 ABI 记账。

**结论（2026-09-24 开工时实测）**：对侧已提交，`ABI_VERSION = 26` 是代码事实
→ 本票携带 **26 → 27**，标题 `abi-26` 只作历史留痕不改（改名会断 issue 引用）。

### 2026-09-24 · 落地：`host-session` 整 interface 退役 + ABI 26 → 27

#### 一、消费者普查（开工前再点一次，票面验收第 1 条）

逐函数扫描 `plugins/*/rust/src` + `packages/*`：

| 函数 | 生产调用方 |
| --- | --- |
| `list-sessions` / `get` / `create-with-spec` / `close` / `remove` / `rename` / `resize` / `annotate` / `output-ring-fetch` / `lifecycle-register` / `input-register` | **零**（`task/mod.rs` 里命中那几处是**负向锁的字符串**，不是调用） |
| `connections-list`（`host-session` 侧旧别名） | **1 处**：插件 `devices.rs` 的 `WasmHost.connections_list()`——票 04 已把它迁到独立原语 `host-connection`，**调用名不变**，只是 trait 来源从 `HostSession` 换成 `HostConnection`（改一行 import） |
| `packages/plugin-sdk-test` 的 `test_session_list` | 夹具命令（非生产），随本票删除 |
| `packages/*-test` 夹具的手写绑定 | 从共享 WIT 生成，随 interface 删除同步清理 |

结论：除 `connections_list` 一处换来路（票 04 预留的迁移动作）外**零生产调用方**。

#### 二、连带删除的第二个 interface：`host-terminal`

普查时发现 `host-terminal`（只有 `send` 一个函数）**实现 100% 依赖会话域**
（`terminal_send` 先查 `ensure_session_owner` 再 `SessionManager::write_input`），
会话域一删它就编译不过；而它自己也是零消费者（P1-b 后属主判定对真实会话恒拒）。

按票面「本票只删**宿主对外能力面**」的口径，把它一并删除（留到票 11 只会得到一个
编译不过的文件）。宿主能力清单因此 **23 → 21**（删 `host-session` + `host-terminal`），
`capability.rs` 的计数注释与 AGENTS §7 同批更新。

#### 三、同批收口的两个「失去派发源」的导出

票 03 把 WIT 面的删除留给本票，故本票一并删：

- `interface terminal-hooks`（整 interface）：逐帧输入修饰链与终端输出修饰链的
  回调宿主都不再调用（票 03 已拆到只剩输出侧，而输出侧服务的是业务输出环）；
- `events` 的 `on-session-lifecycle` / `on-input-submitted`：派发源在票 03 就没了。
  `on-message` / `on-process-done` 保留（仍是必选导出，仍被调用）。

插件侧对应实现（`terminal-session` 的两个回调 override、两个夹具的 `Guest` 实现）
与 SDK 侧（`WasmPlugin` 的两个默认方法、`wasm_entry!` 的两段生成、
`SessionLifecycleEvent` / `InputSubmittedEvent` 两个类型）同批删除。
**删除不是能力收窄**：两个回调上的动作在插件自驱路径上逐条有等价实现，已在
`plugins/terminal-session/rust/src/lib.rs` 的删除处写明对应关系。

#### 四、权限词汇退役（五同步点）

退役 `session:write`（宿主会话写原语的判据位，随 interface 死）与
`terminal:observe`（提交输入行观察面，派发点票 03 已删）。**`session:read` 保留**：
它现在门住的是宿主终端窗口事实（`predictTerminalSize` / `openTerminal` /
`closeTerminal` / `isTerminalOpen` 四个前端原语），前端描述文案按新判据重写。

五同步点逐一落：

1. SDK 真源 `rust/src/permission.rs`：删两个常量 + 反射表两行 + API 映射两行；
2. 前端生成物 `src/plugin/permission-vocabulary.ts`、3. CLI 生成物
   `bin/permission-vocabulary.json`：`gen:permissions` 重出（34 → **32** 条）；
4. manifest 断言三处：插件 `plugin.json`（真源）、插件 Rust 契约用例
   （`declares_only_landed_domain_surface`）、插件前端契约用例
   （`plugin-contract.test.ts`）、宿主 e2e 的清单 pin（`session_e2e.rs`）；
5. 前端锁：`permissionVocabulary.test.ts` 的门禁点基线下界 **19 → 18**；
   `manifest-gen.test.ts` 的样例改用仍存在的 context API（`terminal.onInput` /
   `session.predictTerminalSize` 替代已注销的 `sendInput` / `list` / `stop`，
   否则生成器推断不出权限，是**假红**）；i18n 与 `contributionKinds` 的展示条目
   同步删除（并顺手把 `session:read` 的文案改成实际判据面）。

#### 五、旧产物诊断（票面验收第 4 条）

v27 之后旧产物**在实例化阶段就失败**（找不到 import 的实现），比 `verify_abi` 的
版本协商更早——组件模型不提供「向后兼容的缺省 import」，失败本身不可避免，能做的是
让失败**可诊断**。新增 `LoadedWasmPlugin::stale_artifact_rebuild_hint`：命中
「缺失 import / 缺导出」形态的实例化错误时，在 wasmtime 原文后追加一句
（点明 `host-session` / `host-terminal` / `terminal-hooks` 与「按当前 SDK 重建」）；
与契约无关的失败（WASI 缺目录、trap）**不追加**，避免噪音掩盖真因。

**诚实记账**：端到端那一半（真拿一个 ABI v26 产物加载）**未复现**——插件产物不入库、
旧 SDK 与旧 WIT 都已被本票改掉，本环境下没有可加载的旧产物。故把判据抽成纯函数并单测
（`stale_artifact_instantiation_hint_is_selective`：三种命中形态 + 两种反例），
「wasmtime 缺失 import 的文案确实点名 interface」这一前提按组件模型的报错习惯取信；
端到端验证留给**人工**（与 01 清单一起）。

#### 六、测试改写（判据从「走通旧路径」改为「走生产路径」）

`session_e2e` 里三处原先直接喂合成事件 / 直接调已删回调的地方改走生产路径，
**顺带把覆盖提升到真实链路**：

- `test_session_task_domain_closed_loop` 第 5 步（未适配 agent 不写集成）：
  原喂合成 `Creating` 事件 → 改经真实 `session.create`（`ensure_agent_integration`
  只存在于 `launch::spawn_session` 内）；
- 同用例第 6 步：输入改经 `session.input`（真实写 PTY + 提交行重建 + 任务域分发），
  终态改经 `session.close`（真实 kill → `pty:exit` → 兜底中断），并按**有界轮询**
  等待收敛（终态事件到达 ≠ 收尾帧已消费）；该会话的配置命令改为
  `echo claude && exec bash`（保留 `claude` 关键词让 agent 判定命中，同时让 shell
  **驻留**——原 `claude` 命令在无该 CLI 的机器上会立刻退出，PTY 写入必失败）；
- `test_session_task_http_and_scheduled_closed_loop` 第 5 步（`creating → executed`）：
  原喂合成 `Created` 事件 → 改为「真实创建会话拿 sid → 把 creating 态 job 的
  session_id 播成该 sid → `session.action.restart` 触发同 id 重建（跑 Created 逻辑）」；
  该用例的授权清单补 `pty:spawn` / `pty:io`（它此前不创建会话）。

#### 七、门禁实测

- 宿主 lib：**1116 passed / 0 failed**（较票 09 的 1148 少 32 = 删除的
  `host_api/session.rs`（整文件，含其全部用例）+ `host_api/lifecycle.rs` +
  `host_api/terminal.rs` + 一条旧别名用例，净加 1 条新诊断用例）
- 集成 8 target **逐个串行全绿**
- 插件 native：`terminal-session` **298** / `agent-hub` **98** / `file-transfer` **59**
  / `ai-chatbox` **15**，全 0 failed
- 桌面前端全量：**80 files / 779 tests 全绿**
- 根 `eslint .`：**0 error**（120 warning 与基线同数）
- **四个插件产物全部按新 SDK 重建**（wasmHash `2eed7810…` / `1a57fa14…` /
  `c4c46f20…` / `9f4b71b1…`）
- 期间观察到 `pty_e2e` 两条用例在全量并发下抖动失败、单独复跑 5/5 全绿
  （业务 PTY 时序敏感，非本票改动引入）。

#### 八、记账

- 票面验收第 5 条（01 清单复跑会话创建/停止/列表/输入四项）**未跑**：01 人工基线本轮
  仍为零条观测。用户 2026-09-24 裁：本会话连前置一起做，人工验收项后置。
  据此本票在「人工核验」一项上**不算勾**。
- 本票**不动内核 `src/session/` 目录**（票 11 的删除面）；但删掉宿主能力面后，该目录
  已无外部消费者、「删目录」在编译层面已经可行——这正是票 11 的开工条件。
