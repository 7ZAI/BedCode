# 票 03 产出 · peer 同 crate 复用（19 原语行为对表）

> Date: 2026-10-10。范围：移动 `bare/host_impl/peer.rs`（510 行重复域逻辑 → 19 条薄转发）+
> 移动宿主新端口 `plugin/peer_ports.rs` + crate 侧 3 处对齐修正。
> 证据基准：`packages/bedcode-server-peer-net/src/plugin_binding.rs`（对表目标）、
> 旧移动实现（本票删除前 HEAD 版本）、`wit/core.wit` `host-peer` 注释。

## 1. 落点

| 面 | 文件 | 内容 |
| --- | --- | --- |
| 域逻辑（单点） | `packages/bedcode-server-peer-net/src/plugin_binding.rs` | 19 条 WIT-无关函数（无 feature 门控）+ 句柄表 / 属主仲裁 / 自动重拨 / 载荷校验 |
| 端口 | 同上 `plugin_binding/ports.rs` | `PeerPorts`（check_permission / peer_ctx / block_on_any）+ `install_ports` + `HEADLESS_UNAVAILABLE` |
| wasm-core bare 壳 | `src/manager/runtime/bare/host_impl/peer.rs` | 19 条薄转发（510 → 141 行）；`component.rs` 接线与 `host_impl.rs` re-export 零改动 |
| 移动宿主端口实现 | `bedcode-mobile/src-tauri/src/plugin/peer_ports.rs`（新） | 3 方法：权限门 = `PluginManager::has_permission`（async → wasm-core 唯一同步桥驱动）；引擎上下文 = `crate::peer_net::peer_ctx`（无头返回 crate 文案）；同步桥 = `runtime_util::block_on_async` |
| 装配点 | `bedcode-mobile/src-tauri/src/plugin/manager.rs`（init 段） | `peer_ports::install(app_handle)`（幂等，首装胜出） |
| 依赖 | `packages/bedcode-wasm-core/Cargo.toml` `bare-host` | + `dep:bedcode-server-peer-net`（crate 非 optional 段：纯引擎可被任意宿主引用） |

## 2. 19 原语逐条对表

| # | 原语 | 移动旧实现 vs crate | 结论 |
| --- | --- | --- | --- |
| 1 | dial_peer | 旧：`{nodeId,addr,port}` camelCase 解析 + take node_id；crate：`DialEndpoint`（`rename_all=camelCase`）clone 语义 | **等价**（crate 注释已记：移动 take 后重构造无 bug，两端口径统一为 clone） |
| 2 | close | 旧：session 摘除 → 发送取消 → 接收取消（**无属主校验**）；crate：同三段路由 + **属主判定与拒绝回滚** | crate 更严（安全增强：非属主探测不得摘他人连接）——**接受**（越权 close 由「成功」变「拒绝」） |
| 3 | respond_consent | 同形（权限 → 引擎 → bool） | ✓ |
| 4 | list_trusted | 同形（DTO JSON 序列化） | ✓ |
| 5 | revoke_trusted | 同形 | ✓ |
| 6 | send_files | 载荷双形态（string / `{path,encrypt?}`）+ `concurrency` 拒载 + auto_redial；**文案差异**：旧 `'concurrency' retired in ABI v12` vs crate `... in ABI v31` | **crate 文案去版本号**（`retired` 不再点名 ABI 序列——两端编号体系不同，单点实现不误导任一端）；测试断言同步 |
| 7 | respond_transfer | 同形（hit 忽略） | ✓ |
| 8 | set_receive_policy | 同形 | ✓ |
| 9 | pause_transfer | 旧：宿主适配层「发送批未命中 → **回落接收方向**（pull 会话 / push 接收批 Pause 帧）」；crate 旧：仅发送批，未命中 Err | **crate 内建回落**（移动历史组合语义单点化；两端同集 19 原语下「单一原语管双向」统一）；两向都未命中 → 原文案 Err |
| 10 | resume_transfer | 同上回落；旧 wasm-core 层 `hit=false → Err("no paused send batch")`；crate 旧：**无 hit 判定**（`Ok(false)` 静默） | **crate 补回落 + 补 hit Err**（文案与移动历史版逐字一致）——注：引擎 `resume` 无会话时直接 Err（回落分支仅在罕见 `Ok(false)` 可达），与移动现状同构 |
| 11 | set_shared_roots | 旧：透传端口，宿主解析 `{id,name,safTreeUri}`（SAF 树）；crate 旧：自解析 `{id,name,path}`（**仅桌面形态**） | **crate 补双形态解析**（`path`→Fs 根 / `safTreeUri`→Saf 根，均过；两字段都缺显性点名）；新增测试 2 例 |
| 12 | list_shared_roots | 同形（auto_redial + DTO JSON） | ✓ |
| 13 | browse_directory | 同形（auto_redial） | ✓ |
| 14 | pull_files | 同形（`{relPath,size}` camelCase；auto_redial；u32 返回） | ✓ |
| 15 | set_download_dir | 同形（空串 → None） | ✓ |
| 16 | start_node | 同形（属主记账 `start_node_owned(ctx, caller)`） | ✓ |
| 17 | stop_node | 同形（属主拒绝，文案不回带属主 id） | ✓ |
| 18 | active_transfers | 同形（三表聚合投影） | ✓ |
| 19 | collect_outgoing | 同形（`["path"]` 数组 → `[{path,size}]`；无 peer_ctx 依赖） | ✓ |

**wire 形状核查**（票 03-④）：
- `dial_peer` / `pull_files`：camelCase 与 WIT 注释一致（`DialEndpoint` / `RemotePullFileDto` 均 `rename_all=camelCase`）✓
- `set_shared_roots`：crate 补齐移动形态（WIT 注释原文「移动端 SAF 树以 `safTreeUri` 字段替代 `path`」）✓
- 无头文案 `HEADLESS_UNAVAILABLE`：crate 单点持有，移动端口实现引用（不在宿主另写一份）✓

## 3. 差异裁决汇总（3 处 crate 修改 + 1 处安全增强）

1. `peer_pause_transfer` / `peer_resume_transfer`：补「发送未命中 → 接收方向回落」（移动历史语义单点化）。
2. `peer_resume_transfer`：补 `Ok(false)` → Err（WIT「无命中返回错误」；文案与移动历史版逐字）。
3. `peer_set_shared_roots`：补 `safTreeUri` → `SharedDirRoot::Saf`（WIT 承诺的双形态；两字段都缺显性 Err）。
4. `peer_send_files` 拒载文案去版本号（`retired in ABI v31` → `retired`），测试断言同步。
5. （非 crate 改动）close 属主校验：移动切 crate 后获得属主判定（安全增强，接受）。

## 4. 测试面

- crate `payload_contract.rs`：concurrency 断言更新 + 新增 `set_shared_roots_accepts_both_root_shapes_past_validation` / `set_shared_roots_rejects_entry_without_root_field`。
- 移动宿主旧 peer 端口链（`HostEnginePorts` 的 peer 21 方法 + `peer_*.rs` 适配层）保留在场（无 in-kernel 消费者；清理归票 05/收尾评估，避免本票连锁改动）。
- 无头口径：移动端口 `app = None`（测试/无头）→ `peer_ctx` 返回 `HEADLESS_UNAVAILABLE`（对齐桌面测试口径）。
- crate 内测试不驱动 19 原语的引擎面（FakePorts 恒无头）——回落/引擎面行为由移动全量 + 真机冒烟覆盖（票 06 统一跑）。

## 5. 静态自检

- wasm-core 内 peer 旧符号（`SessionEntry` / `PeerHandleTable` / `PEER_HANDLES` / `with_auto_redial` / `require_peer_permission`）：grep 0 残留。
- `bare/host_impl.rs` re-export（`pub(crate) use peer::*;`）与 `component.rs` 接线（19 调用点签名）零改动。
- 端口三方法签名与 crate trait 逐字一致（`BoxedBlocked` / `HEADLESS_UNAVAILABLE` 引用 crate 单点）。
