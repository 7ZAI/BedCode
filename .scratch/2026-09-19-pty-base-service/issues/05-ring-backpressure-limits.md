# 05: 背压与限额——ring 淘汰 / truncated-resync / 上限常量

**What to build:** 把「慢插件只损失自己」这条背压契约做实：输出持续生产时环形缓冲淘汰最旧数据，落后于环起点的消费者拿到当前可读最早段并带 `truncated: true`，插件据此按 resync 语义重建上下文；PTY 读取端在任何消费者状态下零暂停、零感知。同时把资源上限变成显式、fail-visible 的常量。

**Blocked by:** 02（PtyRing 主干与拉取路径就绪）

**Status:** done（2026-09-19；限额四项 + 条目上限 + 单次读截断专项用例落地，**环容量改为插件声明参数**（用户裁决），宿主上下限仲裁；宿主 5 例 + 环 2 例 + SDK 1 例 + e2e 1 条，桌面 lib 997 全绿、集成 8 目标绿（唯一红为认证中心线 jwt 错误日志），变异 N1-N4 全杀死）

## 已定案的行为（spec D3/D9）

- **单生产者环**：生产端只有 PTY 读取任务；消费者是各插件的游标拉取。满则淘汰最旧，**绝不阻塞/降级生产端**（2026-09-17 拉取订阅者重构的动因，禁止回退成 push 或把背压踢回源）。
- **truncated / resync**：`from-offset` 落后于环起点 → 返回可读最早段 + `truncated: true` + 正确的 next-offset；契约文档明确「检测到缺口即重建上下文」，不做静默补齐。
- **限额常量**（四项，命名对齐既有插件侧常量风格）：每插件在册 PTY 上限、每 PTY ring 容量（起步取业务会话环容量同量级现役值）、单次 ring-fetch 最大返回字节（限制单次 wasm 边界拷贝）、单次 write 上限。
- **超限行为分级**：创建超限 → Err（fail-visible，不静默降级、不排队）；ring 满 → 淘汰 + truncated（数据面正常语义）；单次读/写超限 → Err。
- **不做**：跨插件共享 ring、ring 与业务会话输出环的代码级抽取合并（自持实现；抽取候选记进票 07 的 ADR 注记）。

## 验收

- [x] 大输出 + 小 ring 容量场景：落后消费者得到 `truncated: true` 且 next-offset 可续拉，无数据错序
- [x] 慢消费者（长间隔拉取/从不拉取）不影响输出生产：源侧无阻塞、无丢帧、无背压回传的可观测迹象
- [x] 每插件 PTY 数量达上限后 spawn 返回明确错误；另一插件不受该配额影响
- [x] 单次 ring-fetch 超 `max-bytes`、单次 write 超上限均被拒（不截断不静默）——**读侧按「截断 + 续拉」执行，见「与票面的偏离」第 2 条**
- [x] 四项常量集中在插件侧常量模块并有注释说明取值依据；ring 容量与业务会话现役值同源——**改为「宿主默认 + 插件声明 + 宿主上下限」，偏离登记见第 1 条**
- [x] ring 语义单测 + fixture 断言全绿；桌面 `cargo test` 全绿；测试后无 PTY 残留

## Comments

### 2026-09-19 实施落地

**用户裁决（本票关键设计点）**：票面/spec D9 要求「ring 容量起步对齐业务会话环容量同量级、与现役值同源」。实测业务现役值是 `channels.global_queue_max_bytes` = **50 MB 每条会话队列**，而插件环随 pty 句柄存活、每插件可到 8 条 → 字面对齐即单插件最坏 400 MB 常驻。就此征询时用户裁决：**「应该提供参数，供插件或者别的业务设置这个 ring 容量值」**。故本票不做常量档位之争，改为**声明式容量**：

- spawn config 新增 `ringBytes?`（camelCase，纯引擎参数，零业务语义，符合 ADR 0022 裁剪线）；
- 省略 → 宿主默认 `PLUGIN_PTY_RING_BYTES`（256 KiB）；
- `0` 或 `> PLUGIN_PTY_RING_MAX_BYTES`（4 MiB）→ `Err`，**不夹取到上限**（静默降级会让插件按自己声明的深度规划上下文、实际却少得多，与 D9「配额类失败必须可见」同分级）；单插件常驻上界因此是 `8 × 4 MiB = 32 MiB`，由配额常量表达而非环容量常量硬扛；
- 宿主内部（`PtyRingSink::paired_with_limits`）本就是参数入口，业务线要接同一条路时改传参即可，不需再动 `PtyRing`。

**改动文件**

- `pty/pty_ring.rs`：`PtyRing` 增 `max_chunks`（条目上限，碎块防御——票 02 遗留风险第 1 条）+ `with_limits` / `paired_with_limits` / `chunk_count()`；淘汰条件扩为「字节超容量 **或** 条目达上限」，仍均摊 O(1)、源侧零等待。
- `system/constants/plugin.rs`：补齐 `PLUGIN_PTY_MAX_SESSIONS_PER_PLUGIN`（8）、`PLUGIN_PTY_RING_MAX_BYTES`（4 MiB），并把 `PLUGIN_PTY_RING_BYTES` 的语义改注为「**默认**容量」；四项常量各自带取值依据注释（ws 同档资源画像 / 32 MiB 常驻上界推导 / canonical 分块节奏 / 单次边界拷贝）。
- `host_impl/pty.rs`：`SpawnConfig.ring_bytes` + `resolve_ring_bytes`（上下限仲裁）+ 配额判定**前置于开 fd / 起进程之前**（失败零副作用）；`registered_count_for` 从测试脚手架升为生产判据（属主维度计数）；`pty_is_running` 的判据组合在票 04 已抽成 `running_verdict`。
- WIT `host-pty` 的 `spawn` 文档：config-json 字段表补 `ringBytes?` 与「超限一律 Err」的失败分级。**零 ABI 变更**（config-json 是字符串参数内的 JSON 字段增量，老宿主 `SpawnConfig` 无 `deny_unknown_fields` → 未知字段自然忽略，符合 AGENTS §9 的增量原则）。
- SDK `host/pty.rs`：`PtySpawnConfig::ring_bytes()` 组装助手 + 模块文档补「容量是插件声明面 / 宿主不夹取 / 单次拉取按上限截断、余下续拉」；`pty_spawn` trait 文档补配额失败可见性。
- fixture `plugin-pty-test`：`pty-spawn` 命令透传 `ringBytes`。

**行为契约（新增 8 例，逐条对测试）**

| 契约 | 来源 | 规则 | 测试 |
| --- | --- | --- | --- |
| C-201 | spec D9 | 每插件在册数达上限 → `Err`（错误带上限常量）、零副作用、不淘汰既有句柄；配额按属主独立；句柄回收后额度归还 | `pty_quota_is_per_plugin_and_rejects_overflow_without_side_effects` |
| C-202 | 用户裁决 | `ringBytes` 为 0 / 超上限一律拒绝且**不静默夹取**；恰等于上限放行（off-by-one 另一侧） | `declared_ring_bytes_out_of_range_is_rejected_without_side_effects` |
| C-203 | spec D3 | 小环 + 全程不拉取的消费者：产出偏移持续推进（源侧零等待），落后游标 → `truncated` + 返回段从驻留起点起 + 续拉不再报缺口 | `small_declared_ring_evicts_for_a_never_fetching_consumer_without_stalling_output` |
| C-204 | spec D9 | 单次 `ring-fetch` **正好**被截到 16 KiB（数据面截断，非报错），逐次续拉可拿全量且**逐字节等于**进程产出、游标严格单调 | `ring_fetch_is_capped_per_call_and_resumes_to_the_end` |
| C-205 | 票 02 遗留 | 条目上限：字节有余量时块数仍被钳住，缺口同样以 `truncated` 如实上报 | `chunk_count_cap_evicts_even_when_bytes_fit` |
| C-206 | 量级关系 | 正常块尺寸下字节容量先触发淘汰，条目维度不参与（不误伤历史深度） | `byte_cap_evicts_before_chunk_cap_at_normal_chunk_sizes` |
| C-207 | SDK | `ringBytes` 以 camelCase 序列化；未声明即省略字段（宿主取默认，而不是传 0 被判错） | `spawn_config_json_serializes_declared_ring_capacity_in_camel_case` |
| C-208 | 最高 seam | 声明超上限经 WIT 回 `Err`；512 字节小环的缺口经 WIT 如实送达（`truncated` / `nextOffset` 自洽） | `test_pty_declared_ring_backpressure_roundtrip` |

**实跑证据**

- `cargo test --offline --lib -- pty` → **133 passed / 0 failed**（引擎 + 宿主 + 4 条 e2e 一次过滤全绿）
- `cargo test --offline --lib` → 本票落地时 **997 passed / 0 failed**；票 06 收口后同基线为 **999 passed / 0 failed**（总数随认证中心线在途用例增减，非本票引入）
- `cargo test --offline --tests --no-fail-fast` → lib 997 绿 + 8 集成目标绿，唯一红 `broadcast_shutdown`＝认证线 `utils/auth/jwt.rs:43` 的 error 级日志（票 02/04 已登记同一项，非本票）
- SDK：`cargo test` **85 passed**；`cargo check --features wasm --target wasm32-unknown-unknown`（stable）与 `--target wasm32-wasip3`（nightly-2026-09-16）双通过
- `rustfmt --edition 2021 --check` 对 `host_impl/pty.rs` / `pty_ring.rs` / `constants/plugin.rs` 0 diff（逐文件格式化，未跑全仓 `cargo fmt`，避免重排对侧在途文件）；`cargo clippy --offline --tests` 对这三个文件 0 告警
- 测试后进程核查：无 `sh -c read/echo/stty/while` 载体残留、无 `cat`/`sed`、无 `pty-reaper`、无测试遗留监听端口（AGENTS §3）

**变异自检（实跑后全部还原）**

| 变异 | 结果 |
| --- | --- |
| N1 配额判定阈值改成不可能命中（`> LIMIT*10`） | `pty_quota_is_per_plugin_*` FAILED（第 9 条被放行）✅ 杀死 |
| N2 上限阈值翻倍（`MAX+1` 被静默夹取） | 宿主 `declared_ring_bytes_out_of_range_*` FAILED **且** e2e `test_pty_declared_ring_backpressure_roundtrip` FAILED（拿到成功载荷而非 error）✅ 双杀 |
| N3 `pty_ring_fetch` 去掉 `min(FETCH_MAX)`（不设单次拷贝上限） | `ring_fetch_is_capped_per_call_*` FAILED（首批返回全量、轮次不为 3）✅ 杀死 |
| N4 `PtyRing::push` 去掉条目上限条件 | `chunk_count_cap_evicts_even_when_bytes_fit` FAILED；`byte_cap_evicts_before_chunk_cap_*` 仍绿（该用例正是验「字节维度先触发」，无串扰）✅ 杀死 |

**并发与抖动（登记，不修对侧）**

1. `broadcast_shutdown` 的 error 级日志（`utils/auth/jwt.rs:43` secret-store 回退）＝认证中心线在途项，与票 02/04 登记同因。
2. 本轮一次全量 lib 跑出现 `pty::pty_process::tests::hold_policy_keeps_natural_exit_unobserved_until_killed` 单例翻红（票 01 的业务 `Hold` 回归锁），当时认证中心线正在同一 worktree 并发跑 cargo（`wasm_runtime.rs` 09:50 仍在被写入、编译中途还有瞬时 error）。随后 3 次全量 + 隔离单跑共 4 次均绿，未复现。**这正是票 01 自己点名的未覆盖风险 ③**（「`hold_policy_*` 用 500ms 负向窗口……仍是一个时间型断言」），非本票引入的语义变化；该断言在 CPU 争用下敏感，候选成因是读线程把 `read` 的 `Interrupted` 当读错误退出并置 `reader_closed`（`pty_reader.rs` 未对 EINTR 重试）。**本票不擅自改票 01 的引擎文件**；若后续再翻红，正确修法是①读循环对 `ErrorKind::Interrupted` 重试，②把该断言改成事件驱动判据（kill 前显式 poll 一次无事件），二者都属票 01/07 的范围（已随本条登记，票 07 记入后续工程建议）。

**与票面的偏离**

1. **ring 容量「与业务现役值同源」→ 改为声明式参数**（用户裁决，见上）。业务值 50 MB/会话这一档对「每插件多条、随句柄存活」的插件环不成立；同源诉求由 `PtyRingSink::paired_with_limits` 的参数入口满足（业务线若要接同一条路，传自己的现役值即可）。票 07 的 ADR 注记需同时登记此项。
2. **票面验收「单次 ring-fetch 超 max-bytes 均被拒」按 spec D7/D3 执行 = 截断而非拒绝**：读侧是数据面，`max-bytes` 是「本次最多要多少」，宿主截到上限并回带 `next-offset` 续拉才是既定契约（票 02 的 C-004 已定稿并被本票 C-204 强化到「正好等于上限 + 逐字节全等」）。真正「被拒不截断」的是**写侧**（`PLUGIN_PTY_MAX_WRITE_BYTES`，票 03）。票面这一格按读/写两侧分别勾并记此偏差。
3. 条目上限（`PtyRing::DEFAULT_MAX_CHUNKS`）留在环模块而非插件常量模块：它是环实现自身的防御维度，且被 `PtyRing::new` 的默认路径共用，放常量模块会与环内默认值重复两份。票面「四项常量集中」按环容量默认/上限、拉取上限、写入上限、每插件条数计，仍全部集中在 `system/constants/plugin.rs`。

**未覆盖风险（交票 06 / 票 07）**

- **配额与容量的高 seam 组合**未做「A 打满配额后 B 仍能建满自己的 8 条」这类端到端矩阵（宿主层已验属主独立，e2e 层未验），归票 06 的隔离矩阵。
- **极小块风暴的真实进程载体**未端到端复现（条目上限在环层用 8 字节块直接验），真实 `read` 尺寸分布依赖内核与 `terminal.read_buffer_size`。
- **非 Linux 未验证**：本票 4 例真 PTY 用例 `#[cfg(target_os = "linux")]` 门控（`stty -echo -onlcr` 是 POSIX 终端语义）；ConPTY 下 `ringBytes` 生效性与截断语义待发布前实机验证（同票 01/02/03 注记，收口票 07）。
- **载荷同步点经验**：以 `cat` 做字节级比对时必须等 `stty` 生效（本票用 `echo STTY_READY` 作为同步点），否则前半段被 tty 回显并做 `\n → \r\n` 改写——票 06 若复用该载体需知。
