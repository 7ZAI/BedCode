# 03: `lifecycle: ephemeral` 预留（业务 worker 类型，**不做调度框架**）

**What to build:** 新增第三个正交维度 `lifecycle`（`persistent` 缺省 / `ephemeral`），
把「业务 worker」这个**类型**预留出来。spec 张力 1 / §1.1-1.3。

**Blocked by:** 01

**Status:** done（2026-09-29，**只预留类型**）

- [x] SDK 新增 `InstanceLifecycle` 枚举 + manifest `lifecycle` 字段（缺省 `persistent`、
      `skip_serializing_if` 不落盘）——命名带 `Instance` 前缀，与 `contributes.lifecycle`
      （`LifecycleContribution` 应用生命周期钩子）区分
- [x] `manifest-validate.js`：`ephemeral` + 非 `pluginType: rust` → **构建期拒**（无页面的
      即用即弃形态）；`ephemeral` + `rust` → 也**构建期拒**，文案点名 ADR 0032 §6 与缺口清单
- [x] 宿主**加载期**同样拒（`validation.rs::validate_lifecycle`，经
      `validate_manifest_required` 漏斗覆盖扫描与 zip 安装两条入口）——手写 manifest
      绕过构建链时不得静默当常驻
- [x] 宿主**本票不实现**一次性实例机制与调度框架（用户裁定 2026-09-29）
- [x] 代码注释登记 worker 的技术动机（线性内存只能 grow 不能 shrink + 单实例限额
      `runtime.rs::memory_growing` 不回落 → 长驻实例撞限额 trap 且不自愈），避免后人误删该类型
- [x] §6 启用清单（调度方 / 传参协议 / 权限模型 / per-app 配额 / 内存动机回归锁）
      保持未勾选，作为「预留 ≠ 已实现」的可见凭据

## 关键实现事实

- **修正过一个错误建议**：worker ≠ `CallModel::EventLoop`（后者是**常驻**属主任务，
  与即用即弃语义相反）。`CallModel` 对 `ephemeral` **不适用**
- 现状核查：宿主**无**任何一次性实例机制（实例化只在加载期）；
  `host-task` v20 方向相反（宿主 OS 线程池跑**宿主原语**，`manager/task.rs` 明写
  「不接触 WASM / Store」）
- **为什么必须双侧拒绝而不是静默当常驻**：常驻语义与 worker 的存在理由**正好相反**——
  线性内存只增不减，长驻实例处理大输入会单调涨到宿主单实例限额并 trap，且不重启进程
  好不了。静默降级会让作者以为 worker 生效了
- 传参协议反模式（启用时必须防）：大块数据编进参数 = worker 省下的内存
  在拷贝阶段还回去，**抵消全部意义**
