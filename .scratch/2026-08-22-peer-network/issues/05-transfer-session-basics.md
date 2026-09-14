# 05 — 传输会话基础：批协商 + 接收策略 + 进度/取消

**What to build:** 以 git 标签 v2.0.0 的移动端 actix 服务端代码为种子，去 host 化（剥离终端 WS 与 Announce token 依赖）后改造为 peer-net 的传输会话引擎：控制面跑批协商与应答，数据面沿用既有 HTTP Range / 上传 session 契约。接收端按全局接收策略放行——每次询问（整批接受/拒绝、超时自动拒）、直接接收、直接拒绝；发送端可取消进行中任务；进度事件（已传字节/速率/状态）持续上报。harness 中单文件推送 A→B 全生命周期绿灯。

**Blocked by:** 02

**Status:** ready-for-human

- [x] harness：A 推单文件到 B，三种策略分支行为各自正确（`tests/transfer_session.rs`：always_accept / always_deny / ask×accept+reject）
- [x] 询问超时自动拒绝且 A 收到终态（`ask_timeout_auto_rejects_and_sender_receives_terminal_state`）
- [x] 双向可取消（发送方/接收方），对端收到中断并落正确终态（`sender_cancel_*` / `receiver_cancel_*`，含 RST 吞帧防护）
- [x] 进度事件序列完整（含速率）；断点真源 = 接收端已写字节的约定在此层生效（`receiver_written_offset_is_resume_truth_source`：预置 .part 后发送端进度恰为 全量−已写偏移）
- [x] 批状态机纯函数沿用无头惯例独立单测；`cargo test` 通过（60 lib + 1 discovery + 9 harness + 8 transfer_session）
