# 03: session 插件文件浏览域下沉

**What to build:** 本地文件浏览业务（文件树/文件内容/diff 树/文件 diff 四个查询端点，移动端活跃使用）搬入 session 插件工作区/文件浏览域：经 host-fs 原语与 fs_auth 三层校验（路径白名单 → 插件白名单 → 弹窗授权）接管路径越界防护与大小/深度限制；网关别名切换后宿主旧实现退役。安全语义（`../` 穿越、symlink 逃逸拒绝、大文件上限）不得低于现状。

**Blocked by:** 01（网关地基与形状契约锁）

**Status:** done（2026-09-21；插件 native 199 测试 + 前端契约 21 项全绿；宿主 cargo test 1090/0，含新闭环 `test_business_endpoints_dual_track_closed_loop` 双轨逐字节对照 + 确定性错误文案；移动端 vitest 466 全绿；前端测试零改）

- [x] 四个查询端点 URL 与响应形状与今天逐字节一致（双轨对照测试）
- [x] 路径越界拒绝（`../` 穿越、目录逃逸）在插件 fs_auth 链上等效或更严（插件侧单测）
- [x] 大文件/深目录上限与超限行为与今日一致
- [x] 插件 httpEndpoints manifest 清单与插件内部分派表同源（契约用例锁定）
- [x] 宿主旧文件浏览实现退役（contract），回滚仅剩 git revert
- [x] 移动端文件浏览功能回归通过；前端零改

## Comments

### ① 与 spec 决策 6 的必要偏差（已向用户报备后实施）

host-fs 既有原语只有 read(文本)/write/copy/delete/exists/request-auth——**没有目录直读、没有 canonicalize、没有 stat**；host-process 只有异步事件模型（`run` 立即返回 run-id，结果经 `on_process_done` 回调），而插件 `_http_endpoint` 命令面是**同步**路径。四个查询端点的字节级复刻（目录树扫描、canonicalize containment、2MB 上限、git diff 同步执行）在既有原语下**不可能实现**。

裁定：按 spec 决策 4 自身措辞（文件浏览域 = host-fs + host-process + fs_auth）与 ADR 0022 裁剪线（宿主能力 = 离宿主无法实现、且无业务语义的原语），对两个既有 interface 做**函数级追加**（AGENTS.md §7「同一批次内函数级追加不再 bump」，desktop WIT 保持 v19）：`host-fs.read-dir / canonicalize / stat` + `host-process.run-sync`。均无业务语义（目录直读 / 路径归一 / 元数据 / 同步执行属引擎原语）。实施记录：`.scratch/2026-09-21-ticket03-file-browse/log.md`。

### ② 字节级契约边界

成功路径与确定性错误文案逐字一致（404/403/400/413/415 的固定文案、`Not a git repository`、`Working dir is not a directory` 等）；嵌入 OS io::Error 细节的 500/415 尾部允许差异（宿主侧同样是运行时 io 错误字符串，双轨测试只锁 code 与确定性文案）。git diff 成功路径经插件 native 单测（MockGit）覆盖；真实 git 仓库闭环可后续补。

### ③ 宿主退役清单

`file_controller.rs` 删除、`/api/file-tree|file-tree-children|file-content|diff-tree|file-diff` 路由注销、网关五条目翻 PluginRequired；`resolve_working_dir` 移至 `server/services/workspace.rs`（git 域票 04 前仍在宿主侧使用，票 04 已随 git_controller 一并删除）；`file_dto` 保留为形状契约锚点（网关 golden + 双轨闭环测试消费）。

### ④ 接手会话修订（2026-09-21 后半，随票 04 一批）

1. **P0 修复——query 键名大小写**：`file-tree-children` 的 GET query 走宿主
   `FileTreeChildrenQuery`（**无 serde rename，snake_case**：`session_id` / `dir_path` /
   `exclude_dirs`），移动端 `useHttpApi::httpGetFileTreeChildren` 同名构造；而 POST body 的
   DTO 带 `rename_all = "camelCase"`（`sessionId` / `excludeDirs`）。上一轮实现的 children
   handler 读的是 camelCase query——闭环测试也用 camelCase 构造（假绿），真机请求会解析不到
   `session_id` 而 404。已改为 snake_case 并把闭环测试的 query 构造修正为真实 wire 形状。
   本批次新增的 git GET 端点（`git/branches` / `git/status`）同规则（`GitBranchesQuery`
   无 rename → snake），checkout body 带 rename → camel。
2. **错误文案前缀对齐**：`run_git_lines` / `file_diff` 的失败文案补上宿主
   `AppError::Internal` Display 的 `Internal error: ` 前缀，stderr 不再 trim（宿主 lossy
   转换后原样拼接，尾部换行保留）——确定性文案做到逐字节一致。
3. **门禁重跑**（本修订后）：插件 native 206/0（+7 git 域用例）、src-tauri cargo test
   **1086/0**（含闭环 git 段与网关反向守护新断言）、桌面 vitest 778/778、根 eslint 0 error。
   `file_browse` 三文件已 rustfmt 归一；仓内既有文件的历史格式偏差按项目先例未顺手改。