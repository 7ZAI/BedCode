# 06 — 插件 host_http_fetch 接入 Egress 校验（wasm_host + host_impl/http.rs）

**What to build:** 插件网络原语 `network:http` 接入与宿主 `http_request` 同一 Egress 校验（D9：本分支一并接入）：`src-tauri/src/plugin/wasm_runtime/host_impl/http.rs` 的 `http_fetch` 与 `src-tauri/src/plugin/wasm_host.rs` 的 `execute_http_request` / `execute_streaming_http`（163/236 行起）在发出前调 egress 判定（ticket 02）。被拒 → 插件侧错误（对应 `EXTERNAL_URL_NOT_DECLARED` / `EXTERNAL_URL_DENIED` 语义），请求不发。

**Spec:** §5.6 机制要点 6、§9 D9、spec §4 插件段

**Blocked by:** 02, 05

**Status:** done

## 关键实现事实（handoff §2/§3 已核实）

- `host_impl/http.rs` `http_fetch` 只查 `granted_permissions` 含 `network:http`，**无 URL 粒度**；本 ticket 补 URL 级 Egress 校验（L1/L2/L3 与宿主一致，含授权弹窗桥）。
- `wasm_host.rs` `execute_http_request` / `execute_streaming_http` 由 reqwest 发出，无 Egress；流式分支（SSE/流式响应）同接入。
- 外网调用面：插件 ai-chatbox（`network:http`，预设 provider 域名 + 用户自定义 baseUrl → 自定义 URL 走 L3 弹窗）；其 manifest 的 `preauthUrls` 声明在 ticket 12 补。
- 插件 WASM 日志细节见 `docs/knowledge/logging.md`（target=`bedcode_lib::plugin::plugin_log`，`[plugin:xxx]` 前缀）；Egress 拒绝路径的日志同样遵守。

## 实现清单

- [x] `host_impl/http.rs` `http_fetch` 前置 egress 判定（URL 级，非仅权限位）
- [x] `wasm_host.rs` `execute_http_request` / `execute_streaming_http` 接入（含流式分支）
- [x] 被拒错误映射到插件可见错误（WASM guest 侧可处理、自报可处理错误不 panic）
- [x] 单测：声明 URL 放行 / 未声明拒绝（不发请求）/ 自定义 URL 走弹窗桥 / 流式分支同语义

## 验证

- `cargo test`（src-tauri）插件网络原语 egress 用例全绿
- ai-chatbox 链路（插件请求 → wasm_host → egress）与宿主代理语义一致

## Comments

- 2026-09-11 完成：`host_impl/http.rs` `http_fetch` 在 `network:http` 权限检查后统一加 `check_egress(state, request)`（流式/非流式共用，URL 级判定）：Allow（L1/L2/L3 记忆）放行；Deny → 插件可见错误含错误码（`EXTERNAL_URL_NOT_DECLARED`/`EXTERNAL_URL_DENIED`）；NeedConsent → 弹窗桥（uuid 事务 id，`app_handle` 经 `block_in_place + block_on` 与既有模式一致，30s 超时视为拒绝）。
- 单测 5 例（`host_impl/http.rs` tests）：L2 插件声明放行 / L1 桌面端目标放行 / 未声明+无 app_handle fail-closed / 非法 URL Deny / 缺 url 拒绝。
- 验证：host_impl 5 测试全绿 + 全量 297 passed + clippy 无新警告。
- 说明：wasm_host.rs 的 execute_http_request/execute_streaming_http 不单独加校验——egress 判定收敛在 host_impl 入口（唯一调用路径），避免双校验与弹窗重复。
