# 06 — 断点续传

**What to build:** 传输中断后重试从接收端已写偏移续传而非从头开始：发送端重发 intent 时携带接收端回报的偏移，数据面按 Range 续写；多文件批内逐文件记录断点，已完成文件跳过。覆盖三类中断——发送方取消后重试、连接断开、接收端进程被杀。

**Blocked by:** 05

**Status:** ready-for-human

- [x] harness：传输中途掐断连接 → 重连续传 → 文件内容校验一致（`tests/transfer_session.rs::connection_drop_mid_file_resumes_and_content_matches`：手摇帧推 48KiB 后硬掐 TCP，重发同 batch_id 续传完成、内容逐字节一致）
- [x] 接收端进程重启后续传仍自正确偏移开始（断点真源在落盘侧）（`receiver_process_restart_resumes_from_disk_truth`：整个接收节点实例销毁后仅凭持久化目录重建——身份稳定、续传基线 = 第一腿已写字节）
- [x] 多文件批：部分完成后重试只补未完成文件（`multi_file_batch_retry_supplements_only_missing_files`：f0 完整落位 + f1 半程中断 → 重试只补 f1 余量与 f2 全量）
- [x] 已完成文件不重复传输；总进度按批聚合正确（同测试基线算术：第二腿首进度 ≥ f0 满额 + f1 已写；进度口径改为批内累计含续传基线，跨会话单调收敛至总量；另覆盖发送方取消后重试 `sender_cancel_then_retry_resumes_from_kept_part`）

实现说明：引擎侧 `run_receive` 逐文件前置断点扫描（已完成跳过发满偏移 StartFile+FileDone、尺寸不符提前 duplicate-name 失败、脏超长 .part 弃用并截断打开）；双端 Progress 计数改为批累计口径（StartFile 偏移入账基线，`RateTracker::sync_base` 防速率尖峰）。`cargo test` 通过（61 lib + 1 discovery + 9 harness + 12 transfer_session）。
