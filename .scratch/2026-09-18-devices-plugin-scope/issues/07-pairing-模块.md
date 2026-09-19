# 07: pairing 模块（B2）

**What to build:** 配对码 / QR token / JWT 签发校验策略从宿主语义层平移至认证中心插件（宿主保留密码学引擎与密钥托管），与宿主实现**行为等价**（对照测试：同一输入同输出）。

**Blocked by:** 06, 05（插件骨架 + JWT 密钥治理链路完整）

**Status:** done（2026-09-19）

- [x] 对照测试：配对码 / QR token / JWT 签发校验与宿主实现同一输入同输出
- [x] pairing 策略单测：TTL 边界、一次性语义（正反例）
- [x] HS256 官方 test vector（RFC 7515）通过
- [x] 密钥经 host-auth secret-store 获取，明文不出宿主、不进日志
- [x] 插件 cargo test 全绿

## 验证证据（2026-09-19）

- **插件单测**：`bedcode-desktop/plugins/devices/rust` 全量 **28 passed / 0 failed**
  （pairing 模块 code 14 / qr 11 / jwt 3 向量+对照 / keys 3，另 lib 0）
- **RFC 7515 §A.1 官方向量**：固定 key（官方 base64url 解码 64B）+ 官方 header/payload
  （含 `\r\n` 换行原文）→ 签名 `dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk` 匹配；
  signing input 逐字符锁定；key 曾两次抄错（手写 hex 前 32B 对、后 32B 错 → 签名
  不匹配）——教训：RFC 向量必须由独立实现（Python hmac）交叉验证后再固化
- **结构等价对照**：固定 key（32B 0x00..=0x1f）+ 固定 claims（sub=device-1, iat=1700000000,
  exp=1700604800, device_name/fingerprint）→ 插件 token 与宿主 jsonwebtoken 9.3.1
  encode 结果**逐字符一致**（header `{"typ":"JWT","alg":"HS256"}` 顺序、claims 字段
  声明序、URL_SAFE_NO_PAD base64url、HMAC-SHA256 全部对齐）；宿主侧新增
  `host_jsonwebtoken_matches_plugin_fixed_vector`（jwt.rs，6/6 绿）反向验签插件 token
- **宿主闭环**：`test_devices_plugin_artifact_lifecycle` 扩展 —— 真实 wasip3 产物经
  宿主 async 运行时：配对码生成/验证/一次性/错误码、QR token 生成/验证/一次性、
  JWT 签发→验签（claims 透传 + 篡改拒绝）、**密钥落库断言**（plugin_secrets 表
  `com.bedcode.devices` 属主行 `jwt.key` = 64 hex 字符，可解码回 32B）；全绿
- **全量**：桌面 `cargo test --lib` **971 passed / 0 failed**（基线 955 + 16：插件 28 单测
  经独立 crate 计数、宿主 jwt 对照 1、闭环扩展内联）

## 产物

- `plugins/devices/rust/src/pairing/`：`mod.rs`（模块出口）+ `code.rs`（配对码策略，
  `>` 过期语义、序列化剩余时间、RFC3339 手写格式/解析 + civil 算法单测锚点）+ `qr.rs`
  （QR token 策略，`>=` 过期语义、一次性消费、并发恰一成功线程测试）+ `jwt.rs`
  （HS256 自实现：b64url + hmac/sha2，RFC 7515 向量 + jsonwebtoken 语义复刻
  leeway=60/exp 必填/错误映射）+ `keys.rs`（SecretStore trait + wasm 路径 WasmHost
  host-auth / native mock，get-or-create 32B hex 落库，损坏存储显性失败）
- `lib.rs`：pairing 命令面（code.generate/verify、qr.generate/verify、jwt.generate/verify、
  key.status、pairing.status）+ static Mutex/OnceLock 状态（wasip3 thread_local 是真 TLS
  教训）+ activate 密钥探活（只记长度）；manifest api 未扩（归票 09 互调面）
- 依赖新增：hmac 0.12 / sha2 0.10 / base64 0.21 / hex 0.4 / getrandom 0.4（全部
  wasip3 可编译，产物 679KB Component）
- 宿主改动：`utils/auth/jwt.rs` +1 对照测试（jsonwebtoken 反向验签插件 token）；
  `wasm_runtime.rs` 闭环测试扩展（Arc::clone(host_ctx) 后 DB blocking_lock 查询）

## 说明

- **密钥域**：插件经 host-auth 管理自己的 `jwt.key`（插件属主隔离，宿主
  `plugin_secrets` 表 `jwt.key` 宿主域不可见）；明文不进日志（宿主与插件日志
  均只记长度/存在性）；未托管环境（native 单测）密钥注入式设计，无静默降级
- **wasip3 时间**：std::time 可用（wasi:clocks）；配对码/QR 秒级快照 + `*_at(now)`
  注入，宿主 Instant 亚秒 —— 秒级截断在 TTL 边界决策一致（`== ttl` 处配对码
  未过期 / QR 已过期，双端对齐）；对照测试全部注入时间，无真实睡眠
- **与宿主行为差异留档**：① `created_at` 序列化宿主 chrono 含小数秒、插件秒级
  （RFC3339 shape 对齐）；② 配对码生成分布（getrandom 拒绝采样 vs rand gen_range）
  不在对照范围（对照聚焦 verify 决策与剩余时间）；③ exp 类型错误的错误类别差异
  （宿主 MissingRequiredClaim → VerifyError vs 插件 InvalidToken），均拒绝
- **后续**：票 08 trust（设备信任列表统一视图）→ 09 consent + 互调 api 扩充
  （manifest.api 增 auth.pairing-status / auth.verify-device-token 等，宏自动防漂移）
- 遗留：密钥协商后再签发（当前 JWT 命令直接经 secret-store 密钥）——验签执行点
  留宿主中间件（票 12），本票仅证明插件策略与宿主行为等价
