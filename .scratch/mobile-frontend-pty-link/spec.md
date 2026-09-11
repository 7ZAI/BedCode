# Spec：移动端前端网络能力收束 — 通用 HTTP 代理 IPC + 终端链路改造

> 状态：**已确认（2026-09-11 评审通过，D1-D9 全部定稿，可开工）** · 分支：`feature/mobile-frontend-pty-link`（从 dev 迁出 2026-09-11）
> 范围：bedcode-mobile（移动端前端 + Rust 后端）
> 关联：`docs/knowledge/mobile-desktop-auth.md`、`docs/knowledge/pty-output-pipeline.md`、`.scratch/pty-output-refactor/`（P0-P3 已归档）、`packages/link-crypto`
> 用户原话（2026-09-11）：移动端前端将只负责 UI 渲染，其他（HTTP / WebSocket）放到 Rust 来做；HTTP 调用通过 IPC 桥接（Rust 端代理）；只设计一个通用 HTTP 通信端口 IPC，利用 ID 确定调用者，用多路复用的思想实现 Rust 代理前端 HTTP；终端 WebSocket 搬迁后续再说。
> 调研更新（2026-09-11）：曾评估「保留 @tauri-apps/plugin-http」路径——插件 **2.5.9 = 官方 v2 最新，无任何拦截/中间件能力**（JS 仅导出 fetch，Rust 仅 5 个命令），JWT 注入/加密信封无法在插件层集中 → **确认走迁移，不再使用该插件，全部走自研设计**。其 rid 句柄 + 分步 invoke 的调用机制可借鉴，见 §5.5。

---

## 1. 背景与动机

移动端前端当前承担了超出「UI 渲染」的职责：

1. **HTTP 调用面**：`useHttpApi.ts` 用 `@tauri-apps/plugin-http`（tauriFetch）直发 HTTP；**JWT 注入、链路加密信封（X25519 + AES-GCM）、pinned key 管理与协商、错误处理、超时**全部在前端 JS。
2. **终端 WS 直连**：`useTerminalSocket.ts` 前端 `new WebSocket` 直连桌面端 `/ws/terminal/session/{id}` 取 TB v2 二进制帧（v2.1.0 已上线，属 pty-output-refactor P2 形态）——**本分支不做，见 §7 边界**。
3. **前端网络逻辑双实现**：链路加密信封同时存在前端 `linkCrypto.ts`（@noble/curves 纯 JS 实现）与共享 crate `packages/link-crypto`（Rust）——前端实现属过渡态，应删除。
4. **tauri-plugin-http 无拦截能力**（调研 2026-09-11）：JS 侧仅导出 `fetch`，Rust 侧仅 `init()` + 5 个命令（fetch/fetch_cancel/fetch_send/fetch_read_body/fetch_cancel_body），无 onRequest/middleware/hook 钩子点；官方仓库无相关 roadmap。JWT 注入、加密信封、超时/取消、日志**无法在插件层集中**，只能自研代理收束（细节见 §5.5）。

目标：**移动端前端只负责 UI 渲染**。所有 HTTP（宿主前端 + 插件 WASM）经统一 IPC 桥接由 Rust 端代理发出；用 request_id 标识调用者、多路复用响应路由；链路加密、JWT 注入、超时/取消、日志、**外网访问声明/授权（Egress Policy）**全部收束到 Rust 端。

## 2. 现状（已核实）

| 项 | 现状 | 位置 |
| --- | --- | --- |
| HTTP 发出 | `tauriFetch`（tauri-plugin-http，Rust 实际发请求，但前端持 fetch 封装） | `src/composables/useHttpApi.ts` |
| JWT 注入 | 前端从 `useMobileConnection().authCredentials.sessionToken` 取，拼 `Authorization` 头 | 同上 |
| 链路加密信封 | 前端 Web Crypto（@noble/curves x25519 + AES-256-GCM），`X-BedCode-Crypto: v1 …` 协商头；fail-closed | `src/services/linkCrypto.ts`、`useLinkEncryption.ts` |
| pinned key | 前端持有（配对/认证响应里取），`set_link_crypto_context` invoke 到 Rust | `useLinkEncryption.ts` |
| 更新检查 | `useUpdateChecker.ts` 用 tauriFetch 直连 GitHub API | `src/composables/useUpdateChecker.ts` |
| Rust 命令面 | 无通用 HTTP 代理命令；已有 `set_link_crypto_context`、`ws_authenticate*` 等 | `src-tauri/src/commands/`、`lib.rs:200` |
| Rust crypto | 已依赖 `bedcode-link-crypto`（`src-tauri/Cargo.toml:105`） | `packages/link-crypto` |
| 事件/同步 WS | 已在 Rust（`connection/`），前端只收事件 | — |
| 终端 WS | 前端直连（本分支不做） | `useTerminalSocket.ts` |

## 3. 目标架构

```
移动端前端（仅 UI 渲染）
  └─ invoke('http_request', { request_id, method, url, headers, body, timeout_ms })
        │
        ▼
移动端 Rust（统一 HTTP 代理，单端口多路复用）
  ├─ Egress Policy 校验（§5.6）：桌面端目标放行 → 静态声明（宿主内置 + 插件 preauthUrls）
  │    → 未命中弹授权窗（用户确认/拒绝，fail-closed）
  ├─ 共享 reqwest Client（连接池复用）
  ├─ JWT 注入（/api/auth/* 除外）
  ├─ 链路加密信封（bedcode-link-crypto；非 auth 且已 pin 且开关开）
  ├─ 超时 / 取消（request_id → http_cancel）
  ├─ 结构化日志（request_id 字段）
  └─ 响应 ──► invoke promise 按调用者路由回前端（D1=A）

插件 WASM（ai-chatbox 等）
  └─ host_http_fetch / host 流式 HTTP（network:http 权限）
        └─ 同一 Egress Policy 校验（D9：本分支一并接入）
```

协议兼容约束：Rust 代理产出的 HTTP 请求/响应信封必须与前端现有实现**逐字节一致**（`X-BedCode-Crypto` 协商头、AAD、nonce 顺序、信封 JSON 结构），否则桌面端解密失败——见 §5 兼容性。

## 4. 文件级改动清单

### Rust（bedcode-mobile/src-tauri/）

| 文件 | 改动 |
| --- | --- |
| `src/commands/http_proxy.rs`（新） | `http_request(request_id, method, url, headers, body, timeout_ms)` 命令：Egress 校验（§5.6）、reqwest 发出、JWT 注入、加密信封、响应解码；共享 `reqwest::Client` 存 State；`http_cancel(request_id)` |
| `src/egress.rs`（新） | Egress Policy：L1 桌面端目标判定 / L2 静态声明（宿主内置 + 插件 preauthUrls）/ L3 授权弹窗桥（事件 → 前端渲染 → 结果回 Rust）/ 授权记忆（Rust 持久层，会话/持久粒度）；`EXTERNAL_URL_NOT_DECLARED` / `EXTERNAL_URL_DENIED` 错误码 |
| `src/commands.rs` / `lib.rs` | 注册命令 |
| `src/system/constants/` | 超时默认值、JWT 注入白名单（`/api/auth/*`）、宿主内置外网白名单（GitHub API） |
| `src/plugin/types.rs` | `PluginManifest` 新增 `preauthUrls: Vec<String>` 字段解析（glob host+path，仿 `preauthDirs`） |
| `src/plugin/wasm_runtime/host_impl/http.rs` | `http_fetch` 接入 Egress 校验（D9 确认：插件网络原语与宿主代理同一校验；流式分支同） |
| `src/plugin/wasm_host.rs` | `execute_http_request` / `execute_streaming_http` 前置 Egress 参数（或在校验层包装） |
| `Cargo.toml` | 复用已有 reqwest / bedcode-link-crypto；确认无新依赖或仅小量；**移除 `tauri-plugin-http = "2"`** |

### 前端（bedcode-mobile/src/）

| 文件 | 改动 |
| --- | --- |
| `composables/useHttpApi.ts` | `request()` 改为纯 invoke(`http_request`)，删除 tauriFetch / JWT 注入 / 加密信封逻辑；保留 ApiResult 形状与错误语义；`httpProbe` 一并走代理（/api/health，桌面端目标 L1 放行） |
| `services/linkCrypto.ts` | 删除（能力已入 Rust）——**见 §9 D2** |
| `composables/useLinkEncryption.ts` | 收缩为「开关/状态查询 + set_link_crypto_context」，删除加解密 |
| `composables/useUpdateChecker.ts` | 改走统一代理（GitHub API 经宿主内置声明放行） |
| `utils/frontendLogger.ts` 相关 | 保持 |
| `components/EgressConsentDialog.vue`（新） | 授权弹窗（域名/路径/来源展示 + 确认/拒绝 + 「不再询问」），i18n 双语言 |
| `plugin/`（manifest 加载器） | 解析/收集插件 `preauthUrls` 声明 → 注册进 Egress L2 |

### 插件（bedcode-mobile/plugins/）

| 文件 | 改动 |
| --- | --- |
| `ai-chatbox/plugin.json` | 新增 `preauthUrls` 声明（预设 provider 域名：openai/deepseek/qwen/anthropic/dashscope 等） |
| `plugin-sdk-mobile` | manifest 类型/模板同步 `preauthUrls` 字段 |

### 测试

- Rust：`http_proxy` 命令单测（mock 请求：JWT 注入、auth 白名单、加密信封字节一致性、错误映射、超时/取消）；`egress` 单测（L1/L2/L3 判定、记忆粒度、错误码）；`host_impl/http.rs` 接入后插件网络原语 Egress 单测
- 前端：`useHttpApi` 单测改 mock invoke（request_id 透传、响应路由、错误分支）；EgressConsentDialog 组件测试
- 双端联调（真机）：配对 → 文件树/会话/任务队列等 HTTP 路径全绿；加密信封与桌面端互通回归；自定义 URL 授权弹窗全流程

## 5. 兼容性要点（跨端协议，不可破坏）

1. **信封格式**：`X-BedCode-Crypto: v1 <eph_pub_b64>` + 信封体 JSON（密钥派生 IKM、AAD = 请求行、nonce 顺序）必须与桌面端 `server/…` 解密侧一致。以 `packages/link-crypto` 的 Rust 实现为准（桌面端已用），前端 JS 实现删除前用字节级单测对齐。
2. **HTTP API 形状**：`ApiResult<T> { code, message, data? }` 语义不变。
3. **老端兼容**：HTTP 代理是移动端本地 IPC，不涉及跨端字段演进；但请求出去的报文需与桌面端（master/uat 同版本）严格一致。
4. JWT 注入白名单与现有行为一致：`/api/auth/*` 不带 Bearer，其余带。
5. **报文逐项对齐（迁移后新增风险，§5.5 结论 2.3）**：对比现状 tauriFetch 报文，补齐 `Origin`（桌面端安全过滤可能依赖）、默认 `User-Agent`、POST/PUT 无 body 时 `Content-Length: 0`、Range 头时 `Accept-Encoding: identity`；用捕获报文 diff 单测锁住。

## 5.5 调研：tauri-plugin-http 调用机制与借鉴（2026-09-11）

调研对象：`@tauri-apps/plugin-http` 2.5.9（本机 node_modules + cargo registry 源码），已对照官方仓库 v2 分支最新版（一致）。

**结论 1：无拦截能力**（支持迁移决策）。JS 仅导出 `fetch`（invoke 包装，192 行）；Rust 侧 `lib.rs` 仅 `init()`（State 只含可选 cookies jar）+ `commands.rs` 5 命令，无钩子点；插件仓库相关 issue/PR 均为 proxy/no_proxy 配置类，无拦截 roadmap。

**结论 2：调用机制（可借鉴）**：

1. **rid 句柄 + 分步 invoke 多路复用**：`fetch` 命令把 `Box::pin(future)` 存进 `webview.resources_table()` 返回 rid；前端随后 `fetch_send`（响应头 + 新 rid）、`fetch_read_body`（body 分块，末块哨兵字节 1）、`fetch_cancel_body`（丢弃流）。→ **与 request_id 多路复用同构，验证 D1「invoke promise 模式」可行**；但插件资源由 webview 生命周期自动清理，我们自研需手动 remove（完成/取消/超时兜底），防 map 膨胀。
2. **取消 = oneshot + tokio::select!**：`FetchRequest` 持 `AbortSender/AbortReceiver`（oneshot），`fetch_send` 内 `tokio::select! { res = fut => …, _ = abort_rx => RequestCanceled }`。→ `http_cancel(request_id)` 照搬（或 reqwest `AbortHandle`，见 D5）。
3. **报文对齐（fetch spec 语义）**：POST/PUT 无 body 补 `Content-Length: 0`；Range 头补 `Accept-Encoding: identity`；默认 UA（`插件名/版本`）；自动 Origin（tauri://localhost 或 webview origin）；forbidden headers（Connection/Cookie/Host/Origin/Referer 等 + `proxy-*`/`sec-*` 前缀）丢弃。→ 迁移后逐项对比现状（§5.5 结论 2.3 / §5 第 5 条）。
4. **错误映射**：scope 拒绝 `UrlNotAllowed`、scheme 不支持 `SchemeNotSupport`、取消 `RequestCanceled`。→ 我们保持 ApiResult 形状（HTTP 错误 `code=status` / 网络错误 `code=-1`），取消语义对齐。
5. **State 最小化 vs 连接池**：插件每次请求新建 `reqwest::ClientBuilder`（不共享连接池，局限）；我们采用**共享 `reqwest::Client`（State 持有，连接池复用）**——自研代理相对插件的优势。

**结论 3：scope 白名单**（command_scope + global_scope 合并、Rust 端 `is_allowed` 仲裁）符合「权限仲裁在 Rust 端」；但我们的目标主机由连接模块管理且需支持公网 GitHub API（useUpdateChecker），**不做 URL scope**，改为命令层 method/url/headers 基础校验。

## 5.6 外网访问声明与授权（Egress Policy）——新增（2026-09-11）

用户要求：**合法的外网访问申请必须能走通；一切未声明的外部 HTTP 调用必须被拒绝（fail-closed）**。自定义 URL 走授权弹窗；插件声明内容中新增 URL 声明。

### 校验层（借鉴 fs_auth 三层：路径白名单 → 插件白名单 → 弹窗授权，AGENTS.md §7）

| 层 | 放行条件 | 来源 |
| --- | --- | --- |
| L1 桌面端目标 | 目标 host:port = 连接模块当前 baseUrl（配对/会话内） | 连接状态（无需声明） |
| L2 静态声明 | 命中声明 URL 模式 | ① 宿主内置白名单（useUpdateChecker 的 GitHub API）② 插件 manifest **新增 URL 声明字段**（仿 `preauthDirs` 先例，如 `preauthUrls: string[]`，插件加载时宿主收集） |
| L3 授权弹窗 | 未命中声明 → 首次请求弹窗，用户确认后放行 | 动态（可记忆，见下） |
| 拒绝 | 以上均未通过 → **fail-closed 拒绝，请求不发** | 错误码 `EXTERNAL_URL_NOT_DECLARED` / `EXTERNAL_URL_DENIED` |

### 机制要点

1. **裁决与记忆在 Rust 端**（§8 安全红线：权限仲裁在 Rust）：`http_request` 代理内嵌 Egress 校验；授权弹窗经事件到前端渲染（UI），结果回 Rust 裁决；授权记忆存 Rust 持久层（按 host 级 + 可选 path 模式），**不落 localStorage**。
2. **插件 URL 声明**：`plugin.json` 新增字段（如 `preauthUrls`），宿主 `PluginManifest`（`src-tauri/src/plugin/types.rs`）解析收集进 L2 白名单；SDK manifest 类型/模板同步；现有插件（ai-chatbox）预设 provider 域名迁移进声明。声明粒度建议 host + 可选 path 前缀（glob）。
3. **授权记忆粒度**：单次（本次请求）/ 会话（当前连接）/ 持久（存储，带「不再询问」勾选）——**见 §9 D7**（定稿：默认会话级 + 可选持久）。
4. **自定义 URL 场景**（ai-chatbox 用户手填 baseUrl）：L2 未命中 → L3 弹窗（展示域名/路径、来源插件/调用方）→ 确认放行。
5. **弹窗 UI**：新组件（借鉴 file-transfer ConsentDialog 模式：来源展示 + 确认/拒绝），i18n 双语言（zh-CN/en）。
6. **插件 host_http_fetch 接入**：插件网络原语（`network:http`，现无 URL 粒度）与宿主 `http_request` 共用同一 Egress 校验——**见 §9 D9**（定稿：本分支一并接入）。

## 6. 验收标准

1. 移动端前端源码零 `fetch(` / `@tauri-apps/plugin-http` 引用（useUpdateChecker 一并收束）。
2. `useHttpApi` 所有调用路径（文件树、会话模式、任务队列、SAF、biometric HTTP 绑定等）经统一代理全绿。
3. 链路加密：开开关 + 已 pin 时，请求经 Rust 信封化且桌面端可解密；GET/HEAD 无 body 仍带协商头（与现状一致）；加密失败 fail-closed。
4. request_id 出现在所有代理日志结构化字段；并发请求互不串扰（多路复用正确性单测）。
5. **Egress Policy**：桌面端目标全放行；useUpdateChecker（GitHub API）经 L2 放行；未声明外网 URL 被拒（不发请求）；自定义 URL 弹窗授权流程可用（确认放行 / 拒绝拒绝）；插件 `preauthUrls` 声明被宿主收集生效。
6. 全量：移动端 cargo test + vitest 绿；根目录 eslint 0 error。

## 7. 边界（本分支不做）

- **终端 WS 搬迁**：`useTerminalSocket` → Rust WS 客户端 + IPC 桥接，**后续分支**（用户明确"后续再说"）。本 spec §8 预留规划段落。
- 事件/同步 WS（`connection/`）已在 Rust，不动。
- 桌面端不改（代理是移动端本地行为；跨端报文格式不变）。
- peer-net 链路加密（对等网络直连）不在本任务范围。

## 8. 后续规划（预留）

- 终端 WS 收束：移动端 Rust 新增 ws_client（已有 connection/ 基础）+ 通用 WS IPC（request_id 多路复用同思路）→ 前端 `useTerminalSocket` 改为纯 IPC 消费 TB v2 帧（arraybuffer 经 invoke 返回或事件推送）。
- 前端 `useMobileConnection` 中若残留网络直连逻辑一并收束。

## 9. 决策记录（2026-09-11 全部确认，用户拍板）

| # | 决策 | 定稿 |
| --- | --- | --- |
| D1 | 响应路由模式 | **A) invoke promise**（request_id 作日志/追踪/取消标识；插件 rid 机制已验证） |
| D2 | 加密信封收束时机 | **本分支一并删除 `linkCrypto.ts`，信封进 Rust**（前端只渲染） |
| D3 | request_id 归属 | **前端生成**（UUID，随 invoke 传，可关联业务上下文） |
| D4 | plugin-http 依赖 | **确认移除**（JS+Rust+caps 三处全删；不再使用，全部走自研设计） |
| D5 | 取消语义 | **需要 `http_cancel(request_id)`**（reqwest AbortHandle，借鉴插件 oneshot+select 模式）；默认超时 **30s 保留** |
| D6 | 插件 URL 声明字段 | **`preauthUrls: string[]`**（glob host+path，仿 `preauthDirs`）；宿主解析落点 `plugin/types.rs`；SDK manifest 类型/模板同步 |
| D7 | 授权记忆粒度 | **默认会话级 + 可选持久**（带「不再询问」勾选） |
| D8 | 弹窗触发时机 | **请求时懒触发** + 授权记录在设置页可查看/撤销 |
| D9 | 插件 host_http_fetch 接入 | **本分支一并接入 Egress**（改 `host_impl/http.rs` + `wasm_host.rs`；ai-chatbox manifest 补 `preauthUrls`） |
