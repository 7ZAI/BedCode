# 07 — 前端 useHttpApi 收束：request()/httpProbe 走统一代理

**What to build:** `src/composables/useHttpApi.ts` 的 `request()` 改为纯 `invoke('http_request', { request_id, method, url, headers, body, timeout_ms })`，**删除 tauriFetch / JWT 注入 / 加密信封逻辑**（已入 Rust）；保留 `ApiResult<T>` 形状与错误语义（HTTP 错误 `code=status` / 网络错误 `code=-1`）；前端生成 UUID request_id（D3，随 invoke 传，可关联业务上下文）；`httpProbe` 一并走代理（/api/health，desktop 类请求经 L1 放行）。导出 `httpRequest` 供插件 shared runtime（auto-task 用 `mobileApi.httpRequest`）继续可用。

**Spec:** §3 目标架构、§4 前端段、§5.2（ApiResult 形状不变）、§9 D1/D3

**Blocked by:** 03, 04

**Status:** done

## 关键实现事实（handoff §2 已核实）

- 现状 `request()`：JWT 注入（`/api/auth/` 前缀排除）+ 加密信封（fail-closed，GET/HEAD 无 body 仍带协商头）+ pin 刷新（`notePinFromAuthData`）+ `httpProbe`（3s connectTimeout，无 JWT/无加密，**参数传入 address:port，不走 API_BASE_URL**）。
- 导出 `httpRequest` 供插件 shared runtime（auto-task 用 `mobileApi.httpRequest`）——收束后该导出保持签名兼容。
- `httpProbe` 语义：探测 address:port 的 /api/health；走代理后为 desktop 类请求（L1 放行；时序细节见 ticket 03 前置决策）。
- 单测改造：mock invoke（request_id 透传、响应路由、错误分支、取消传播）。

## 实现清单

- [x] `request()` 改纯 invoke(`http_request`)，删 tauriFetch / JWT 注入 / 信封逻辑；ApiResult 归一化保持
- [x] UUID request_id 生成（D3）随 invoke 传
- [x] `httpProbe` 走代理（desktop 类 /api/health，L1 放行；时序决策按 ticket 03）
- [x] `httpRequest` 导出签名兼容（插件 shared runtime 不破坏）
- [x] 单测：mock invoke（request_id 透传、响应路由、HTTP/网络错误分支、取消）

## 验证

- vitest（`pnpm run test:run`）useHttpApi 相关用例全绿
- 前端源码本文件零 `fetch(` / `tauriFetch` 引用

## Comments

- 2026-09-11 完成：`useHttpApi.ts` 收束——`request()` 改纯 `invoke('http_request')`（uuid request_id D3、desktop 类、timeoutMs 30000、body 归一化；删除 tauriFetch / JWT 注入 / 加密信封 / pin 刷新——全部由 Rust 代理完成）；`httpProbe` 走代理（先 `egress_declare_desktop_target` 声明目标——L1 时序方案 a 的前端侧，再 http_request desktop 类 + 3s 超时）；`setApiBaseUrl` 时同步声明桌面端目标；`httpRequest` 导出签名不变（插件 shared runtime mobileApi 兼容，src/plugin/index.ts 引用未破坏）。
- 集成测试适配（4 文件 + helpers）：helpers 加 `mockProxyResponse`（HttpProxyResponse 形状）；session-flow / connection-flow / pairing-flow / terminal-flow 的 `mockFetch`（plugin-http）分发改为 `mockInvoke('http_request')` 按 URL 分发 + `egress_declare_desktop_target` 分支；断言改 invokeCalls（url/kind/timeoutMs/body）。connection-flow 残留 `mockDesktopReachable` 调用删除（默认分发已覆盖）；session-flow 补 invokeCalls helper。
- 验证：vitest 全量 361 passed（42 文件）；vue-tsc exit 0；根目录 eslint 0 error（125 既有 warning）。
- 坑：connection-flow.test.ts 是 CRLF——sed 删除行不能用 `$` 锚（`\r` 干扰），用不含锚的模式。
