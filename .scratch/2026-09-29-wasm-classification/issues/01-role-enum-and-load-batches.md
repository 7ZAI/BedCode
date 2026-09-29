# 01: 角色枚举扩展（L1/L2/L3）+ 加载顺序分两批

**What to build:** `PluginKind` 从两值扩为多值（`application` / `basic-service` / `internal-business` / `business-app`），并把启动序列步骤 4 扩为「按角色两批」。spec §2/§3/§4/张力 3。

**Blocked by:** 张力 4 需先裁定（4a/4b 决定 L2 由谁承担，但**枚举本身可先行**）

**Status:** done（2026-09-29）

- [x] SDK `types.rs::PluginKind` 扩值；**缺省仍 L3**（向后兼容，旧产物零迁移）
      ——落地为**三值 + 旧拼写反序列化别名**（`system`→L1、`application`→L3），
      不新增与 L3 并存的第四个 `application` 值（两个拼写指同一层 = 第二处漂移源）
- [x] manifest `type` 字段取值域校验（`manifest-validate.js`，非法值构建期拒；旧拼写只告警不拦）
      + 宿主加载期反序列化收口（`validation.rs` 用例锁取值域）
- [x] `boot.rs` `activate_system_components()` → `activate_role_driven_components()`：
      **按角色两批**（L1 → L2，层序真源 = SDK 常量 `PluginKind::ROLE_DRIVEN_LOAD_ORDER`），
      批内按 id 排序；`auto_activate_from_persisted_state()` 是 L3 批，现**只收 L3**
- [x] 保留「单个失败不阻断其余」语义（L2 失败 → L3 照常激活、认证面 fail-closed，
      失败组件落 Error 态 + `error` 日志点名角色）
- [x] 角色驱动层（L1/L2）启停不持久化（`get_activated_state` 跳过）
- [x] 单测：**L1 先于 L2 先于 L3 的顺序断言**（用 `activated_at` 激活事件时刻，不用时序 sleep）
      + **L2 激活失败不阻断 L3**（`system_component_test.rs`，boot 全路径集成用例）
- [x] 宿主**只按谓词**判角色（`is_role_driven` / `provides_host_capabilities` / `is_business_app`），
      不出现 `PluginKind::X` 字面量（锁在 `l2_gating_test.rs`）
- [x] 同步 `plugin-development-checklist.md` 分类条目 + 桌面端 code-map（capability 段）
      + ADR 0032 转「已实施（机制层）」+ ADR 0022 v33 偏离条款

**本票不动**：`ROUTABLE_CAPABILITIES` 注册表化（张力 2a）随第一个真实 L1 组件同批——今天
它是单条目零使用者，先换机制只会得到空表 + 无人调用的装配（ADR 0032 §6 清单保留）。

## 关键实现事实

- `type` 字段**取值域扩大**属破坏性 manifest 变更，但因缺省值不变 + 旧拼写按别名解析，**旧产物零迁移**
- 与 ADR 0031 的 ABI 31→32 是**两件独立的事**（本票不动 WIT）
- 现有 `ROUTABLE_CAPABILITIES` 只对 L1 有意义（L2 不提供 host-* 同形能力）
- **本票不给任何插件挂角色**：`com.bedcode.terminal-session` 的 `"type": "internal-business"`
  由 **ADR 0031 专项**挂上（与其 `auth-center-register` 动态就绪同批）——只挂静态声明
  会让「谁该先加载」与「是否已就绪」两段判据互相矛盾（spec 4b「不合并」的落地次序）。
  **随之生效的语义**：L2 是角色驱动，它不再进持久化激活表、也不再由 L3 批激活，
  用户在插件管理页停用 terminal-session 只对当前会话生效（下次启动按角色恢复）。
  方向与 fail-closed 一致，但属用户可见变化，需与 ADR 0031 的未就绪提示一并确认口径
- **既存名实不符的处理**：`System` 文档曾称「只停不删」，实际 `uninstall_plugin` 无 kind 拒绝分支。
  本轮不补守卫（L1 零使用者 = 无人调用的死代码），改为**把文档口径改准** + 留在 §6 启用清单
