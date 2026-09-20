# 04: session 插件 git 工作区能力下沉

**What to build:** 把当前僵尸形态的 git 业务（分支列表/状态/checkout 端点，桌面与移动端均无消费方）复活为 session 插件工作区 git 域的组件化能力：经 host-process（非交互进程原语）+ host-fs 实现，响应契约（含非 git 仓库判定语义）通过网关对外保持；宿主侧 git 业务模块与路由正式退役，将来加 UI 时直接在插件 API 面上接。

**Blocked by:** 01（网关地基与形状契约锁）

**Status:** done（2026-09-21；插件 native 206/0（含 git 域 7 用例）+ src-tauri cargo test **1086/0**（含闭环 `test_business_endpoints_dual_track_closed_loop` 扩展的真实 git 仓库段）+ 桌面 vitest 778/778 + 根 eslint 0 error；移动端零改动零验证责任）

- [x] `/api/git/*` 响应契约保持（分支/status/checkout 语义、非 git 仓库判定、参数校验），无消费方不改变契约——非仓库 branches 答 200 `isGitRepo:false`、status/checkout 不做 `.git` 预检由 git 非零退出答 500（宿主同格）；错误文案含 `Internal error: ` 前缀与 stderr 原样拼接逐字节一致
- [x] git 操作经插件 API 面可调用（插件侧契约测试锁语义）——`file_browse::ops` git 查询域（`git_branches` / `git_status` / `git_checkout` + `is_valid_branch_name` 白名单）7 条 native 单测（MockGit 注入 + 宿主文案逐字断言 + 白名单宿主用例逐条对齐）；宿主侧真实 git 仓库闭环覆盖 branches/status/checkout/白名单拒绝
- [x] 宿主 git 业务模块与路由退役（contract），回滚仅剩 git revert——`git_controller.rs` 与 `server/services/workspace.rs` 删除、`/api/git/*` 路由注销、网关三 git 条目翻 PluginRequired；`git_dto` 保留为形状契约锚点（网关 golden 消费）；网关新增反向守护（PluginRequired 条目不得再挂宿主路由）
- [x] 插件 httpEndpoints manifest 清单与插件分派表同源（契约用例锁定）——manifest 27 条 = task 17 + 业务 2 + file_browse 8（含 git 3）；`task/mod.rs` 并集测试 + TS 契约测试同步
- [x] cargo test / eslint 0 error；前端零改

## Comments

### ① 落点与能力映射（无新增宿主原语，desktop WIT 保持 v19）

git 查询域并入 `file_browse` 模块（`ops.rs` git 查询域段 + `mod.rs` 三端点 handler + `source.rs::GitPort` 复用）——与文件浏览域共享 working_dir 解析、fs_auth 链与 `run-sync` 原语，同一域模块承载「工作区」两域。命令执行全部经 `host-process.run-sync`（argv 数组不经 shell；分支名白名单是纵深防御）；`git init` 级别的写操作不存在——checkout 是唯一变更型命令，白名单前置。

### ② 双轨注记

宿主旧实现在本票 contract 前**从未有消费方**（审计确认僵尸），故「双轨对照」不适用于本票：对照锚 = 网关 golden（`git_dto` 形状）+ 插件 native 用例（宿主文案逐字）+ 真实 git 闭环（行为等价）。