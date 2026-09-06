# 10 — 移动端链路加密 pin 落地断链修复 + 全链路闭环梳理

**What happened:** 移动端在连接配对已建立（JWT 有效、WS 已连）的情况下，设置页点击「启用加密链路」主开关仍弹「请先完成设备配对」toast；且即便绕过守卫，加密功能实际全程不生效。

**Status:** resolved（代码修复完成，双端测试全绿；真机联调验证待执行）

---

## 根因：pin（桌面端身份公钥）在移动端从不落地

加密协商的信任锚是配对期 auth 响应携带的桌面端身份公钥（`kdPublicB64` / `kdFingerprint`），但该数据在移动端**三处断链**：

1. **前端认证不走 pin 提取路径**：移动端配对/重认证全部走 Rust invoke（`wsVerifyPairingCode` / `wsAuthenticate` / `wsAuthenticateWithQr` / `wsAuthenticateWithBiometric`），而 `notePinFromAuthData`（前端唯一写 pin 的函数）挂在 `useHttpApi` 的 `httpVerifyPairingCode` / `httpQrConnect` / `httpReauth` 上——这三个函数**全仓零调用**（死代码）。→ localStorage 的 `link_kd_public_b64` 恒为空。
2. **Rust 侧丢弃 pin**：移动端 `AuthTokenResponseData`（`auth/http.rs`）没有 kd 字段，桌面端 auth 响应携带的身份公钥被 serde 静默丢弃。
3. **连带症状**：设置页守卫用 `getPinnedFingerprint()`（恒 null）→ 误判未配对；即使绕过守卫，`isChannelEncryptionActive`（依赖 `getPinnedKey()`）恒 false、Rust `is_event_encryption_active()`（依赖 context 的 kd）恒 false → **HTTP / 终端 WS / 事件 WS 三通道加密全部形同虚设**。

## 修复：pin 随认证成功统一落地

认证成功收尾统一入口 `apply_auth_success`（配对码 / QR / reauth / 生物认证四个 HTTP 方法共用）补 pin 收尾：

```
桌面端 auth 响应 (kdPublicB64/kdFingerprint)
  → Rust apply_auth_success:
      ① update_link_crypto_pin → LinkCryptoContext.kd_public_b64（事件 WS 建连协商直接用）
      ② 广播 MobileEvent::LinkCryptoPin → ws_link_crypto_pin 前端事件
  → 前端 initLinkCryptoPinSync 监听 → applyPin 写 localStorage
  → syncLinkCryptoContextToNative 回推（幂等，与 ① 一致）
```

**改动清单：**

| 文件 | 改动 |
|---|---|
| `bedcode-mobile/src-tauri/src/auth/http.rs` | `AuthTokenResponseData` 增加 `kd_public_b64` / `kd_fingerprint`（`#[serde(default)]` 兼容老桌面端省略） |
| `bedcode-mobile/src-tauri/src/auth/manager.rs` | `apply_auth_success` 扩展签名接收 kd → 更新 Rust context + 广播 `LinkCryptoPin`；4 个认证方法传参 |
| `bedcode-mobile/src-tauri/src/state.rs` | 新增 `update_link_crypto_pin()`（仅 Some 覆盖，None 不清除——防主动降级攻击抹除信任锚） |
| `bedcode-mobile/src-tauri/src/router/event.rs` | `MobileEvent::LinkCryptoPin` 变体 + forward 转 `ws_link_crypto_pin`（camelCase 字段，null 不写 pin） |
| `bedcode-mobile/src/composables/useLinkEncryption.ts` | 抽取 `applyPin` 统一写入入口（公钥必存、指纹随带）；新增 `initLinkCryptoPinSync()` 监听事件；`notePinFromAuthData` 改为复用 `applyPin` |
| `bedcode-mobile/src/App.vue` | 启动时注册 `initLinkCryptoPinSync()` |
| `bedcode-mobile/src/views/settings/ConnectionSettingsView.vue` | 主开关守卫从 `getPinnedFingerprint()` 放宽为 `getPinnedKey()`（公钥才是协商前提，指纹仅展示用途） |
| `bedcode-mobile/src/__tests__/composables/useLinkEncryption.test.ts` | 新增 pin 写入语义测试：指纹缺失仍存公钥 / 空 payload 不清既有 pin |

**防降级语义保持**：`update_link_crypto_pin` 与前端 `applyPin` 都只在收到非空 kd 时覆盖，协商失败 / 守卫拦截均不清 pin——信任锚只随重新配对/重认证刷新，不因降级被抹除。

## 验证

- `cargo check`（移动端）：通过
- `cargo test`（移动端）：328 项全绿（306 lib + 11 集成 + 10 + 1）
- `pnpm run test:run`（移动端前端）：341 项全绿（含新增 3 个 pin 语义用例）
- LSP primary：clean（无类型错误）

## 加密链路闭环梳理（供验收对照）

**四步设计闭环**：配对期下发身份锚 → pin 落盘 → 三通道协商 → 降级裁决。

| 通道 | 实现位置 | 协商方式 | 降级裁决 |
|---|---|---|---|
| HTTP REST | 前端 `useHttpApi.request()` | 每请求新临时 X25519 密钥对，`X-BedCode-Crypto: v1 <ek>` 头 + 信封 `{v,n,ct}`；AAD 含路径/方向 | strict → `LINK_ENCRYPTION_DOWNGRADE`；非 strict 明文续跑 + warn |
| WS 终端 | 前端 `useTerminalSocket` | 建连首帧 `crypto:{v,ek}` 提案 → auth_ok 回执 `crypto.ek` → `deriveWsSession`；帧 seq 严格单调 | strict → 断连；非 strict 明文续跑 |
| WS 事件 | Rust `connection/manager.rs` + `event_ws.rs` | `reauthenticate_with_crypto` 协商 → `ClientWsCrypto::derive` → `install_link_crypto`；重连重新握手 | strict → 断连报错；非 strict 明文续跑 |

**修复后完整闭环**：任一认证成功（配对码/QR/reauth/生物）→ 桌面端下发公钥+指纹 → Rust context 更新 + 事件广播 → 前端 localStorage 落盘 → 事件 WS 建连协商、HTTP/终端 WS 通道判定全部可用 → 设置页指纹展示、开关守卫与实际状态一致。

## 追加修复（真机验证发现）：HTTP 加密链路两处协议缺口

**现象**：启用加密链路后移动端 HTTP 请求失败——GET 全部报 fetch 构造错误，POST
（sessions/start、file-tree 等）响应 `code= undefined` 界面显示失败但实际已成功。

**根因（查移动端 dev 日志 `android-dev.YYYY-MM-DD.log` + 对照 spec §4）**：

1. **响应标记头缺失（核心）**：spec §4 要求桌面端加密响应后回 `X-BedCode-Crypto: v1`
   响应头作标记，但 `on_http_outbound` 只改了响应体、从未设置响应头。移动端
   `decryptResponse` 依赖该头判定是否解密 → 实际响应是密文信封被当明文
   `JSON.parse` → 无 `code` 字段（`code= undefined`）→ UI 误报失败。POST 请求体
   加密正常、桌面端已执行（会话/文件树真实创建）——与服务端行为一致。
2. **GET 请求带信封 body**：移动端 `request()` 对 GET 也把信封塞进 body，满足
   HTTP 语义（GET/HEAD 无 body）→ `Failed to construct 'Request': Request with
   GET/HEAD method cannot have body` → 所有 GET（configs/sessions/git 等）直接失败。

**修复**：

| 端 | 文件 | 改动 |
|---|---|---|
| 桌面 | `server/filter.rs` | `FilterContext` 新增 `outbound_headers` 字段（HTTP 出站附加头通道） |
| 桌面 | `server/link_crypto.rs` | `on_http_outbound` 加密成功后 push `X-BedCode-Crypto: v1` 标记头；`on_http_inbound` 空 body（GET/HEAD）跳过解信封仅缓存响应密钥 |
| 桌面 | `server/middleware/http_filter.rs` | 重建响应时注入 `outbound_headers`（显式 HeaderName/HeaderValue 构造） |
| 移动 | `composables/useHttpApi.ts` | GET/HEAD 不再塞信封 body：仍发协商头 + 派生响应密钥，body 留空 |
| 测试 | 桌面 `link_crypto.rs` | 既有 roundtrip 断言标记头；新增 `http_get_with_empty_body_negotiates_and_encrypts_response` |

**二次修复（标记头取值 bug，真机复测发现）**：`PROTOCOL_VERSION` 是 `u8 = 1`，
初版用了 `.to_string()` 产出 `"1"`，而移动端判定是 `respHeader === 'v1'`——
响应头 `X-BedCode-Crypto: 1` 永远不匹配 → 仍判 downgrade。改为
`format!("v{PROTOCOL_VERSION}")`（与 `parse_negotiation` 的请求侧同源），
测试断言同步改为 `"v1"`。

**闭环效果**：GET/HEAD 请求侧无 body 可加密（查询参数本就明文 URL），协商头 + 临时
公钥仍触发桌面端派生响应密钥 → 响应加密 + `X-BedCode-Crypto: v1` 标记头 → 移动端
按头解密。POST 全链路不变。

**验证**：桌面 `cargo test` 568 全绿（含新 GET 协商用例与 `"v1"` 取值断言）；移动
前端 341 绿；移动 Rust 328 绿。

## 遗留观察项（未在本次改动内）

1. **生效时机**：开启加密后已建立的连接不重协商——HTTP 每请求即时生效，两条 WS 通道需等下次重连/建连才加密（设置页 hint 可加一句「重启连接后对 WS 通道生效」）。
2. **升级补种**：已配对旧用户（修复前从未落过 pin）下一次 reauth（自动重连/重启触发）即自动补上，无需重新配对——事件广播每次认证成功都发，覆盖首配 + 重连。
3. **指纹展示**：设置页「对端指纹」随修复自动从占位态恢复正常展示。

> 实施备注：
> - 事件名 `ws_link_crypto_pin`，payload 字段 camelCase（`kdPublicB64` / `kdFingerprint`），null 不清既有 pin
> - `notePinFromAuthData` 保留（将来 HTTP auth 通道接入仍用），但真实落地以 Rust 广播事件为主路径
> - 真机联调：`pnpm run tauri:android:dev:log` 下验证「已配对设备 → 开启加密开关 → 指纹立即显示 → 事件 WS 重连后协商加密」