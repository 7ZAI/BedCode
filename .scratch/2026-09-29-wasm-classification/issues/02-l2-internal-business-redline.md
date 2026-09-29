# 02: L2 反向依赖红线登记 + 防回接锁

**What to build:** L2「内部统一业务应用」是宿主**唯一反向依赖**的类别（宿主主动调它），
必须把三条约束落成代码与锁，不能只写文档。spec §4「L2 是唯一反向依赖类别」。

**Blocked by:** 01

**Status:** 部分 done（2026-09-29）——红线**锁**已落地；约束①②③的**运行时实现**
（注册表 / fail-closed / 未就绪可见信号）随 ADR 0031 认证中心一起落地，不在本票。

- [x] 约束①**白名单式登记**的**可锁部分**：`internal_business_host_dependency_stays_gated`
      ——宿主源码里碰 L2 桥接面（`utils/auth/auth_center` / `host_api/auth_center` 注册表）
      的文件集合 ⊆ `L2_CONSUMER_ALLOWLIST`，新增消费点必须登记进白名单并写明理由
      （登记 = 白名单式；发现 = 显式审计过的文件集，不是自动扫描猜测）
- [x] 约束②③**只做安全闸门 / 只转发不解释**：`l2_gate_returns_decision_only` 锁住
      `enforce_connection_policy` 的签名必须只回裁决（`Result<(), String>`）——
      成功态一旦带上宿主可解析的产品载荷即转红
- [x] 第四条锁 `host_switches_on_role_predicates_not_role_values`：宿主**只按谓词**判角色，
      不出现 `PluginKind::X` 字面量（可扩 `ROLE_VALUE_ALLOWLIST`，默认空）——防的是
      「分类学退化成宿主按角色名做业务分支」
- [ ] 约束①的**注册表本体**（单中心唯一性仲裁）：ADR 0031 K1
- [ ] 约束②③的**运行时面**（fail-closed + 组合式认证零解析转发）：ADR 0031 K3/K6
- [ ] L2 未就绪的**可见信号**：**已落** `error` 日志 + `deny_kind=no_center`（`boot.rs` 激活
      就位点，ADR 0031 实施时带进来的）；**未落** core-monitor 计数 + 前端一次性提示——
      没有后者，fail-closed 在用户侧表现为「所有东西都连不上」而无处下手
      （故本行保持未勾选：三项只落其一）
- [x] ADR 0032 转「已实施（机制层）」，未落地项在 §6 清单与本票留痕

## 关键实现事实

- 这三条是**安全闸门**，属 ADR 0022 §5.1.3 允许的宿主薄壳，不越 §5.1 红线
- 反向依赖是**方向性**问题：L1/L3 都是「宿主提供 / 被调用」，只有 L2 是「宿主主动调」——
  写锁时按这个方向性断言，**不**泛化成「宿主不许调插件」
- 白名单里的每个条目都带理由（网关/WS 认证中间件 = 安全闸门；`session_gateway` = 零解析
  窄转发；`host_api/auth*` = 组合式认证原语 + 唯一性仲裁；`boot`/`activation` = 停用回收
  与启动对账的生命周期闸门接线）
- **锁会与 ADR 0031 并行演进**：其新增的 `host_api/auth.rs`（4 个 `host-auth` 原语）与
  `host_api/auth_center.rs`（注册表）已登记进白名单；白名单条目「不再命中」时锁会显性报错，
  防止白名单腐化后静默放行越界消费点
