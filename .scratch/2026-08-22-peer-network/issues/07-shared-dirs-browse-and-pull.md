# 07 — 共享目录暴露 + 远端浏览/拉取（引擎层）

**What to build:** 两端对称的共享目录注册：移动端 SAF URI 条目（系统目录选择器选择、持久授权、重启有效）+ app 私有下载目录免授权特殊条目；桌面端为用户选择的文件夹。可信对端可列目录（目录优先、按名排序）并拉取文件落入本机下载目录——移动端写 MediaStore.Downloads 失败回退私有目录，桌面端写入接收落点。仅可信对端可浏览，恒只读。引擎与适配层在 harness 上验证；SAF/存储适配沿用移动端现有无头测试惯例。

**Blocked by:** 05

**Status:** ready-for-human（引擎与两端适配层完成；SAF 真机冒烟待人工）

- [x] harness：B 向 A 暴露目录 → A 列目录见条目 → 拉取文件落 B 下载位置且内容一致
      （`packages/peer-net/tests/shared_dirs.rs`：`fs_root_browse_lists_sorted_and_pull_lands_identical_content`，
      含目录优先/按名排序断言与子目录下钻；SAF 缝全链路 `saf_root_flows_through_host_seam_for_browse_and_pull`）
- [x] 未信任节点列目录/拉取被拒
      （`untrusted_dial_is_gated_before_any_share_session`：首连闸门默认拒绝，共享会话结构性不可达）
- [x] 只读约束有测试：任何写路径被拒
      （`traversal_and_absolute_pull_requests_are_rejected_readonly`：越界/绝对路径 PullRequest 一律
      Decision{not-found} 且源目录零写入；线协议无任何指向暴露端的写语义帧；
      路径清洗纯函数 `resolve_rel_path` 无头单测在 `shared/registry.rs`）
- [ ] Android SAF 授权条目重启后仍可读（真机冒烟）——注册表落盘持久化已由
      `store_roundtrips_through_disk_reload` 单测覆盖，真机验证待人工执行
- [x] 移动端落点 MediaStore 回退逻辑有单测
      （`bedcode-mobile/src-tauri/src/peer_net.rs::share_landing_tests`：提升成功删私有副本 /
      失败保留私有副本两分支 + MIME 映射，fake SafIo 无头注入）

## 实现摘要（2026-08-23）

- **crate**（`packages/peer-net/src/shared.rs` + `shared/registry.rs`）：`SharedDirStore`
  原子 JSON 持久化（trust_store 同款模式）、内置免授权下载条目不落盘、`SharedDirHandler`
  首帧分流（Offer→push 接收管线 / BrowseRequest / PullRequest）、拉取复用 issue 05/06
  数据面（断点真源、取消、进度全继承）；线协议新增 `browse_request`/`browse_response`/
  `pull_request` 帧 + `RejectReason::{NotFound,ReadFailed}`；SAF 缝 = `SharedSafAccess` +
  `SeqReader`（宿主实现、fake 注入测试）；只读为结构性约束（无写语义帧 + 路径清洗）。
- **transfer.rs 微重构**：抽取 `receive_files_after_accept` 供 pull 接收复用；新增
  `FileLanding` 落位钩子（rename 后回调）。
- **桌面端**（`peer_net.rs`）：handler 换装 SharedDirHandler（占位 handler 移除），接收
  落点 `Downloads\BedCode\`，命令 `list/add/remove_shared_directory` 已注册。
- **移动端**（`peer_net.rs`）：`SafSharedAccess` 适配既有 SafIo（同步 list_tree 下降 +
  open_stream 句柄定位读，Drop 收尾 fd）；内置「下载」条目；`MediaLanding` 提升+回退；
  命令 `list_shared_directories` / `add_shared_directory_saf`（系统目录选择器）/ 
  `remove_shared_directory` 已注册。
- **测试**：crate `cargo test` 94 全绿（68 lib + 1 discovery + 9 harness + 12 transfer_session
  + 4 shared_dirs）；移动端 `cargo test peer_net --lib` 3 绿；两端 `cargo check` 通过
  （桌面端后续由并行 issue 08 会话继续演进同一文件）。
