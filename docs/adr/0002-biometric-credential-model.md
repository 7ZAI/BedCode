# 生物凭证密钥模型 —— 移动端生成密钥对、私钥永不出设备

---
status: accepted
updated: 2026-08-06
---

> **Amended 2026-08-06**：实机排查（Xiaomi Pad 5，Android 13）发现「一直显示设备不支持生物认证」
> 的根因是 **IPC 序列化命名不匹配**：`BiometricKeyStatus` 未加 `#[serde(rename_all = "camelCase")]`，
> 响应为 snake_case（`device_supported`），而前端按项目约定读 camelCase（`status.deviceSupported`）
> → undefined → 误判为不支持。原生检测（`canAuthenticate(BIOMETRIC_STRONG)` = SUCCESS）一直正常。
> 修复：响应结构体补 `rename_all = "camelCase"`（与 commands/session.rs 约定一致），
> 并新增检测结果码透传（BiometricManager code：0=SUCCESS 1=HW_UNAVAILABLE 11=NONE_ENROLLED
> 12=NO_HARDWARE）与前端状态错误态（区分"检测失败"与"设备不支持"），避免同类问题再被静默掩盖。
> 另确认：Android CDD 规定仅强生物特征（Class 3）允许与 Keystore 集成做加密运算，
> 弱生物特征设备（摄像头人脸等）无法使用本功能，属平台硬约束。

生物认证作为第三种连接认证方式（与配对码、QR 并列），需要一枚"绑定"到桌面端的设备密钥：移动端本地生成非对称密钥对，私钥存入系统安全硬件（Android Keystore `setUserAuthenticationRequired(true)` / iOS Secure Enclave），**仅生物认证通过后私钥可被取用签名，永不出设备**；公钥在已认证连接（配对码/QR 完成配对后）上注册到桌面端，存于 pairings.public_key。连接握手为挑战-应答：桌面端下发一次性随机数 → 移动端生物认证解锁私钥签名 → 桌面端用公钥验签 → 签发 JWT（7 天，与配对码签发一致，有效期内重连仍走 Reauthenticate 静默再认证）。

## Considered Options

- **服务端生成密钥对并签发私钥交付移动端**：密钥经网络传输，桌面端曾持有私钥副本，且硬件绑定的密钥（Keystore/Secure Enclave）无法从外部注入——与"生物认证门禁保护私钥"的语义根本冲突。否决。
- **纯软存储 + 生物认证仅作 UI 门禁**（Rust 生成密钥、加密落盘、弹窗后放行）：实现最简，但文件系统层面的攻击可绕过门禁直接取钥，保护是名义上的。否决。
- **移动端生成 + 自建原生插件（本方案）**：私钥一生只存在于安全硬件内，生物门禁由硬件强制。代价是新增一个跨平台原生插件（Kotlin/Swift），且需要设备实机验证。

## Consequences

- **新增原生插件** `tauri-plugin-biometric-key`（mobile 项目内）：Android 用 Keystore + BiometricPrompt，iOS 用 Secure Enclave + LAContext；Rust 侧经 PluginHandle 调用，供绑定（生成+取公钥）、签名（弹生物认证）、删除使用。
- **签名时同步弹生物认证**：签名命令内完成"弹窗 + 取钥 + 签名"，一次原生调用返回签名，避免前端先弹一次再签一次的割裂体验。
- **绑定依赖已认证连接**：未配对/未连接时无法注册公钥，生物认证选项不可用；解绑 = 移动端删密钥 + 通知桌面端清公钥。
- **移除设备连带删除连接历史**：无审计需求，桌面端移除配对时级联删除。
- **新增指纹录入后密钥失效**：`setInvalidatedByBiometricEnrollment(true)`，换指纹/面容需重新绑定（iOS 用 `BiometryCurrentSet` 同语义）。
