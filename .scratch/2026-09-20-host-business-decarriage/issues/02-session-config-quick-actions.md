# 02: session 插件配置与快捷指令域下沉

**What to build:** 会话配置 HTTP 查询面（`/api/configs`）真源从宿主主库切到 session 插件（与桌面命令面同一真源，消除双轨）；快捷指令（`/api/quick-actions` 查询 + 桌面快捷指令命令面 + WS 变更同步广播）整体迁入 session 插件第 4 域（私有库持久化）。宿主主库会话配置/快捷指令业务表与对应宿主实现按 expand-contract 退役：先双写对照与迁移幂等验收，插件面无缺口后删除宿主表契约。

**Blocked by:** 01（网关地基与形状契约锁）

**Status:** done（2026-09-21；插件 native 199 测试 + 前端契约 21 项全绿；宿主 cargo test 1090/0，含新闭环 `test_business_endpoints_dual_track_closed_loop` 双轨逐字节对照 + handoff 幂等；移动端 vitest 466 全绿；前端测试零改）

- [x] `/api/configs` 与 `/api/quick-actions` 响应形状与今天逐字节一致（双轨对照测试）
- [~] 桌面快捷指令命令面形状不变，真源在插件（插件未激活降级语义按 spec 决策 3 返回明确错误，非假数据）——**现状不存在**（见下方裁定，跳过）
- [~] WS 快捷指令变更同步广播形状不变，由插件经广播原语触发；宿主同步处理器中对应业务形状段落退役——**现状不存在**（见下方裁定，跳过）
- [x] 迁移幂等：旧库重跑、历史快捷指令/会话配置数据零丢失
- [x] 宿主主库会话配置/快捷指令业务表契约退役（插件面无缺口验收之后）——**quick_actions 表全量退役**（schema 不再建表 + operations/models/控制器删除 + 网关 PluginRequired）；**session_configs 表保留为引擎输入投影**（/api/sessions/start、桥接投影、host-session 配置面仍读它，见 Comments ②）
- [x] 移动端回归通过；前端测试零改

## Comments

### ① 迁移通道裁定（spec 决策 6 的必然延伸）

快捷指令没有像会话配置那样的 legacy 读取面（host-session 配置面只覆盖配置表），且决策 6 禁止为此开新 host 原语/ABI bump。故迁移由**宿主侧 handoff**（`plugin/quick_actions_migration.rs`）完成：读 legacy 主库 `quick_actions` 行 → 经插件互调 api `com.bedcode.session.quick-actions-import`（JSON-RPC over 既有 host-bus，插件侧幂等 marker）推入插件私有库。触发 = boot 装配后（与 `task_data_migration` 同位置）；插件未激活时跳过（数据留主库，双轨期继续服务）。已知边界：插件运行中途被启用时 handoff 不感知（数据零丢失，插件面下次启动补齐），已记录在模块文档。

### ② session_configs 表保留为引擎输入投影（部分退役）

`/api/configs` 宿主读取面已退役（网关 PluginRequired + 控制器删除），但 `session_configs` 表**不能**随之删除：`POST /api/sessions/start`（移动端活跃路径，spec 决策 2 明确不进网关收编）、桥接投影（`session_config_bridge`）、host-session 配置面（迁移读 legacy）都直接读它。该表按票 08 设计继续作为「引擎输入投影」（单写者 = 宿主桥接），其彻底退役绑定 `/api/sessions/start` 的后续桥接，超出本票范围。

### ③ 双轨对照测试

新闭环 `test_business_endpoints_dual_track_closed_loop`（真实 wasip3 产物 + 真实宿主原语）：legacy 主库播种 → 激活插件（配置迁移 + 快捷指令建表）→ handoff 推送 → `_http_endpoint` 五端点输出与宿主旧 DTO 形状逐字节比对（含 `icon/color` 显式 null、`wslDistro` 显式 null、`sort_order` 升序、Cache-Control 头）+ 重推幂等（`alreadyMigrated`）。

### ④ 前端零改确认

桌面/移动端前端零改动（移动端 vitest 466 全绿）；`useQuickActionStore` 等死壳未动（无行为面）。
## Comments

### 前提核对（票 01 实施时发现，需范围裁定）

票面两格假设「桌面快捷指令命令面」与「WS 快捷指令变更同步广播」有现状形状需要保持，实装核对结论是**两者都不存在**：

| 票面表述 | 现状 |
| --- | --- |
| 「桌面快捷指令命令面形状不变」 | `src-tauri` 里没有任何 `*_quick_action*` Tauri 命令，也不在 `lib.rs::invoke_handler` 清单里；前端 `deviceCommands.ts` 的 4 个 wrapper（`list_quick_actions` / `create_quick_action` / `update_quick_action` / `delete_quick_action`）指向**未注册命令**，调用即 reject；`stores/quickAction.ts` 只做 pendingInput 中转，与 `quick_actions` 表无关 |
| 「WS 快捷指令变更同步广播形状不变」 | `SyncPayload` / `DesktopSyncEvent` 无快捷指令变体；桌面 WS 侧没有 `session_config` 帧分派，`services/session_config.rs::list_quick_actions_response` 与 `SessionConfigAction::ListQuickActions` 都是无调用点的线协议残留（两端有 serde 形状测试） |
| 「全链路活跃」 | 活的出口只有 `GET /api/quick-actions`；两端均无快捷指令 UI，移动端 `httpListQuickActions` 亦无调用方；表在现网只可能被历史版本写入 |

补一个真正的变更广播需要给 `bedcode_plugin_api::events::SyncEvent` 加变体（= SDK/WIT 追加），与 spec 决策 6
「不新增 host 原语、不触发 ABI bump」冲突，属扩大范围。故本票按「下沉 + 消除双轨」推进，
命令面/广播两格待用户裁定（详见会话记录）。
