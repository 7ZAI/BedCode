# 03 — 补 PtySession 行为测试（start / write / resize / kill 阶梯 / Drop）

**What to build:** `PtySession` 的 12 个 pub 方法中有 8 个无任何测试触达（7 个完全零覆盖 + `kill()` 仅未 start 退化路径）。补上真实 PTY 行为测试，覆盖 kill 四级杀进程阶梯与 `write` 分块约束。

**Blocked by:** 无

**Status:** done（2026-09-15 修复）

- [ ] `start()` 真实 spawn 后 `is_running() == true`、`process_id` 非 None、`name()` 正确
- [ ] `write_str` + `write` 写入 echo 命令 → 经 `start_output_reader` 读回（可在 `src/pty` 内用内存/管道假 reader 隔离 PTY 依赖，或走真实 PTY）
- [ ] `write()` 8KB 大负载分块写入不失败（验证 `CHUNK_SIZE: usize = 4000`（`pty_process.rs:169`）的分块约束真实有效）
- [ ] `resize()`（`pty_process.rs:217`）真实生效（写入后可调 cols/rows，断言不 panic）
- [ ] `send_special_key("ctrl_c")` / `"ctrl_d"` / `"invalid_key"` 三分支（含 `AppError::InvalidKeyCombo` 错误路径）
- [ ] `subscribe_lifecycle()`（`pty_process.rs:237`）能收到后续状态迁移
- [ ] `kill()`（`pty_process.rs:247`）**graceful 四级阶梯**：Ctrl-C → `\nexit\n` → Windows `taskkill /pid /t /f` / Unix `kill -9` → 4s 超时 `force_kill`；至少断言 start 后 kill 完成、进程树清理、`is_running() == false`
- [ ] `Drop`（`pty_process.rs:327` `fn drop(&mut self)`）：drop 后 PTY 释放、`is_running() == false`
- [ ] `new_generates_unique_session_ids` 保留或改为更强断言（当前近乎恒真，ID 由 `process::id` + 单调 `AtomicU64` 构造即保证唯一）
- [ ] `cargo test --lib pty::pty_process` 通过

> 复核（2026-09-14）：pub 方法实为 12 个（`new`/`with_id`/`id`/`name`/`start`/`write`/`write_str`/`send_special_key`/`resize`/`subscribe_lifecycle`/`is_running`/`kill`；`start_output_reader` 为私有）。8 个无测试触达的结论不变。

## 证据

`grep -rn` `tests/` 目录，关键字 `PtySession|start_output_reader|send_special_key|write_str|resize` 的命中数为 **0** —— 集成测试全程经 WS → `SessionManager` 间接驱动，单测层碰不到 PTY 行为本身。

现有 2 个测试里 `kill()` 被调用的场景是**未 start**：

```rust
let session = PtySession::with_id("test-session-001".to_string(), config())...;
session.kill().await.expect("kill should succeed");   // process_id: None → 跳过 taskkill
assert!(!session.is_running());
```

即 `kill()` 里 `if let Some(pid) = self.process_id` 的整个 `Some` 分支（含 Windows `taskkill /t /f`）0% 覆盖。

## 风险点（注释记录的修复无测试守卫）

`kill()` 注释明确记录：Windows 上 bash 是 WSL2 内部进程，`bash.exe` 的 Ctrl-C 无效，**必须 `taskkill /pid /t /f` 递归杀整树**。`send_special_key` 注释记录 bash 下 Ctrl-C 需补 `\n` 才能送达。这两个平台相关修复**没有任何测试守着**，且 Linux 上走 `kill -9` 分支、Windows 上无 CI —— 改坏了全部 17 个测试仍会绿。

`resize()` 注释记录 Windows 上 `PtyProcess` dropped 后 PTY 关闭的竞态，同样无测试。

## 建议形态

真实 `openpty` 已在 `with_id_creates_session_with_properties_and_kill_stops_it` 验证过可跑（Linux 本机通过），因此可以直接走真实 PTY：

1. `let mut s = PtySession::new(config()).unwrap();`
2. `s.start().unwrap();`
3. 写入 `echo MARK_<session_id>` → `start_output_reader` 读回断言
4. `s.resize(cols, rows).unwrap();`
5. `s.send_special_key(...)` 三分支
6. `s.kill().await` → 断言 `!is_running()` + 进程树清理（可 `ps` 校验子进程已消失）

注意：真实 PTY 测试在 Windows 上走 ConPTY，需区分环境问题与逻辑缺陷（参考 `.scratch/desktop-integration-tests/spec.md` 场景 6 的约定）。建议在单元测试里只覆盖平台无关路径，平台特定阶梯放集成层。

## Comments

- 2026-09-14 审计发现，见 `../spec.md` §5.4
