# 票 05 · 含逻辑域收窄 / 下沉（fs / db / http）——实施记录（2026-10-10）

> 输入：`spec.md` §4 票 05 + `ticket-01-domains.md` 域分类（含域逻辑 = peer / fs / db / http）。
> 纪律：**共有机制域不搬壳位置（D2）**——三个域的 WIT 壳仍留 `runtime/bare/host_impl/`，
> 只把**域逻辑**移到共享 crate；平台差异经端口注入。WIT / ABI / SDK / wire / 权限位零变更。

## 0. 结论表（每域落点）

| 域 | 票 01 分类 | 落点裁决 | 本次动作 |
| --- | --- | --- | --- |
| `peer` | ②+④ | **已由票 03 完成** | 无（票 03 已下沉 `bedcode-server-peer-net`，19 薄转发） |
| `http` | ②+④ | **下沉能力域 crate `bedcode-server-http`**（与 peer 同款手法） | 执行本体迁 crate；`host_api/http_engine.rs` 退役；移动 adapter 只接端口 |
| `fs` | ②+④ | **上移共享核 `bedcode-host-api-core::fs`**（新模块 + 端口） | 6 原语机制面上移；移动 adapter 收缩为端口接线 + 薄转发；Android 删除桥经端口注入 |
| `db` | ②+④ | **已在目标形态，零改动**（机制语义单点 `core_db`；本端只留句柄解析） | 仅登记判据（见 §3） |

## 1. http：执行本体下沉能力域 crate（D1 手法）

### 1.1 动作

- 新增端口能力 `HttpPorts::jwt_auth_token`（**带默认实现 `Ok(None)`** = 本宿主无「全局
  token」概念 ⇒ 桌面侧行为零变化；移动侧覆写为读宿主全局态 + 缺 token 即 `Err`）。
- `plugin_binding::egress::execute_http_request` 增 `bearer: Option<String>` 参数（由
  `http_fetch` 经端口取，`"jwtAuth": true` 才取），请求头注入点紧接 guest 自带头之后。
- 退役 `packages/bedcode-wasm-core/src/host_api/http_engine.rs`（498 行；执行 / SSE /
  流式 / 字节上限全体与 crate 逐字同源）。
- `runtime/bare/host_impl/http.rs`（188 → 约 200 行，含测试）重写为 port adapter +
  薄转发：快照 `granted_permissions` / `ports` / `app_handle`（能力域端口契约带
  `'static`，故**持有句柄副本**而非借 state），egress 三态结论经 `authorize_outbound`。
- 移动宿主 `plugin.rs` 的 `wasm_host` 垫片删除 http 侧 glob re-export（防「看起来能用
  的假入口」）；`Cargo.toml` 的 `bare-host` feature 增 `dep:bedcode-server-http`。

### 1.2 行为对表（移动旧实现 → crate）

| 面 | 旧（`http_engine` + `host_impl/http.rs`） | 新（crate） | 判定 |
| --- | --- | --- | --- |
| 权限门 | `permission denied: network:http` | `http error: permission denied: plugin '<id>' does not declare network:http` | 判定不变；文案细化（仍含 `permission denied`） |
| 出站授权拒绝 | `egress denied: …`（原样透出） | `http error: network authorization denied (<reason>): plugin '<id>' -> <origin>` | 判定不变；**新增归一化 origin**（不带 path / query，AGENTS §8） |
| 无头上下文 | `egress denied: app_handle unavailable (headless/test)` | 同文案经 `CheckFailed` 包装（fail-closed 不变） | 判定不变（用例保真：两条子串都在） |
| 非流式失败前缀 | `HTTP request failed: …` | `http error: …` | 文案统一（能力域字眼） |
| 缺 `url` | 入口入参校验 `missing 'url'` | 执行层 `Missing 'url' in HTTP request` | 判定不变；用例改按小写子串断言 |
| jwtAuth 代注 | `resolve_jwt_auth_header` 三向 | 端口 `jwt_auth_token` 三向（crate 用例覆盖） | 语义逐字保留（无 token ⇒ `Err`，**禁止**降级匿名） |
| SSE 格式解析 | `sseFormat == "openai"` 专用解析 | 仅 raw `data:` 行透传 | **双端 union app 均用 `sseFormat: ''`**（grep 证据：ai-chatbox 两端），原分支为无消费者死代码 |
| 响应字节上限 / 流式非 2xx / streamId 注册 | 各自实现 | crate 同值同形（32 MB / 事件 error+done / 入口返回 streamId+streamEvent） | 逐条对齐 |

### 1.3 覆盖保全（R6）

`http_engine` 自带 5 个用例，逐条对照后**无净损失**：

| 旧用例 | 新归属 |
| --- | --- |
| `http_fetch_small_response_ok` | crate 既有小/大响应用例 |
| `http_fetch_oversized_response_rejected` | crate 既有越限用例 |
| `http_fetch_without_jwt_auth_sends_no_authorization` | **新补** crate `jwt_auth_off_sends_no_authorization`（回显头真验） |
| `resolve_jwt_auth_header_three_way` | **新补** crate 三例（注入 / 未开不注入 / 无 token 拒且不触网） |
| `http_fetch_jwt_auth_without_token_fails_visibly` | 同上第三例 |

集成面：`bedcode-mobile/src-tauri/tests/session_http_flow.rs` 由「自持执行器」改为
**测试就地端口 impl 驱动 crate 真执行器**（token 仍取 `HostPorts::global_token` 与生产
同源；`#[tokio::test]` 改 `multi_thread` —— 能力域走同步桥，current_thread 上 `block_in_place`
会 panic）。四场景（list 的 wire + Bearer / 五命令请求形状 / 业务码信封 / 非 2xx 透出）未缩水。

锁面同步：`retired_mobile_session_control_face_lock.rs` 的 `KEEP_HOST_FACE` 由 2 条扩到
3 条（能力域开关点 `.get("jwtAuth")` + 端口契约 `fn jwt_auth_token` + 移动 adapter 的
缺 token 拒绝文案），旧 `fn resolve_jwt_auth_header(` 针脚随之消解。

## 2. fs：机制面上移共享核（新 `bedcode-host-api-core::fs`）

### 2.1 抽取判据（为什么这一段可以抽）

6 原语的机制面双端逐字相同：① 权限门（`fs:read` / `fs:write`）② 访问裁决
（`copy` 双路径、`request-auth` 批量）③ 路径形状守卫（写自动建父目录、删除幂等）。
**平台差只剩一条**：删除在 Android 走 MediaStore/SAF 通道 ⇒ 经
`FsHostPorts::platform_delete` 注入（`None` = 标准 `std::fs`），共享核不认平台名词。

### 2.2 落点与形状

- 新模块 `packages/bedcode-host-api-core/src/fs.rs`：`FsOp` 词汇 + `FsHostPorts`
  （`check_permission` / `check_access` / `check_access_batch` / `platform_delete`）
  + 6 原语函数（`PermissionGate` 入参，拒载文案由各端传入）+ 8 个单测。
- 移动 adapter `runtime/bare/host_impl/fs.rs`（216 → 约 190 行含头注）：`StateFsPorts<'_>`
  接 `WasmHostContext::{granted_permissions, fs_auth, ports}`，6 条薄转发。
- **守卫形态**：`guarded_host_call` 由「只包访问裁决」改为「包整条调用」（文件操作同样
  会 panic）；fallback 文案沿用迁移前的拒绝文案（本次迁移只承诺行为零变化，不顺手改
  panic 语义）。
- **桌面侧不在本次范围**：桌面 fs 实现体（`host_api/fs.rs`，含 WSL 桥 / 任务单元两个
  消费者）保持原样；共享核的成立不依赖桌面改用，后续是否迁移另票（已在该 crate 头注
  与 lib.rs 域表登记）。

### 2.3 覆盖保全

原 `fs.rs` 无单测 ⇒ 新模块补齐 **7 例**（缺失文件 = `Ok(None)` / 写读往返 + 自动建父
目录 / 权限门先于访问闸门 / 拒载文案稳定 / copy 双权限 + 双路径 / request-auth 空表与
批量「全部同意」+ 非法 JSON / 删除走平台通道 + 幂等）。

## 3. db：判定已在目标形态（零改动）

证据：

1. 机制语义（权限门 / 语句超时护栏 / 结果集护栏 / 批量事务护栏 / 参数绑定 / 行转换）
   **全部**在 `bedcode-host-api-core::database`（双端单点）——移动 `db.rs` 与桌面
   `host_api/database.rs` 都只做「权限门 + 调 core_db」。
2. 本端剩余物 = **私有库句柄解析**（`app_handle` / `ports.app_data_dir` /
   `WasmHostContext.plugin_dbs` 连接表缓存）：这是「本端存储落点 + 句柄生命周期」的
   宿主事实（两端进程不同、路径不同、缓存各持一份是必然），符合共享 crate 的
   「不得引用端类型」判据（`crate::db::Database` 属 wasm-core 机制层）。
3. 若强行下沉，只能在 host-api-core 里为「数据库句柄 + 平台路径」再造一层泛型端口，
   收益为零、抽象成本高 ⇒ 不做。

## 4. 验证状态（越界说明：票内跑了编译排障）

用户指令⑦要求「全部票改完后统一验证」。本次在票 05 开工前**先跑了一轮 `cargo check`
排障**（理由：票 02–04 的改动尚未过任何编译器，继续叠变更会让收尾一次性爆栈无法定位；
属 spec §5「排障例外」，不作票级通过判据）：

| 目标 | 结果 |
| --- | --- |
| `packages/bedcode-wasm-core`（`--no-default-features --features bare-host`） | 0 error（修 3 处票 02–04 机械伤：`bedcode_host_kit::limits::defaults::*` 路径、`self.host_ctx(state)`→`host_ctx(self.state)`、`block_on_async` 单参形态） |
| `bedcode-mobile/src-tauri`（`cargo check --lib`） | 0 error |
| `bedcode-desktop/src-tauri`（`cargo check --lib`） | 0 error |
| 票 05 后 `bedcode-mobile/src-tauri`（`cargo check --lib --tests`） | 0 error（含 `session_http_flow` 重写） |
| `packages/bedcode-server-http`（`cargo check --lib --tests`） | 0 error |

**未跑**：全量 `cargo test`、前端 `pnpm run test:run`、`cross-end-tests`、锁变异自检
——统一归票 06（磁盘 99% 曾中断一次 `cargo test`，已 `cargo clean` 释放 5.6 GiB；
收尾仍需预留空间）。
