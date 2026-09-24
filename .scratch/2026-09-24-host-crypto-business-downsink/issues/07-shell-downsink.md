# 07: shell.rs 下沉（ExecutionEnvironment / WindowsShell 随 host-session 退役）

**What to build:** 把环境/发行版选择的业务语义从宿主清出：「执行环境（Windows shell 选择 / WSL 发行版 / Linux 原生）」这类产品级选择不再是宿主类型。随 `host-session` 剩余函数退役（v26 批次），其消费方清零后移除这些类型；宿主 pty 只保留纯引擎的启动规格（argv / env / 工作目录 / 行列 / 环——即「最基础 POSIX 级 exec 参数」），发行版/环境选择的 argv 由插件算好传入。WSL 发行版枚举类能力已由 `host-platform` 提供，环境选择归插件。

**Blocked by:** P1-b land + host-session 退役（`session-engine-downsink` ABI 批次先落地）

**Status:** ready-for-agent

- [ ] `host-session` 退役完成后，环境选择类型宿主消费方清零（grep 断言）
- [ ] 宿主 pty 启动只需裸 exec 参数（argv / env / cwd / 行列 / 环），无环境/发行版枚举；发行版 argv 由插件算
- [ ] 曾消费环境选择类型的宿主调用点全部改经插件互调或移除
- [ ] 线协议/前端受影响面（若有移动端形状）按增量原则复查