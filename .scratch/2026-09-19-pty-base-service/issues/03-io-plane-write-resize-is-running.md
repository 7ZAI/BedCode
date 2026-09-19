# 03: 数据面补齐——write / resize / is-running

**What to build:** 插件能把自己的 PTY 用起来而不只是看：向进程写输入并回读到进程的回显/响应，按终端尺寸 resize 让全屏程序输出对齐，随时用 is-running 快照查询进程是否还活着（事件丢失后自愈的唯一手段）。fixture 演示插件在本票后可跑「spawn → 写 → 拉回显 → resize → 查询」的完整交互回路。

**Blocked by:** 02（host-pty 契约定稿 + spawn→ring-fetch 最小贯通）

**Status:** done（2026-09-19；数据面三函数实装 + e2e 交互回路贯通，桌面 lib 980 绿 / 宿主 21 例 / SDK 84 例 + 双 wasm target 绿，契约与变异见 Comments）

## 已定案的行为（spec D6/D9/D11）

- **write**：`list<u8>` 直传（不 base64/JSON 包装，对齐 binary 先例）；分块语义在宿主内建，复用既有分块 + 让出策略；超单次上限直接拒绝（上限常量随票 05 定，本票按现值实现并留常量位）。
- **resize**：透传列/行到 PTY 尺寸；无错即视为生效，不承诺同步生效时序（插件按输出对齐验证）。
- **is-running**：布尔快照，**属主校验前置**；定位是「bus 不缓冲不重放」下的自愈兜底，不是事件替代品。
- **明确不做**：特殊键/组合键 API（键盘组合属终端业务语义，归插件层用 write 自行编码）；close/reopen（kill 即销毁，见票 04）。
- **SDK**：补齐三个函数的类型化包装 + 文档注释（camelCase 字段、轮询时序建议、write 上限语义）。

## 验收

- [x] fixture 闭环：向交互式命令 write 输入后，`ring-fetch` 能拉到该输入的回显/进程响应字节（`/bin/sed s/^/OUT:/`——前缀只可能由进程加，排除 tty 本地回显）
- [x] resize 生效可验证：**行为等价断言**——进程侧 `stty size` 反查内核 winsize 由 `30 100` 变 `24 80`；e2e 侧断言无错 + 回 ok。原因记录见 Comments「resize 断言口径」
- [x] is-running 在进程存活期为 true、退出后为 false；非属主查询被拒
- [x] 权限：仅声明 `pty:io` 的插件可调用三函数；越权调用被 deny（只授 `pty:spawn` 者三函数一律 `permission denied: pty:io`）
- [x] write 超限返回明确错误（不静默截断）；错误带操作上下文，无 `let _ =`
- [x] 宿主单测 + SDK 单测/wasm32 check 全绿；桌面 `cargo test` lib 980 绿 + `tests/pty_session_chain.rs` 业务链路绿；测试后无 PTY 进程残留（唯一外部红项见票 02 记录）

## Comments

（实施记录追加此处）

### 2026-09-19 实施落地

**改动文件**

- `pty/lifecycle.rs`：`PtyTerminationGate::reader_closed()`——终态信号 ① 的只读访问器（此前只能从「是否发出事件」间接推断）。
- `pty/pty_process.rs`：`PtySession::output_terminated()` = `gate.reader_closed()`。**刻意不改 `running` 标志**：引擎的 `running` 只在 `kill()`/`Drop` 翻下，子进程自然退出时仍为 true（票 01 已实证 EOF 在业务 `Hold` 下不可观测），若直接改 `running` 就是跨线行为变更。故新增判据、由 host-pty 侧合成，业务线零感知。
- `host_impl/pty.rs`：
  - `pty_write`：权限门 → **准入上限** `PLUGIN_PTY_MAX_WRITE_BYTES`（超限零字节写入）→ `with_entry` 取会话克隆 → 锁外 `block_on_async(session.write)`（分块 4000 + 逐块 yield 的节奏在引擎侧，本层不重复实现）。
  - `pty_resize`：权限门 → 属主 → `session.resize(cols, rows)`；`Ok` 只表示提交内核（SIGWINCH 通知前台组），文档写明不承诺同步生效时序。
  - `pty_is_running`：权限门 → 属主 → `session.is_running() && !session.output_terminated()`。
  - 守卫收敛为 `with_entry`（锁内只查表 + 克隆，绝不跨锁 await），`session_of` / `guard_owner` / `not_found` 为其薄封装；`pty_ring_fetch` 改走同一守卫（原内联查表删除）。
  - `PtyEntry.session` 去掉票 02 的 `#[allow(dead_code)]`——数据面已是真读取点。
- `system/constants/plugin.rs`：`PLUGIN_PTY_MAX_WRITE_BYTES = 64 KiB`（≈16 个 4000 分块，够一次粘贴级输入）。票面写「上限常量随票 05 定，本票留常量位」——**不留空位而是定值**：无上限的 write 在语义上不可测试（无法断言「超限拒绝」），票 05 若要复核只改一个常量与一处用例。
- SDK `host/pty.rs`：三函数文档补齐（分块与 64 KiB 准入、超限零副作用、SIGWINCH 时序、`pty_write → 20~50ms 续拉到 next_offset 不再前进` 的轮询建议、`is-running` 的 false 与 Err 语义差别）。签名与 WIT 一致，无 ABI 变化。
- fixture `packages/plugin-pty-test`：新增 `pty-write`（`bytes` 为 u8 数组）/ `pty-resize`（cols/rows）/ `pty-is-running` 三命令 + `require_bytes` / `require_u64` 助手；宿主侧新增 `pty_activate_fixture` helper。

**行为契约与测试**

| 契约 | 来源 | 规则 | 用例 |
| --- | --- | --- | --- |
| C-101 | 票面 write | ≤ 上限整块送达进程（非 tty 回显） | `write_response_comes_from_process_not_tty_echo`（sed `OUT:` 前缀）、e2e `test_pty_interactive_io_loop_roundtrip` |
| C-102 | 票面超限 | \> 上限 → Err 带常量、**零字节**进进程 | `write_over_limit_is_rejected_without_partial_input`（拒后一行合法写入仍被 `read` 完整取到 = 无残渣） |
| C-103 | `>` 判定边界 | 恰好等于上限放行且整块送达 | `write_at_limit_is_accepted_and_delivered_whole` |
| C-104 | 票面 resize | winsize 真被内核改掉（进程侧可观测） | `resize_changes_kernel_winsize_observed_by_process`（`30 100` → `24 80`） |
| C-105 | 票面 is-running | 存活 true / 自然退出 false | `is_running_true_while_alive_and_false_after_natural_exit` + `reader_closed_accessor_tracks_signal_one` |
| C-106 | D8 两域 | 只授 `pty:spawn` → 数据面三函数一律 denied | `io_apis_without_io_permission_are_denied` |
| C-107 | D2 属主 | 三函数非属主 → `not owner of pty handle`（先于任何业务判定） | `io_apis_enforce_owner` |
| C-108 | 票 01 地基 | `running` 语义不变、业务线零感知 | `pty::` 83 例（含真 PTY）+ `tests/pty_session_chain.rs` |

**resize 断言口径（票面第 2 条的取舍）**：票面允许「不可行时按行为等价断言并记录原因」。测试内跑一个真实全屏 TUI 并断言「渲染对齐」不可自动化（帧栅格化需人眼/像素比对），故取**因果链两端**：① resize 之后进程读到的 `stty size` 变了（内核 winsize 事实源，等价于「对齐的前提成立」）；② WASM seam 上 resize 无错且回 ok。宿主层用①、e2e 用②。

**竞态处理**：`resize` 用例的进程脚本为 `stty size; read go; stty size`——第二次读取由测试在 resize 之后 `write` 解锁，因此「resize 已提交」与「第二次读取」之间**无时间竞态**（不用 sleep 赌窗口）。同理 `is-running` 用 `read go` 阻塞/解锁来构造存活与退出两态，单用例覆盖状态迁移，不留常驻进程。

**实跑证据**

- `cargo test --offline --lib host_impl::pty` → **21 passed; 0 failed**
- `cargo test --offline --lib test_pty`（两条 e2e）→ **2 passed; 0 failed**
- `cargo test --offline --lib` → **980 passed; 0 failed**；`--test pty_session_chain` → ok
- `cargo test --offline --lib pty::`（引擎 + 宿主同名过滤）→ 83 passed
- SDK：`cargo test` **84 passed**；`cargo check --features wasm --target wasm32-unknown-unknown`（sdk-publish CI 口径）与 `--target wasm32-wasip3`（nightly-2026-09-16，现役 fixture 链）均 Finished 无 error
- `rustfmt --check`：`host_impl/pty.rs` / `lifecycle.rs` 0 diff（`pty_process.rs` 剩 1 处属票 01 在途 hunk，未越界重排）；`cargo clippy --tests` 对本票文件无告警
- 测试后 `ps` 复查：无 `sed` / `cat` / `stty` / `pty-reaper` 残留、无僵尸（常驻进程均随测试二进制退出时的 master 关闭收 SIGHUP）

**变异自检（实跑，逐发还原后复验 21 例全绿）**

| 变异 | 结果 |
| --- | --- |
| 去掉 write 准入上限（条件短路） | `write_over_limit_is_rejected_without_partial_input` FAILED ✅ |
| `pty_is_running` 去掉 `!output_terminated()` | `is_running_true_while_alive_and_false_after_natural_exit` FAILED ✅ |
| 上限判定 `>` → `>=`（off-by-one） | `write_at_limit_is_accepted_and_delivered_whole` FAILED（其余 20 例仍绿）✅ |

两发变异同时命中同一轮时结果为 `18 passed; 2 failed`——各变异只杀各自用例，无串扰。

**未覆盖风险 / 交后续票**

1. **`is-running` 的 Err 侧未测**：句柄被摘除后应回 `pty handle not found`（而非 false），但摘除路径属票 04（kill / 退出事件 / 停用回收），本票只有「未知句柄」的 not-found 断言（`ring_fetch_on_foreign_handle_is_not_owner`）。票 04 落地时补三函数的摘除后 Err 断言。
2. **write 不测「环淘汰下的写侧行为」**：环满只影响读侧历史（票 05 的背压矩阵覆盖），写侧无环压力。
3. **`pty_write` 在进程已退但句柄未摘（票 04 前的窗口）时**：引擎 `write_all` 于已关闭 master 可能报 EIO → 现为带上下文的 `pty write: 写入失败 (pty_id …)`，不 panic；票 04 摘除后该窗口收敛为 not-found。
4. **非 Linux 未验证**：数据面用例全部 `#[cfg(target_os = "linux")]` 门控（真 PTY 依赖），ConPTY 下的 `resize`/`write` 行为待发布前实机验证（同票 01/02 注记，收口票 07）。
5. **canonical 模式行长假设**：边界用例的 64 KiB 载荷刻意由短行组成（内核 `MAX_CANON` ~4 KiB 会卡无换行整块写入）。真实插件若要写无换行的二进制大块到 canonical 终端，可能阻塞在引擎分块写——属终端行规程而非 host-pty 语义，票 05 评估是否在文档中提示插件先 `stty -raw`（业务性包装仍归插件层）。

### 2026-09-19 票 04 落地后的连带修正（由接手票 04 的会话追加）

票 04 的「exit 即摘除」使本票 8 例以 `/bin/true`、`/bin/echo`、`/usr/bin/env`、`/bin/stty` 为载体的用例失效（断言前句柄已被摘走 → `pty handle not found`），已统一迁到常驻载体（`sh -c "…; read go"` / 常驻 `sed`），细节与理由见票 04 Comments 第 3 条。两处对本票记录的修正：

1. **变异 C-②（`pty_is_running` 去掉 `!output_terminated()`）的证据易主**：原先靠 `is_running_true_while_alive_and_false_after_natural_exit` 翻红，摘除语义把「EOF 后、摘环前」压成毫秒窗口，端到端已不可确定性命中。现由纯函数 `running_verdict` 的四格真值表用例 `is_running_verdict_covers_all_four_states` 锁住（票 04 的 M5 实证杀死）。本票其余两条变异（去掉准入上限、`>`→`>=` 边界）证据不变。
2. **本票「未覆盖风险 1」已闭合**：句柄被摘除后 `is-running` 回 `pty handle not found`（而非 false）现由 `is_running_true_while_alive_and_handle_retired_after_natural_exit`（宿主）+ `test_pty_exit_event_and_purge_roundtrip`（e2e，kill 后经 WIT 边界验 `not found`）双向覆盖。
