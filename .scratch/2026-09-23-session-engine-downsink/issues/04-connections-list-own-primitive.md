# 04: 连接清单迁独立原语（不随会话 interface 一起死）

**What to build:** 「某会话当前被哪些设备/窗口连着」这份**宿主 server 自己的连接事实**换一个
不绑在会话 interface 上的家，让插件继续拿得到它，而票 10 删会话 interface 时不会顺手把
这个仍有真实消费者的面删掉。

**Blocked by:** 无（可以立即开始）。

**Status:** done（2026-09-24 落成 `host-connection` + `connection:read`，双挂不 bump；
宿主 1165/0、集成 8 target、权限锁 59/0、插件 287/0、eslint 0 error；前端唯一红属对侧 → 票 15）

## 为什么单独一张

2026-09-23 的裁决写得很明确：连接清单**不随会话原语退役**——它是宿主 server 的连接事实、
零产品语义，只是历史上住错了 interface。当前唯一消费者是插件的设备派生视图
（连接列表渲染要按会话聚合在线设备）。会话真源已迁插件之后，这份事实**仍然只有宿主知道**
（连接注册表在宿主侧），所以原语必须活下来、换个位置活。

## 验收标准

- [x] 该面从会话 interface 迁到中立归属（候选：事件/总线域的扩展，或新开一个极窄 interface），
      并给出**选了这个归属而不是那个**的理由（写进票末，一句话也行）
- [x] 返回字节与迁移前逐字一致：设备派生视图的回归用例不变即通过（若必须改断言，
      改动只能来自函数改名，不得来自字段）
- [x] 权限归属重述清楚：原挂在哪一位、迁后挂在哪一位、为什么（审计要能回答
      「谁能读到别人的连接清单」）；权限词汇五同步点一起落——SDK 词汇表 / 宿主能力清单 /
      宿主权限门 / 构建链映射表 / 前端合法集，漏任一处词汇漂移锁就红
- [x] 门禁：宿主 `cargo test --lib` + 集成全绿、插件 native 全绿、产物重建后 `[skip]` = 0
- [x] ADR 0022 里这条裁决从「待定」改成「落成 + 归属」，AGENTS 能力清单同步换名

## 边界与不做

- 不改连接注册表本身的语义（谁算在线、心跳怎么判）。
- 不碰会话 interface 的其余函数（10 统一收口）。
- 若归属选择会**新增** ABI bump：停手找用户裁决——本票默认要求函数级追加/搬迁，不 bump。

## Comments

### 2026-09-24 · 设计定稿（等并发 crypto 批次落 SDK 文件后按本清单施工）

**为什么现在是「定稿」而不是「做完」**：本票的改动面 9 个文件里 7 个正被同日并发的
`.scratch/2026-09-24-host-crypto-business-downsink/` 占用且未提交
（`packages/plugin-sdk-desktop/rust/{wit/bedcode.wit,src/{abi.rs,permission.rs,wasm_host.rs,host/mod.rs}}`、
`src-tauri/src/wasm_core/{manager/capability.rs,host_api/session.rs,manager/runtime/component.rs}`，
其中 SDK `permission.rs` 最后写入时间 02:21）。用户 2026-09-24 裁决：**等对侧提交，先把设计定稿**。

#### 1. 归属：新建极窄 interface `host-connection`（网络域第 5 组）

事实本体 = `WebSocketManager::global().list_clients()` → `WsSessionRegistry` 的原始条目
（`{clientId, deviceName?, fingerprint?, addr, authenticated, connectedAt}`），
**只有宿主机进程知道**（注册表在宿主），零会话语义。ADR 0022 v12 裁决 5 原文即「迁独立原语」，
本节把它落成。

四个候选的取舍（票面要的理由）：

| 候选 | 判 | 理由 |
| --- | --- | --- |
| 折叠进 `host-websocket` | ✗ | 那一面是「插件自己开 WS 客户端 / WS 服务端」的引擎原语（`ws:client` / `ws:server`），主语是插件的 socket；本事实的主语是**宿主 server 的在册连接**。合表会让「我能连出去」与「我能枚举谁连着宿主」两件事共用一个权限语义——审计问题（谁能读到别人的连接清单）当场失答。 |
| 折叠进 `host-events` / `host-bus` | ✗ | 那是**推**语义（emit / broadcast / publish-subscribe），本面是**拉**一次快照，形状不同；总线域还要背上 topic 命名空间裁决（v22 的 `<owner>::<base>` 迁移线），无谓扩面。 |
| 折叠进 `host-app` | ✗ | `host-app` 是宿主应用面（CLI 安装 / 资源目录），与「网络在册连接」不同域；且 `host-app` 已是宿主面 8 组里的杂项收容，继续塞会让裁剪线失去判据。 |
| **新建 `host-connection`** | ✓ | 与事实同域同名、单函数、可独立挂权限位；接口面 = 一份事实清单，符合裁剪线「宿主只暴露离宿主无法实现且无业务语义的原语」。 |

命名细节：interface `host-connection`（单数域名，与权限位 `connection:read` 同源）；
函数名 **`list`**（原 `connections-list` 在会话 interface 里需要前缀消歧，独立域内不需要），
返回 `result<string, string>` 且**继续返回同一份 JSON 数组文本**——
SDK 侧对插件暴露的方法名**保持 `WasmHost.connections_list()` 不变**（插件 `devices.rs` 零改），
故插件侧回归用例的断言**不需要动**（票面「改动只能来自函数改名」这条应达成「零改动」）。

- 兜底：若 wasmtime 48 在同一个 host struct 上因 `list` 与既有 trait 方法产生调用歧义，
  改名 `list-connections`——改名只发生在 WIT / SDK 绑定层，返回字节与插件方法名均不受影响。

#### 2. 权限：新增位 `connection:read`，且**新旧两条路径共用同一判据**

- 原挂位：`session:read`（`host_api/session.rs::session_connections_list` 的
  `check_permission(..., PERMISSION_SESSION_READ, "host_session_connections_list")`）。
- 迁后挂位：**`connection:read`**。审计口径随之单值化：
  「谁能枚举宿主 server 的在册连接（含 `fingerprint` 这类设备标识）= 声明 `connection:read` 的插件」。
- **旧 `host-session.connections-list` 一并改判 `connection:read`**（不保留 `session:read` 后门）。
  理由：留着第二把钥匙，直到票 10 删 interface 之前审计问题都是双答案，正是本票要消掉的东西。
  代价与先例：属「不 bump 的行为变更」（同 v22 的 `host-bus` topic 命名空间口径）——
  只声明 `session:read` 而未声明 `connection:read` 的**旧产物**调用旧函数会拿到
  `permission denied`（**fail-visible，不静默**）。生产消费方只有 terminal-session 一家，
  且它与本票同批重建产物，故无外部受损面。
  - 保守变体（若裁决要零行为差异）：旧函数保留 `session:read` 至票 10，
    同时在票面登记「双钥匙窗口期」——**默认不取**，因其把审计问题留给 10 之后。

#### 3. 不 bump 的落地形态：expand（双挂）→ 本票切消费者 → contract 随票 10

- 本票只做**函数级追加**：新增 `host-connection.list`，旧 `host-session.connections-list`
  保留为**同一实现的别名**（不复制实现体，别名直接调新落点），删除动作归票 10
  （它反正整 interface 退役并携带 bump）。
- 因此 `ABI_VERSION` 本票**不动**，WIT 注释里也**不写版本号**，只写
  「票 04 函数级追加，不 bump」——避免加深下面这条撞号：
  **对侧在途已把 `abi.rs::ABI_VERSION` 写成 26**，而 §3b-1 的裁决是本专项票 10 走 25→26、
  对侧走 26→27。撞号已从「票面文字待改记」升级为「代码事实」，**票 10 开工前必须由用户重裁版本号**。

#### 4. 施工清单（逐文件锚点，按依赖顺序）

1. **WIT**（`packages/plugin-sdk-desktop/rust/wit/bedcode.wit`）
   - 新 interface 块（放 `host-session` 之后、`host-database` 之前）：
     `interface host-connection { list: func() -> result<string, string>; }`，
     文档注释**整段搬运** `host-session.connections-list` 现有的「无排序 / 不过滤 / 不合并 /
     不加派生字段 + 元素字段清单」说明，并写明「票 04 自 host-session 迁入，不 bump」。
   - `world plugin` 追加 `import host-connection;`（现 22 个 import，见 :869-891）。
   - 旧 `connections-list` 注释加一行「**票 04 起为别名**，真源面是 `host-connection.list`，
     随票 10 的 interface 退役一并删除；权限判据已改 `connection:read`」。
2. **SDK 权限词汇**（`packages/plugin-sdk-desktop/rust/src/permission.rs`）
   - `pub const PERMISSION_CONNECTION_READ: &str = "connection:read";`（带 `///` 域说明）
     + `PERMISSION_VOCABULARY` 追加 `(stringify!(PERMISSION_CONNECTION_READ), PERMISSION_CONNECTION_READ)`。
   - 跑 SDK `pnpm run gen:permissions` 重出 `bin/permission-vocabulary.json` 与
     `src/plugin/permission-vocabulary.ts`（**禁止手抄**，AGENTS §7）。
3. **SDK 绑定**（`rust/src/host/mod.rs` 新 `pub mod connection;` + `host/connection.rs` 定义
   trait 方法 `connections_list()`；`src/wasm_host.rs` 把 `host_session::connections_list()`
   换成 `host_connection::list()`）——**方法名与返回类型 `Result<serde_json::Value, HostError>` 不变**。
4. **构建链映射表**（`packages/plugin-sdk-desktop/bin/manifest-gen.js`）
   `RUST_PERMISSION_RULES` 追加 `{ re: /\bconnections_list\b/, perm: 'connection:read' }`；
   该表在加载时校验「规则里的权限必须在词汇表内」，故必须在第 2 步之后跑。
5. **宿主能力清单**（`src-tauri/src/wasm_core/manager/capability.rs:58`
   `HOST_PRIMITIVE_CAPABILITIES`）追加 `"host-connection"`（22 组 → 23 组，网络域 5）。
6. **宿主实现与权限门**：新建 `src-tauri/src/wasm_core/host_api/connection.rs`，
   把 `session.rs:559-590` 的 `session_connections_list` 函数体**原样搬入**并改名
   `connection_list`，门换成 `PERMISSION_CONNECTION_READ` + 操作名 `host_connection_list`；
   `host_api/session.rs` 的旧函数改为一行转发别名（同门 `connection:read`，见 §2），
   `host_api.rs` 声明 `mod connection;`。**返回字节逐字不变**：`json!` 的六个键名与顺序、
   `block_on_async(manager.list_clients())` 的存储序、`serde_json::to_string` 全保留。
7. **WIT 绑定实现**（`manager/runtime/component.rs:411` 附近）新增
   `host_connection` trait 的 `fn list()` 转发到新落点；旧 `connections_list` 臂保留转发。
8. **前端权限文案**（`src/locales/zh-CN/desktop.ts` 与 `src/en/desktop.ts`
   的 `permission.*` 分组，见 zh-CN :186 形态）各加一条 `connection:read`
   ——**缺一条就复现今天 crypto 位那种红**（`permissionMeta.test.ts > 词汇表每一条权限都有文案`）。
9. **插件侧**：`plugins/terminal-session/plugin.json` 的 `permissions` 追加
   `connection:read`（`session:read` 仍保留，它有别的读面消费者）；`rust/src/devices.rs`
   **不改**（方法名不变），其回归用例 `parse_connections` 系列即票面要求的「返回字节不变」锁。
10. **词汇漂移锁**：`src-tauri/src/wasm_core/permission.rs`（对侧在途大改 302/319 的那份）
    的「每一条词汇都有真实门禁落点」断言会自然覆盖新位——第 6 步的门即其落点，
    **不要手抄清单**；若对侧改后锁的形态变了，按新形态接第 2/6 步，不反向改锁。

#### 5. 门禁计划（票面第 4 条）

- 宿主：`cargo check --lib --tests` → `cargo test --lib` → 集成 8 target（串行，占端口）。
- 插件 native：`cd bedcode-desktop/plugins/terminal-session/rust && cargo test`
  （**不与宿主测试并发**，见项目记忆：会撞出 pty / session 时序假红）。
- 产物重建：`pnpm run plugins:build` 三插件全绿 + 确认闭环用例日志 `[skip]` 计数为 0。
- 前端：`NODE_OPTIONS=--max-old-space-size=6144 pnpm exec vitest run --pool=forks`
  （本机默认堆必 OOM，见项目记忆）+ 根 `pnpm exec eslint .` 0 error。
- 形状/字节不变的直接证据：`devices.rs` 用例与 `session_e2e::` 的 connections-list 段
  **一行断言都不改**即通过。
- 文档：ADR 0022 的 v12 裁决 5 与 :130 表格行改「落成 + 归属 `host-connection`」，
  AGENTS §7 能力清单计数与域名同步换名（现写 21 组 / 网络 4 → 23 组 / 网络 5，
  注意对侧 crypto 也在动这段文字，施工时以当时 HEAD 为准）。

#### 6. 施工前必须重查的三件事（对侧落地后清单会变）

1. `ABI_VERSION` 当时是几（决定 §3 的注释措辞与票 10 的版本号重裁）。
2. `wasm_core/permission.rs` 词汇锁被对侧改成什么形态（第 10 步按其新形态接）。
3. `host_api/session.rs` 对侧改动的范围（第 6 步要搬的函数体若被他们顺手改过，
   以「函数体逐字搬运 + 只换权限位」为准，不并入他们的语义）。

### 2026-09-24 03:50 · 落地（对侧 02:35 提交后开工，全程未碰对侧未提交内容）

前置复查（按 §6）：对侧 `dcf5a5fd2` + `b7c30a013` 落了 crypto 契约面，**`ABI_VERSION` 已是 26**
（票 10 的版本号需按 §3b-1 重裁，已在票 10 面上登记）；本票**不动 ABI**（`abi.rs` 零改动）。
`wasm_core/permission.rs` 的词汇锁仍是「三副本相等 + 每条有门禁落点 + 生产 manifest 无死词汇」
形态，第 10 步按原样接（未手抄清单）。`host_api/session.rs` 的对侧残留只是 import 排序（2/2 行），
函数体未被他们动过 → 「逐字搬运 + 只换权限位」成立。

**与定稿的唯一偏差**：函数名不叫 `list`——**WIT 里 `list` 是关键字**（`host-session` 的
`list-sessions` 注释早就写了这点）。于是落 `host-connection.connections-list`，
**连改名都没发生**：插件调用点 `WasmHost.connections_list()` 与 `devices.rs` 派生视图回归用例
一行未动（只换 trait import：`HostSession` → `HostConnection`）。票面验收第 2 条
「断言改动只能来自函数改名」因此根本不触发。

**五同步点落位**（AGENTS §7）：① SDK 真源 `permission.rs` 新位 + 反射表条目；
②③ `pnpm run gen:permissions` 重出 CLI/前端两份生成物（现 34 条），并加锁
`connection_read_bit_is_in_generated_vocabulary`（漏跑生成器即红）；
④ `capability.rs` += `host-connection`（AGENTS 改记 22 组 / 网络 5，并注明对侧 crypto 并入时应 23）；
⑤ 新建 `wasm_core/host_api/connection.rs::connection_list` 挂 `connection:read` 门，
旧 `session_connections_list` 改**同判据别名转发**；⑥ `manifest-gen.js` 加
`connections_list → connection:read` 规则 + 前端 `contributionKinds.ts::PERMISSION_META` 条目
+ zh-CN/en 双语 title/desc。

**三条新锁**（`host_api/connection.rs::tests`）：`session_read_alone_no_longer_reads_connections`
（单钥匙：只授 `session:read` 时新面与旧别名一律 `permission denied`）、
`alias_returns_identical_bytes`（别名与新面返回逐字相同）、生成物词汇锁。
连同随实现迁走的权限门 + 空形状锁、以及插件侧 `devices.rs` 回归用例零改动通过
= 票面第 2 条的完整证据链。

**顺带挖出一处陈年不自洽（非本票引入）**：`manifest-gen.js:410` 对权限数组做
`[...permissions].sort()` 并**回写源清单**。HEAD 里 `pty:spawn` 排在 `pty:io` 前是未排序的
陈旧形态，任何一次 `plugins:build` 都会归一，而三个 pin 早前就对不上生成物——宿主
`session_e2e` 的 manifest pin、插件 `lib.rs::declares_only_landed_domain_surface`、
插件前端 `plugin-contract.test.ts`。本票重建产物后三处同时翻红，处理是**跟随生成物排序**
并在每处留注释（顺序来自生成器，不是手改）。建议单开一票定「pin 读生成物 vs 字面顺序」的口径。

**门禁实跑（03:0x–03:5x）**：`cargo check --lib --tests` 0 error；
`cargo test --lib -- --test-threads=1` **1165 passed / 0 failed**；集成 8 target 全 ok；
`cargo test --lib permission` **59/0**（三副本相等 + 门禁落点 + 无死词汇）；
terminal-session 产物 02:59 重建（含 `host-connection` 导入）、`[skip]` = **0**；
插件 native **287/0**；桌面前端 81 files / 794 tests = **793 passed / 1 failed**，
唯一红是 `crypto:aead|asym|kdf` 缺 zh-CN 文案，**HEAD 级、属对侧**
（已立 [`issues/15-crypto-permission-copy-missing.md`](15-crypto-permission-copy-missing.md)），
本票的 `connection:read` 文案齐全；根 `pnpm exec eslint .` **0 error**。

**行尾自查（这次真踩了一次）**：脚本以文本模式改 `plugin-sdk-desktop/rust/src/permission.rs`、
`src/plugin/contributionKinds.ts`、`src/locales/{zh-CN,en}/desktop.ts`，把 4 个 HEAD 100% CRLF
的文件整片翻成 LF（479/404/289/303 行 CR 归零）。已按二进制 `b'\n'→b'\r\n'` 还原，
diff 缩回纯增行（8/0、6/0、1/0、1/0）且 `raw == --ignore-cr-at-eol`；还原后重跑
`--lib permission` 59/0 与 `--lib` 全量确认无损。新文件（两处 `connection.rs`）LF，
与同目录 `host_api/session.rs` / `host/session.rs` 一致。

**格式自查**：只对两个新文件跑 rustfmt（clean）；其余一律 `--check` 比对，确认我的行不引入
新偏离——`wasm_host.rs` 改前改后同为 35 处（对侧 crypto 的 import 块本就未排序，我并入同一
hunk 但不重排他人行）、插件 `devices.rs` 9→9、插件 `lib.rs` 与新文件 0。

**边界遵守**：不改连接注册表语义（`list_clients` 调用面一字未动）；不碰 `host-session`
其余函数（归票 10）；不新增 ABI bump。

**遗留**：① 旧别名随票 10 删除；② AGENTS 能力清单计数在 `host-crypto` 并入时改 23；
③ 「pin 读生成物 vs 字面顺序」的口径建议单开一票定案。
