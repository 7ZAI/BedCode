# 10: 能力转发链路缺调用方身份 —— 运行期替换机制当前对一切按插件分区的能力都不可用

**What to build:** 让 `CapabilityTarget` 的转发调用**携带调用方 `plugin_id`**，并在系统组件
（ADR 0032 的 L1 基础服务）侧按调用方身份隔离真源（存储命名空间 / 句柄属主 / 私有库）。

**Found by:** wasm-core-lib-split 票 09 第 3 项（运行期能力路由表扩表）——扩表前要求
「确认既有缺陷是否修复；未修则单独立票」，实测**未修**，且**缺陷范围比票 09 假设的更宽**。

**Status:** ready-for-agent

- [ ] WIT 侧形状定案并在 spec / ADR 记理由（三选一，见「方案」；**本票不得顺手改 WIT**）
- [ ] `CapabilityTarget` 全部方法补调用方身份（或等价机制），`GuestOp` 与两条 dispatch 路径同步
- [ ] `manager::capability` 的 `forward_*` 不再把 `caller_plugin_id` 只用于自调用判定
- [ ] `plugin_binding::storage` / `DiscoveryPorts` 侧的转发端口签名同步（`plugin_id` 不再只是判自调用）
- [ ] 复现用例先行：两个不同调用方经同一提供者读写同名 key **必须互不可见**（今天会互相看见）
- [ ] 句柄类能力的属主判定同样按调用方（browse / advertise 句柄归属调用方而非提供者）
- [ ] 提供者侧停用回收：`purge_for_plugin` 能回收**调用方**留在提供者里的句柄
- [ ] ABI 影响评估落地（若 WIT 变更：desktop ABI bump + 双端 WIT 副本同步 + 移动端影响评估）
- [ ] CHANGELOG 双语条目；ADR 引用本票

## 缺陷取证（2026-10-05 实测，非推断）

### 现象 A：storage 命名空间丢失（票 09 已知的那条）

`plugin_storage` 表按 `plugin_id` 分区（`wasm_core/storage.rs:57`
`SELECT … WHERE plugin_id = ?1 AND key = ?2`）。两条路径对同一调用的处理不同：

| 路径 | 谁解析 `plugin_id` | 结果 |
| --- | --- | --- |
| 宿主原语 | `PluginStorage::get(plugin_id, key)`，用**调用方** id | `A/k` |
| 路由转发 | `CapabilityTarget::storage_get(key)` → `GuestOp::CapStorageGet { key }` → 提供者实例的 `host-storage.get(key)` | 提供者用**自己**的 id ⇒ `S/k` |

`caller_plugin_id` 在 `manager/capability.rs` 里**只**用于自调用判定
（`system_component_instance(name, caller_plugin_id)` 里 `plugin_id != caller_plugin_id`），
转发时一个字节都没带给提供者。

**后果**：系统组件代持 `host-storage` 后，①调用方读不到自己写在宿主原语侧的值（前后
不一致）；②两个不同调用方经同一提供者读写同名 key 时落在**同一个**分区——A 能读到 B 的
值。这正是 `security/frontend_channel.rs` 文档头描述的那类越权（该文件已为
`invoke('plugin_storage_get', { pluginId: '受害者', key })` 做了通道绑定，唯独转发路径没有）。

### 现象 B：不只是 storage —— 每一个能力都按 `plugin_id` 分区

| 能力 | 分区依据 | 转发后落到谁身上 |
| --- | --- | --- |
| `host-storage` | `plugin_storage.plugin_id` | 提供者自己（现象 A） |
| `host-database` / `host-plugin-database` | 私有库目录 `app_data_dir()/plugins/<id>` | 提供者的私有库 |
| `host-mdns` | 句柄表 `BROWSERS/ADVERTISERS.owner`；事件 topic `mdns:found.<plugin-id>` | 提供者自己——**且事件投递到提供者的 topic，调用方根本收不到** |
| `host-peer` / `host-websocket` / `host-http` | 传输/端点/传输任务表的属主列 | 提供者自己 |

⇒ **运行期替换机制当前对全部能力域都只是「机制面就绪」，没有任何一个是可用的**。
票 09 因此只把 `host-mdns` 加进路由表而**不加 WIT `plugin-system` 导出**：没有组件能
导出那五个函数，路由入口在构造上不可达（有判别力的用例见
`manager::capability::tests::routable_capabilities_and_forward_methods_stay_in_sync`
与 discovery 域的路由用例）。

### 现象 C：停用回收漏人

`purge_for_plugin(plugin_id)` 按属主回收句柄 / 连接 / 端点。转发路径下，调用方的句柄
落在**提供者**的表里、属主记的是提供者 ⇒ 调用方停用时它的句柄**不被回收**（泄漏，
且其 re-announce 任务继续跑）。

## 方案（三选一，须先定案再动手）

| # | 方案 | 代价 / 风险 |
| --- | --- | --- |
| **A** | WIT 层显式带调用方身份：能力接口（如 `host-storage.get(key, caller)`）加一个身份参数，或新增一组「代持身份」接口 | **ABI 变更**（desktop bump + 双端 WIT 副本同步 + 移动端影响评估）；提供者可在接口层拒收非法身份（fail-closed） |
| **B** | 不改 WIT，改为**每能力一个 provider 池**：注册表按 `(capability, caller)` 而非 `capability` 寻址，每调用方一个提供者实例 | 不动契约；但提供者实例数 = 调用方数（L1 基础服务会按调用方各起一份，资源与语义都要重评）；句柄归属天然正确 |
| **C** | 只修 storage 一族（键名空间在**键**里携带调用方前缀，如 `A:k`），其余能力维持「不可路由」 | 最小；但把隔离责任放进键命名（插件可自带前缀绕过或碰撞），与「隔离由宿主裁定」的既有口径相反 ⇒ **不推荐** |

判断依据须写进 ADR：① §5.1.3「安全闸门不可插拔」；② 现有 `frontend_channel` 的
「身份由通道绑定、参数自报无效」口径；③ 双端 WIT 同步成本（ADR 0019 / 0022 偏离条款）。

## 与已完成部分的边界（不要重复做）

- 票 09 已完成：路由表扩到 `host-mdns` + 五条转发方法 + 能力域侧转发端口 + **闭表锁**
  （表 ↔ `CapabilityTarget` ↔ 能力域端口三层的逐项对应）。本票改签名时，闭表锁会自动
  把改动面钉住（方法数与导出数必须仍一一对应）。
- 票 09 已完成：`DiscoveryPorts::forward_mdns_*` / `SqlitePorts::forward_storage_*`
  的 `Option` 契约（`None` = 无提供者，不是错误）与「权限门先于转发」的顺序语义。
- **不要**在本票里改 `world plugin-system` 的导出清单：那是「某能力被允许由组件提供」
  的开关，应在本票定案后单独提。

## Comments