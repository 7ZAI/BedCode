# 生物认证完全下沉认证中心（biometric downsink，ABI desktop 33 → 34）

## 为什么（用户裁定）

宿主 `utils/auth/biometric.rs` 的挑战状态机已随票 07 迁插件，但**公钥托管 + P-256 验签
执行仍留宿主**（host-auth `biometric-*` 三原语）。用户裁定：生物线整体下沉认证中心
（`com.bedcode.terminal-session`），与 v33 入场 JWT 迁中心同一路线（ADR 0033 D1 精神）。

## 双层判断

- **是否业务代码**：编排（挑战/单次消费/过期/配对判定）已迁；剩下的是「公钥托管 +
  验签执行」——按 AGENTS §5.1.3 属引擎原语候选，但按「认证全归中心」纯粹性应下沉。
- **安全**：P-256 公钥是**公开材料**（私钥永在移动端安全硬件，ADR 0002），迁插件私有库
  无泄露面变化；验签没有「私钥在 guest 内存」问题。真正变化的是**执行点位置**。

## 变更清单（七层）

### 1. 插件（terminal-session）——先做，验证性最强
- `auth_records/store.rs`：新增 `auth_biometric_keys(fingerprint PK, public_key)` 表 +
  `biometric_key_get/set/delete` 端口（wasm 实现 + Mock 同步）
- `auth_records/ops.rs`：生物公钥 CRUD 编排（native 单测覆盖）
- `auth_http/biometric.rs`：
  - `issue_challenge`：闸门改 `pairing_active` + 私有库公钥存在（不再调 host-auth bound）
  - `verify_signature`：公钥从私有库取 → **WASM 内 p256 验签**（不再调 host-auth verify）
- `auth_http/mod.rs`：`biometric_credential_bind` 改写私有库（不再调 host-auth bind）
- `Cargo.toml`：`p256 = { version = "0.13", features = ["ecdsa", "pkcs8"] }`（探针已验证
  wasm32-wasip3 编译通过）
- 宿主旧 `utils/auth/biometric.rs` 的 `verify_biometric_signature` 逻辑在插件侧用
  p256 crate 复刻（`from_public_key_der` + r||s raw session）

### 2. WIT（桌面）
- `host-auth` 删 `biometric-credential-bound` / `biometric-verify-signature` /
  `biometric-credential-bind` 三函数（破坏性）
- ABI desktop 33 → **34**

### 3. SDK（桌面）
- `host/auth.rs` trait 删三方法
- `wasm_host.rs` 删三实现 + wasm_binary/签名面核对
- `abi.rs` ABI_VERSION 33 → 34 + 注释 + 测试

### 4. 宿主
- `wasm_core/host_api/auth.rs`：删三函数 + `biometric_secret_key` + 相关测试
- `utils/auth/biometric.rs`：整文件删（含死代码 BiometricChallengeManager）
- `utils/auth.rs`：删 `pub mod biometric;`
- `system/app_context.rs`：删 biometric_challenges 字段/getter/构造
- `server/websocket/conn.rs`：删 clear 调用
- 集成测试 `tests/http_auth_biometric.rs`：seed 公钥从宿主主库 `plugin_secrets`
  改到插件私有库 `auth_biometric_keys`

### 5. 权限
- 生物三函数用 `PERMISSION_AUTH`，其余 host-auth 面仍用（secret/setting/center），
  权限位保留；无新增

### 6. 双端
- 移动端 WIT **无** biometric 原语（走 HTTP /api/auth/biometric-*），线协议不变 →
  双端偏离条款适用（ADR 0022），mobile ABI 不动，移动端零改动（核对确认）

### 7. 文档 + 产物
- CHANGELOG.md / CHANGELOG_zh.md（ABI 34 条目）
- ADR 0022 修订记录追加 v34
- ADR 0033 修订（Out of Scope 生物行）
- checklist 版本行 33 → 34 + 新条目
- AGENTS.md §8「生物凭证（P-256 公钥）仍由宿主托管」→ 更新为已下沉
- code-map host-auth 描述
- terminal-session 产物重建（wasmHash 注入）

## 实施顺序（ABI bump 硬约束）

1. 插件侧先自持（native + wasm 编译/测试过）→ 2. WIT + SDK → 3. 宿主删面 → 4. 集成测试
→ 5. 双端核对 → 6. 文档 → 7. wasmHash + 全量回归

## 验证

- 插件：`cd wasm-apps/terminal-session/rust && cargo test`（native）+ wasm 编译
- 宿主：`cargo test --lib` + 全量
- 集成：`tests/http_auth_biometric.rs` 走真实中心产物闭环
- p256 WASM 验签性能探针（生物非热路径，但要有数——吸取 0033 #lesson）
## 实施后补充（2026-09-30 收尾）

- **全部代码/测试/文档落点已完成**：插件侧（auth_biometric_keys 表 + WASM p256 验签）、
  WIT/SDK（ABI 33→34 + trait/实现三删）、宿主（host_api/auth.rs 三函数删 +
  utils/auth/biometric.rs 整删 + app_context/conn.rs 清理 + v34 迁移清扫）、
  集成测试（seed 改私有库）、防回接锁（host_has_no_biometric_crypto）
- **验证**：插件 403 绿；宿主 lib 相关面全绿（l2_gating 10 / db 22 / gateway 20 /
  auth_center 10 / websocket 21 / host_api::auth 14）；集成测试 http_auth_biometric +
  ws_auth_rules 绿；wasm 产物重建（wasmHash 已注入）
- **环境说明**：工作区存在并行会话（refactor/merge-test-fixtures 分支的 fixture 合并
  重构，wasm 闭环测试属其范围）。我的变更集与它互不重叠（白名单核对过）；用户已确认
  该问题自行处理，本任务不介入。
