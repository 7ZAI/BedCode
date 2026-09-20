# 票 03 实施记录：session 插件文件浏览域下沉

Status: done（2026-09-21）
Date: 2026-09-21
关联: `.scratch/2026-09-20-host-business-decarriage/issues/03-file-browse-domain.md`

## 关键发现与裁定（与 spec 决策 6 的偏差，已向用户报备后实施）

**发现**：host-fs 现有原语只有 read(文本)/write/copy/delete/exists/request-auth——
**没有目录直读、没有 canonicalize、没有 stat**；host-process 只有异步事件模型
（`run` 立即返回 run-id，结果经 `on_process_done` 回调），而插件 `_http_endpoint`
命令面是**同步**路径。四个查询端点（file-tree / file-tree-children / file-content /
diff-tree / file-diff）的字节级复刻（目录树扫描、`../` 穿越 + symlink 逃逸的
canonicalize 判定、2MB 大小上限、git diff 同步执行）在既有原语下**不可能实现**。

**裁定**：按 spec 决策 4 的自身措辞（文件浏览域 = host-fs + host-process + fs_auth）
与 ADR 0022 裁剪线（宿主能力 = 离宿主无法实现、且无业务语义的原语），对两个既有
interface 做**函数级追加**（AGENTS.md §7「同一批次内函数级追加不再 bump」，desktop
WIT 保持 v19）：

| 追加 | 所在 interface | 语义（引擎级、无业务语义） |
| --- | --- | --- |
| `read-dir(path) -> JSON[{name,nodeType}]` | host-fs | 目录直读（DirEntry::file_type 语义，symlink=other） |
| `canonicalize(path) -> option<string>` | host-fs | 绝对路径归一（containment 判定） |
| `stat(path) -> JSON{size,isFile,isDir}` | host-fs | 文件元数据（大小上限） |
| `run-sync(request-json) -> {exitCode,stdout,stderr,timedOut}` | host-process | 同步进程执行（git diff 用；run 是异步事件模型不适配同步 HTTP） |

权限：read-dir/canonicalize/stat 走 `fs:read` + fs_auth（session 插件在插件白名单，
自动放行）；run-sync 走 `process:run`（session 插件 manifest 需新增该权限）。

## 实施顺序

1. WIT 追加（host-fs ×3 + host-process ×1）
2. SDK trait + wasm_host 绑定（host/fs.rs、host/process.rs）
3. 宿主实现（host_impl/fs.rs、host_impl/process.rs、component.rs Host impl）
4. 插件 file_browse 域（模型/源端口/ops/HTTP 分派）+ manifest（httpEndpoints + process:run）
5. 插件单测（containment / exclude / tree / diff parser / 形状）
6. 宿主退役（file_controller 删除、app.rs 路由、网关 fallback 翻 PluginRequired）+ 双轨闭环测试
7. 重建插件产物 + 全量验证

## 并发状态提醒

宿主 cargo test 仍被 peer 线（票 05/06）在途文件阻断（peer_engine_receive.rs 3 处
未定义引用）；本票宿主侧以 `cargo check --lib`（仅 peer 线报错）+ 插件侧全量测试
为验证边界。

## 后续会话修订（2026-09-21 接手会话，随票 04 一批）

- peer 线阻断已由票 05/06 会话解除，本票门禁以全量口径补跑：src-tauri cargo test
  **1086/0**（含闭环 git 段）、插件 native 206/0、桌面 vitest 778/778、eslint 0 error。
- **P0 修订**：children handler 的 GET query 键名修正为 snake_case（原 camelCase 实现
  与闭环测试同样写错，真机请求会 404）——详见票 03 Comments ④。
- 票 04（git 三端点）已并入本模块落地；`file_browse` 三文件已 rustfmt 归一。
- 并发事件记录：本会话接手时曾与上一会话短暂并行（其残留 source/ops/mod 与本会话
  起草的 model/ports/browse_ops/git_ops 交错），已按「自建文件可删」原则清理本会话
  草稿、采纳上一会话架构（source.rs/ops.rs/mod.rs），在其基础上继续修订。
