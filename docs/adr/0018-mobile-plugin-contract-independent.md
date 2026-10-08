# 移动端插件契约独立于桌面端维护

桌面端与移动端各持一份独立的 `bedcode.wit`（WIT 单一事实来源），契约层（宿主能力 import 集 + 插件导出回调集）随两端产品能力分化各自演进，同名词义保持对齐（差异表见 `docs/implementation-plans/mobile-wasmtime-component-migration.md` §3.2），传输机制层（组件模型 + wit-bindgen）两端一致。

移动端有意**不含** session 能力：会话状态机在桌面端，移动端曾以 noop 占位（被删除的代码化石）；契约中缺失使插件编译期即失效，优于运行时拿到空值——这一不对称是产品决策，不是遗漏。共享超集方案被否决：会把移动端宿主永远 `unreachable!` 的接口泄漏给插件 SDK。对齐语义的历史记录：`docs/implementation-plans/mobile-wasmtime-component-migration.md` §8（Q2/Q3）。

## 双端偏离登记（增量）

- **`com.bedcode.terminal-session` 两端同名不同职责（2026-10-08，票 12 / spec D6 选项 A）**：
  桌面端是会话 / 任务 / 终端的**权威**（持有 PTY、WS 服务端端点、认证中心桥接）；移动端
  是**远程终端控制端**（只做订阅消费、会话控制调用，spec ADR 0012「手机看、桌面管」）。
  同名 id 不构成两端共享 WIT / ABI / 权限集 / 存储 schema 的任何推断依据（C8）；两端契约
  各自独立演进，差异以各自 WIT 注释 + 双端 code-map 为真源。**票 16 补充（2026-10-08）**：
  移动端 app 职责面扩为「终端订阅消费 + 认证 / 配对编排 + 任务域只读投影」——原独立插件
  `com.bedcode.auto-task`（任务队列面板 / 工具箱「任务记录 + 定时任务」两页签）已并入本 app，
  旧 id 退役（fail-visible 三形态 + 防回接锁）；桌面端任务域权威语义不变（「手机看、桌面管」）。
- **`host-connection` 同名不同形（2026-10-08，票 12）**：桌面是 15 函数连接上下文域
  （connection-context 安全上下文查询面）；移动端只暴露 1 函数 `primary-target`（主连接
  目标引擎事实，无权限门）。`host-websocket`（桌面 15 函数 / 移动客户端 5 函数子集，
  ADR 0041）之后第二个「同名接口移动子集」先例。