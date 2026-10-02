# 票 02 — HTTP 代理面 http_request（Egress + JWT 注入 + 链路加密信封）跨端覆盖

**状态**：resolved · 2026-10-01
**类型**：task
**依赖**：spec.md §5.2（前置工程 cross-end-tests 已交付）

## 落点

新增 `cross-end-tests/tests/http_proxy_flow.rs`（独立测试二进制）+ `common/desktop_ctx.rs`
两处装配增量：

- `enable_link_crypto_http()`：**headless 启动期链路加密装配**（`link_crypto::init_at_startup`
  建 Kd 身份 → 读 DB 配置 → `sync_registration`），与 GUI 的
  `composition::init_link_crypto_at_startup(app_handle)` 同款，只是数据目录由 rig 供给。
- `link_crypto_counters()`：桌面侧加密计数器**只读**快照（加密帧 / 解密失败 / 响应取钥失败）。

`Cargo.toml` 新增两个 **dev**-dep：`bedcode-server-core`（装配 + 只读计数器）、
`bedcode-server-websocket`（票 01 已加）。二者均不参与任何应答生成。

## 契约落表

| 契约 | 行为 | 场景 |
|---|---|---|
| P-001a | 未声明 host:port + `kind="desktop"` → `AppError::Egress(EXTERNAL_URL_NOT_DECLARED)`，**不发请求** | 反例 |
| P-001b | `https://` + `kind="desktop"` → 拒（局域网明文端口，不得借 desktop 逃逸外网） | 反例 |
| P-001c | 未声明 + 缺省 `kind="external"` + `app=None` → L3 需授权而无 UI → fail-closed 拒 | 反例 |
| P-001d | 经 `egress_declare_desktop_target`（前端 `setApiBaseUrl` 的生产入口）声明后 → 真往返 200 | 正例 |
| P-002a | 有效全局 token + `/api/sessions` → 200 + `data.sessions` 数组 | 正例 |
| P-002b | **无** token → 401 + 业务码 1007（没注入，不是「注入了被拒」） | 反例 |
| P-002c | 伪造 token → 401（注入了但认证中心拒） | 反例 |
| P-003a | pin 来自**真实认证链**（认证响应 `kdPublicB64`）；开主开关后经代理发**加密信封** POST → 桌面真实解密 → 响应密文被移动端真实解密；计数器 `encrypted_frames` **+2**、`decrypt_failures` **+0** | 正例（核心） |
| P-003b | `/api/auth/*` 白名单：请求与响应都**明文**（计数器 +0）且响应可解析为真实信封（防「一刀切加密」） | 正例 + 反例 |
| P-004a | `POST /api/sessions/{id}/resize`（**移动端无 Rust 客户端**，只能经代理面）真实往返 + 计数器 +2 | 正例 |
| P-004b | `/api/configs`（含播种配置名逐字断言）、`/api/quick-actions`、`/api/plugin/<id>/task-queue/list?session_id=` | 正例 |
| P-004c | GET 面同样带协商头：桌面端加密响应（计数器 >0），移动端解开 | 正例 |
| P-005 | 黑洞监听器构造「在途」→ `http_cancel` 打断 → `REQUEST_CANCELED`；未知 `request_id` 幂等成功 | 正例 |

## 关键发现（改变了实现形态）

1. **桌面端加密过滤器对环回对端显式豁免**（`link_crypto::is_exempt`：hook 脚本 / 本机工具
   直连 REST 不加密）。rig 的服务器与客户端同主机，连 `127.0.0.1` **必然**命中豁免 →
   加密分支永不执行，P-003 会退化成「明文 200」恒真。
   **解法**：连本机自己的 LAN IP（内核 `ip route get <本机IP>` = `local … src <同IP>`），
   服务器看到的对端就不是环回。取不到非环回 IPv4 时**显性 panic**（不静默 skip）。
   新增 `mobile_ctx::set_target_at(host, port)` 支持显式主机。
2. **HTTP 级失败经代理面是 `Ok(HttpProxyResponse{status})`，不是 `Err`**：`Err` 只承载
   传输 / Egress 门禁失败；前端按 `code != 0` 归一化。P-002b/c 因此断言 `status == 401`
   + 业务码，而不是 `expect_err`。
3. **`init_at_startup` 对建身份失败只 error 日志 + 强制全关**（fail-safe，不 panic）——
   首版 rig 忘了建父目录，装配静默降级为「全关但无报错」。故装配函数自带
   `identity_parts().is_some()` 断言，否则加密用例会假绿。

## 验证

- `cargo test --test http_proxy_flow` 绿（0.5s，**连跑 4 次稳定**）
- `cross-end-tests` 全量 **10 个二进制全绿**（既有 8 + 票 01 无回归）
- 变异自检 2 项：
  - M1「AAD 不绑路由」（移动端 `http_aad(Inbound, "/")`）→ 桌面端
    `AES-256-GCM 解密失败` → 400，P-003a 打红（证明 AAD 路由绑定真的跨端生效）；
  - M2「不注入 JWT」（`should_inject_jwt` 恒 false）→ P-002a 打红（401 ≠ 200）。
- 变异全部逐字回滚（`git diff` 两端生产代码为空）
- `cargo clippy --tests`：本包 **0 诊断**，exit 0（14 条 warning 全在未改动的既有 crate）
- 收尾无残留进程 / 端口 / `/tmp` 目录（`TempDirGuard` 覆盖 panic 路径）