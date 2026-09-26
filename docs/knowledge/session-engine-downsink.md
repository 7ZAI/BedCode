# 会话引擎整体下沉（Session Engine Downsink）—— 决策与终态记录

> 整理自 2026-09-23 会话引擎下沉专项 spec（2026-09-27 迁入 docs）。
> 本文件只保留**决策、终态与验收红线**；实施过程的票级细节（issues/01…12）留在原 scratch。
> 边界裁决见 `docs/adr/0022-plugin-host-interface-primitive-boundary.md`；阶段归属见
> `docs/knowledge/plugin-kernel-roadmap.md` 阶段 3。

Status: **done（2026-09-24，宿主零会话对象达成；移动端受损清单 M6–M9 见路线图）**
Date: 2026-09-23
范围: 桌面端为主（宿主 `session/`、`pty/`、`wasm_core/host_api/{session,terminal,lifecycle}.rs`、
`server/`、`commands.rs`、`events/`、`utils/session_*_bridge.rs`、WIT/SDK）；
移动端按用户 2026-09-23 明确指令「不用管」→ **不设双端同步豁免的收窄承诺，改出「移动端受损清单」如实记账**

---

## 0. 用户方向指令（原文要点，作为验收基线）

1. **PTY 留宿主但必须解耦**：pty 只提供基础服务接口（或满足业务的基础接口），不得与业务代码耦合。
2. **会话状态机 + 登记 + 生命周期分发应放插件端**；其他服务调用插件端接口或转发给插件；**同步阻塞的留在宿主**。
3. **输出环**：「性能红线」应经测试后判断，性能可以就迁插件。**输入通道**也应由插件转发给宿主 pty。
4. **移动端不用管**；总原则 —— 向微内核靠拢，插件实现大部分业务逻辑，只有 io / 文件 / 线程等 WASM 无法实现的才留宿主。

---

## 1. 终态划界（微内核）

```
宿主（内核，只留引擎 + WASM 做不到的）
├── PTY 引擎：Raw exec + PtyRing + 终态门 + 回收        ← 点 1，唯一 PTY 出口
├── host-pty 原语（6 函数，v16）                        ← 宿主唯一的会话相关原语
├── 网络引擎：HTTP/WS server 骨架 + 认证/过滤链 + 连接注册表 + mDNS
├── 存储/安全/通信/wasmtime 运行时
└── 同步阻塞点（spawn 边界、系统关停）                  ← 点 2 明确保留

插件 com.bedcode.terminal-session（业务真源）
├── 会话登记 + 状态机 + 生命周期分发（新 session_registry 域）
├── 会话真源 → 插件私有库（新 sessions 表）
├── 输出消费环 + 自持游标（经 host-pty.ring-fetch）
├── 输入转发（host-pty.write）+ 提交行重建（自 input_line.rs 迁入）
├── 尺寸裁决（已在插件 actions.rs）
└── 会话编排（已在插件 launch.rs）
```

**宿主「零会话对象」是本专项的验收红线**：`src-tauri/src/session/` 目录整体不存在（2026-09-24 已达成）。

---

## 2. 硬约束（不解决就无法落地，必须先裁决/改造）

| # | 约束 | 证据 | 处置 |
| --- | --- | --- | --- |
| H1 | **`PLUGIN_PTY_MAX_SESSIONS_PER_PLUGIN = 8`** | 宿主常量 | **已解（P1-b 前置 B）**：manifest 声明 `ptyQuota` + 宿主加载期区间仲裁（上限常量 64，越界拒绝不夹取），`spawn` 判据按属主声明；terminal-session 的实际声明值随 P1-b 接入 `host-pty.spawn` 同批写入 |
| H2 | **同实例串行红线** | 插件开发检查清单「同实例串行红线」 | 同插件实例同一时刻只允许一个 guest 调用；桌面终端窗口 + 移动端 + WS 通道同时拉输出会串行化。需量化预算（每帧成本 × 端数），不能靠假设 |
| H3 | **WASM 不可重入、无 push 模型** | ADR 0022 D3（两条理由） | 输出只能「拉」不能「推」；**已由开放点定案规避**——环本体留宿主 PTY 引擎（形态 B），不入插件内存 |
| H4 | **server 留宿主** | 点 4（网络引擎属 WASM 做不到） | 移动端所有会话面由宿主 server 收 → 会话**事实**面转发给插件；输出**字节**面按形态 B 由宿主直读 PtyRing，不跨边界 |
| H5 | **同步阻塞留宿主** | 点 2 明确；`dispatch_lifecycle_event(Creating)` 现在同步阻塞 | Creating「hook 就位后才 spawn」的时序在插件化后由**插件自己串行保证**（它自己先做 hook 再做 spawn），宿主只保留 spawn 原语的同步边界 |
| H6 | **会话 id 与 PTY 句柄是两个标识** | `host-pty.spawn` 返回 `pty-<uuid>`；现会话 id 由宿主 `create-with-spec` 预生成 | 迁移期需定义映射；`BEDCODE_SESSION_ID` 注入随迁形态要定 |

---

## 3. 分票与阶段（实施记录概览）

> 本节是「阶段与裁决记录」，不是工单。P1-b 之后的实施工单已按纵向切片拆成
> `issues/01…12`（权威拓扑与依赖边见原 spec §3b 票据拓扑），全部完成。

- **P0 — PTY 与业务解耦（✅ 2026-09-23）**：删 `pty/command.rs`、`pty/wsl.rs`；`PtyCommandSource`
  收敛为 Raw（删 `Business(SessionLaunchConfig)` 变体）；`create-with-spec` 只走 Raw 路径。
  验收：`pty/` 不含任何 shell 包装 / WSL 转换 / 产品环境分支；`PtyCommandSource::Business` 不存在。
- **P1 — 会话真源下沉（✅ 2026-09-24，票 06–11）**：会话登记 + 状态机 + 生命周期分发 +
  输出消费环迁入插件 `com.bedcode.terminal-session`（`session_registry` 域 + 私有库 `sessions` 表）；
  宿主 `src-tauri/src/session/` 整目录删除（票 11）；`host-session` / `host-terminal` 两 interface
  退役（ABI v27）；`utils/session_gateway.rs` 成为会话事实唯一读出口（全接口 `serde_json::Value`
  原样透传插件 reply，零解析零解释，插件未激活显性报错）。
- **P2 — 移动端受损清单如实记账**：M6–M9 挂进路线图（见 `plugin-kernel-roadmap.md`「移动端受影响清单」）。

---

## 4. fail-visible 三形态（本专项建立的通用判据，落锁清单）

**真源换了地方就要 fail-visible**（通用判据）：事实真源迁走后，**旧读路径必须显性失败，
禁止静默降级成「无数据」**——静默降级会让「线还在、数据永远是空」的断链在测试全绿的情况下
长期存活。三种具体形态，缺一不可：

| 形态 | 要求 | 落锁位置 |
| --- | --- | --- |
| ① **宿主侧回查** | 旧读路径要么删掉、要么对真源外的对象显性报错，不得返回空 / `NotFound` 让调用方当「无数据」吞掉 | 防回接锁：`retired_kernel_session_domain_is_not_reintroduced`（`wasm_core/manager/host/tests/wasm_flow_test.rs`）、`retired_session_command_surface_is_not_reintroduced`（`wasm_core/manager/host/api_bridge.rs`）、`retired_session_observation_surface_is_not_reintroduced`（`wasm_core/manager/host/tests/wasm_flow_test.rs`） |
| ② **旧 ABI 产物** | 破坏性契约变更后旧产物要在**实例化期**拿到点名缺失 interface + 「按哪个版本重建」的错误，不是 trap 也不是静默降级 | `LoadedWasmPlugin::stale_artifact_rebuild_hint`（`wasm_core/manager/runtime/component.rs`） |
| ③ **退役的权限位 / 命令字眼** | 构建链映射表含词汇表外条目时**加载即抛**，而不是注入一个永远过不了门的权限 | `packages/plugin-sdk-desktop/bin/manifest-gen.js` 加载期词汇自检 |

**先例（本判据的来源）**：2026-09-24 会话下沉专项——宿主「回查内核拿会话」曾造成桌面终端按键丢失、
任务队列被批量标中断而测试全绿；三形态各已落锁，见上表。

---

## 5. 验证（完成定义）

- 宿主 `src-tauri/src/session/` 目录不存在（防回接锁随回归运行，越线回接直接测红）
- `host-session` / `host-terminal` 不在 WIT 与 SDK 中（`retired_session_*` 锁覆盖）
- 桌面端 `cargo test` 全量绿（含 wasm_flow / pty_session_chain 集成）；插件契约测试同步更新
- `utils/session_gateway.rs`：插件未激活时显性报错，不返回空数据

---

## 6. 相关文档

- `docs/adr/0022-plugin-host-interface-primitive-boundary.md` —— 会话语义下沉批次（v8）、
  host-pty（v6）、终端（v9）、性能红线 v23 的边界裁决
- `docs/knowledge/plugin-kernel-roadmap.md` —— 阶段 3 归属、移动端受损清单 M1–M9
- `docs/knowledge/plugin-development-checklist.md` —— 插件开发检查清单（同实例串行红线等）
- 原实施 spec 的票级记录保留在专项目录（issues/01…12），不随本文件迁移
