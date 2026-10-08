# 票 02 · 移动端 ABI 规划表（D2 落档素材）

> 状态：**核验完成（2026-10-07 实测）**。数据取当前工作区；与 spec §1.2 的
> 差异已在表内标注。破坏性批次 = ABI bump 随对应业务下沉票（阶段 2/3）同批执行，
> 每批带 fail-visible 三形态。

## 1. 基线（实测）

| 项 | 值 |
| --- | --- |
| 移动 ABI_VERSION | **11**（`plugin-sdk-mobile/rust/src/abi.rs`） |
| 桌面 ABI_VERSION | **34**（`plugin-sdk-desktop/rust/src/abi.rs`；spec §1.2 写 31 已过时） |
| 移动 WIT import | 11 组 host-*（storage/database/terminal/events/http/fs/config/log/bus/peer/mdns/platform） |
| 移动 WIT export | 8 组（command/lifecycle/events/events-binary/terminal-hooks/manifest/abi/…） |

## 2. 接口差异逐项（对齐/新增/退役三列）

| 接口 | 现状（移动） | 动作 | 批次 | ABI | 破坏性 | 对照桌面 |
| --- | --- | --- | --- | --- | --- | --- |
| `host-peer` | 15 函数 | **对齐 19**：删 `resume-all-transfers`；增 `set-download-dir` / `start-node` / `stop-node` / `active-transfers` / `collect-outgoing` | 票 04/06/08（传输下沉） | 12 | **删 = 破坏性**（随票 06 迁移后再删）；增 = 纯增量可先加 | v34 同形 |
| `host-terminal` | 1 函数（send） | **整 interface 退役** | 票 15（终端下沉） | 13 | 破坏性 | v27 已删，对齐 |
| `terminal-hooks` | 2 导出 | **整 interface 退役** | 票 15 | 13 | 破坏性 | v27 已删，对齐 |
| `host-websocket` | **无** | **新增客户端域**（connect/send-text/send-binary/close/is-connected + 属主私有 topic `<owner>::ws:open\|error\|close`）+ 可选导出 `events-ws`（宿主动态探测） | 票 11（终端/事件通道下沉前置） | 12 | 纯增量 | 桌面 v34 客户端 5 函数，对齐移植 |
| `host-database` | 2 函数（execute/query 裸 SQL） | **依 D4**：选「统一 13 原语」→ 对齐桌面 database/plugin-database/storage 语义（权限门/表名前缀纵深/护栏/属主分区）；选「薄库」→ 保持 2 函数仅共享执行器 | 票 05（依 D1/D4 定案） | 12 或不变 | 若统一则增函数为增量、语义收紧为破坏性（表名前缀） | v34 13 原语 |
| `host-storage` | 3 函数 | 对齐（同形，无动作或微调） | — | 不变 | 否 | 同形 |
| `host-mdns` | 5 函数 | 语义对齐：属主定向事件 `<owner>::mdns:found\|lost`（payload 增量 serviceType/browserId） | 票 03（mDNS 单守护） | 12 | 事件 payload 增量（新字段忽略原则，非破坏性） | 同形（v2 基础能力服务形态） |
| `host-bus` | 5 函数（**已有 publish-binary/subscribe-binary**） | 无动作（帧级二进制通道已就位，终端下沉直接复用） | — | 不变 | 否 | 同形 |
| `host-events` | 2 函数（emit/notify） | 无动作 | — | 不变 | 否 | 同形 |
| `host-http` / `host-fs` / `host-config` / `host-log` / `host-platform` | 各有 | 无动作（移动域自持） | — | 不变 | 否 | 移动独有或同形 |
| `host-pty` / `host-auth` / `host-crypto` / `host-connection` / `host-process` / `host-app` / `host-task` | 无 | **不跟演**（ADR 0018 移动端不需要主机侧引擎） | — | — | — | 桌面独有 |

## 3. ABI 演进路径

```
v11 ──(票 04/06/08 host-peer 对齐 19：先增 5 后删 1)──► v12 ──(票 11 host-websocket 新增)──► v12
                                                          │
                                                          └──(票 03 host-mdns 事件语义对齐，payload 增量，可并批)──► v12
v12 ──(票 15 host-terminal + terminal-hooks 退役)──► v13（破坏性，随终端下沉同批）
```

- v11 → v12：**一次收口**（host-peer 增 5 + 删 1、host-websocket 新增、host-mdns
  事件语义对齐全部并入 v12 批，随阶段 2/3 前段落地）；`resume-all-transfers` 的
  迁移（票 06）与删除同批——先让 file-transfer 插件放弃调用，再删接口。
- v12 → v13：host-terminal/terminal-hooks 退役，旧产物实例化期点名重建。

## 4. 权限位增删表（对照桌面五同步点）

| 五同步点 | host-peer 对齐 | host-websocket 新增 | host-terminal 退役 |
| --- | --- | --- | --- |
| ① SDK 常量 | `resume_all_transfers` 移除；新增 5 函数权限常量 | 新增 5 函数权限常量 + events-ws 探测 | `terminal_send` 移除 |
| ② 打包 CLI 合法集合 | wasm-apps manifest 校验集合同步 | 同步 | 同步 |
| ③ 前端合法集合 | mobile 前端 capability 集合 | 同步 | 同步 |
| ④ 宿主能力清单 | 宿主 permission 门同步 | 同步 | 同步 |
| ⑤ 权限门 | host_impl/peer.rs 门面同步 | host_impl/ws.rs 新门面 | 门面删除 |

## 5. 破坏性变更配套（fail-visible 三形态）

| 形态 | host-peer 删 resume-all-transfers | host-terminal/terminal-hooks 退役 |
| --- | --- | --- |
| ① 旧读路径删除或显性报错 | file-transfer 插件迁移后 `resume_all_transfers` 调用点删除（编译断链） | 前端命令面注销 + 插件命令面接管；`terminal_*` 宿主命令删除 |
| ② 旧产物实例化期点名 | `stale_artifact_rebuild_hint` 判据扩展：v12 产物携带 `requires ABI >= 12` | v13 产物点名缺失 `terminal` / `terminal-hooks` interface 并重建 |
| ③ 退役词汇加载即抛 | `resume-all-transfers` 词汇从权限表移除，旧 manifest 加载即抛 | `terminal.send` 词汇同理 |

## 6. 受影响插件调用点（阶段 2/3 迁移放宽清单）

| 插件 | 调用点 | 批次 |
| --- | --- | --- |
| file-transfer | `resume_all_transfers` / `send_files` / `respond_transfer` / `set_receive_policy` / `pause/resume` / `set_shared_roots` 等（对齐桌面 v30/v31 调用形状） | 票 06/07/08 |
| com.bedcode.session（新建） | host-websocket 客户端域（connect/send-text/close）+ events-ws | 票 11/12/13 |
| auto-task / ai-chatbox | host-database 若统一 13 原语 → 调用点同批迁（表名前缀） | 票 05 |

## 7. 门禁（本票）

- ABI 规划表核验：本表 §2/§3 全部为 2026-10-07 工作区实测（WIT 逐接口、ABI 常量、
  移动插件调用点经 rg 抽查）。
- 双端 WIT 对照锁草案：SDK `mobile_parallel_copy_shape_lock` 扩展为全接口逐函数对照锁
  （桌面 v34 vs 移动 v13 终态），属票 18 范畴，本票只落草案：
  `host-peer`/`host-storage`/`host-bus`/`host-events`/`host-mdns` 逐函数签名对照；
  移动独有（terminal/websocket 客户端域）与桌面独有（pty/auth/crypto/…）双向豁免。
