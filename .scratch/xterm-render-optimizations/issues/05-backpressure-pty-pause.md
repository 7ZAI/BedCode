# 05 — 背压：PTY 暂停/恢复

**What to build:** 背压反馈环的 Rust 侧——未 ack 字节/水位超阈值时暂停从 PTY read，ack 推进后恢复；暂停期间零字节丢失，恢复后继续转发全部输出。与 04 并行，共享 spec 中的 wire 契约。

**Blocked by:** None — can start immediately（与 04 并行，共享 spec 中 wire 契约）

**Status:** resolved（实现完成）

- [x] 未 ack 字节/水位超阈值时暂停从 PTY read（`PtyReader` 每次 read 前查 `GlobalOutputManager::should_pause`（try_read 非阻塞零锁），超 1MB 水位 → sleep 5ms 重查；`start_with_pause` 提供注入点供测试）
- [x] ack 推进后恢复读取（ack 帧经 `terminal_ws` 入站二进制 → `GlobalOutputManager::ack` → `SessionOutputManager::on_ack` 按序弹出 FIFO 精减 unacked_bytes，降至水位下自然恢复）
- [x] 暂停期间零字节丢失（暂停只在两次 read 之间，绝不中断半途 read；PTY 内核管道缓冲积聚，子进程写满即自然阻塞 = 真背压传导），恢复后全部转发（与快照重订阅/重播跳过语义兼容）
- [x] ack 帧 wire 编码单测（control_frame.rs parse_ack_frame：合法/魔数错/非 ack 标志/长度越界/非 UTF-8/偏移逐字节断言，3 用例）
- [x] `MemoryReader` 测试模式覆盖暂停/恢复/无丢失（RecordingReader：暂停 30ms 零读取 → 恢复全量字节无丢失 → EOF 退出）；`cargo test --lib` 567 全绿（pty_reader/session_output/terminal_ws 各模块）
