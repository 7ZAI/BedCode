# 07: shell.rs 下沉（ExecutionEnvironment / WindowsShell 随 host-session 退役）

**What to build:** 把环境/发行版选择的业务语义从宿主清出：「执行环境（Windows shell 选择 / WSL 发行版 / Linux 原生）」这类产品级选择不再是宿主类型。随 `host-session` 剩余函数退役（v26 批次），其消费方清零后移除这些类型；宿主 pty 只保留纯引擎的启动规格（argv / env / 工作目录 / 行列 / 环——即「最基础 POSIX 级 exec 参数」），发行版/环境选择的 argv 由插件算好传入。WSL 发行版枚举类能力已由 `host-platform` 提供，环境选择归插件。

**Blocked by:** P1-b land + host-session 退役（`session-engine-downsink` ABI 批次先落地）

**Status:** ✅ 完成（2026-09-24）

## 落地

- 移除宿主 `enums/shell.rs` 三大类型：`ExecutionEnvironment` / `WindowsShell` / `SessionLaunchConfig`
- 宿主侧零业务消费（票 11 删内核会话目录后）：PTY 引擎用裸 `CommandBuilder`，业务 argv 由
  插件 `launch.rs::build_argv` 算好经 `host-pty.spawn` 传入；仅 `enums.rs`/`pty.rs` 的 re-export 残留
- 移除面：`enums.rs`（删 `pub mod shell` + re-export 行）、`pty.rs`（删 WindowsShell re-export）、
  `enums/shell.rs` 整文件删除
- 插件 terminal-session 自持 `resolve_environment`（config→launch spec 映射，宿主已解 environment）；
  移动端零引用
- `config.rs::session.default_environment` 是配置字符串（wire 面供插件读，非 shell 枚举），保留
- 门禁：宿主 lib 全量 1055/0

- [x] `host-session` 退役完成后，环境选择类型宿主消费方清零（grep 断言）
- [x] 宿主 pty 启动只需裸 exec 参数（argv / env / cwd / 行列 / 环），无环境/发行版枚举；发行版 argv 由插件算
- [x] 曾消费环境选择类型的宿主调用点全部改经插件互调或移除
- [x] 线协议/前端受影响面（若有移动端形状）按增量原则复查