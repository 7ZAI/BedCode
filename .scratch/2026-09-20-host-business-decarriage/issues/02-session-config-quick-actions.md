# 02: session 插件配置与快捷指令域下沉

**What to build:** 会话配置 HTTP 查询面（`/api/configs`）真源从宿主主库切到 session 插件（与桌面命令面同一真源，消除双轨）；快捷指令（`/api/quick-actions` 查询 + 桌面快捷指令命令面 + WS 变更同步广播）整体迁入 session 插件第 4 域（私有库持久化）。宿主主库会话配置/快捷指令业务表与对应宿主实现按 expand-contract 退役：先双写对照与迁移幂等验收，插件面无缺口后删除宿主表契约。

**Blocked by:** 01（网关地基与形状契约锁）

**Status:** ready-for-agent

- [ ] `/api/configs` 与 `/api/quick-actions` 响应形状与今天逐字节一致（双轨对照测试）
- [ ] 桌面快捷指令命令面形状不变，真源在插件（插件未激活降级语义按 spec 决策 3 返回明确错误，非假数据）
- [ ] WS 快捷指令变更同步广播形状不变，由插件经广播原语触发；宿主同步处理器中对应业务形状段落退役
- [ ] 迁移幂等：旧库重跑、历史快捷指令/会话配置数据零丢失
- [ ] 宿主主库会话配置/快捷指令业务表契约退役（插件面无缺口验收之后）
- [ ] 移动端回归通过；前端测试零改
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
