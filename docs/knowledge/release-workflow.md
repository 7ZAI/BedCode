# GitHub Actions Release 工作流使用指南

## 工作流概述

`.github/workflows/release.yml` 支持 Windows NSIS 安装包 + Android APK 的自动构建发布。

**触发方式：**
- 推送 `v*` 格式 tag 自动触发
- GitHub Actions 页面手动触发 (workflow_dispatch)

---

## 前置配置：GitHub Secrets

在仓库 **Settings → Secrets and variables → Actions** 中添加以下 Secrets。

### 必需（Android 签名）

| Secret | 说明 | 获取方式 |
|--------|------|---------|
| `ANDROID_KEY_BASE64` | keystore 文件的 base64 编码 | 在终端执行：<br/>macOS/Linux: `base64 -i your-keystore.jks`<br/>Windows PowerShell: `[Convert]::ToBase64String([IO.File]::ReadAllBytes("your-keystore.jks"))` |
| `ANDROID_KEY_ALIAS` | 密钥别名 | 生成 keystore 时设定的 alias |
| `ANDROID_KEY_PASSWORD` | 密钥密码 | 生成 keystore 时设定的密码 |

> **注意：** `storePassword` 与 `keyPassword` 在工作流中使用同一个 Secret `ANDROID_KEY_PASSWORD`，如需不同密码请分别配置。

### 可选（Windows 代码签名，预留）

| Secret | 说明 | 获取方式 |
|--------|------|---------|
| `WINDOWS_CERTIFICATE` | PFX 证书的 base64 编码 | 同上 base64 编码方式 |
| `WINDOWS_CERTIFICATE_PASSWORD` | PFX 证书密码 | — |

未配置时 Windows 构建跳过签名步骤，生成未签名的安装包。

### 必需（Updater 更新签名）

| Secret | 说明 | 获取方式 |
|--------|------|---------|
| `TAURI_SIGNING_PRIVATE_KEY` | Tauri updater 私钥内容（单行 base64） | `pnpm exec tauri signer generate -w ~/.tauri/bedcode.key` 生成的 `.key` 文件全部内容 |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 私钥密码 | 生成密钥时设置的密码（未设置则为空） |

`tauri.conf.json` 已配置 `createUpdaterArtifacts` 与 `plugins.updater.pubkey`，构建升级包时强制要求私钥，缺失会导致构建失败。对应公钥必须与 `bedcode-desktop/src-tauri/tauri.conf.json` → `plugins.updater.pubkey` 一致，否则客户端校验更新签名失败。

**密钥轮换：** 重新生成密钥对后，必须同时更新 `tauri.conf.json` 的 `pubkey` 与上述两个 Secret，否则已发布版本的自动更新校验会失败。

---

## 发布流程

### 1. 更新版本号

同时更新以下两处版本号：

- `src-tauri/tauri.conf.json` → `"version": "x.x.x"`
- `src-tauri/Cargo.toml` → `version = "x.x.x"`

### 2. 提交并打 Tag

```bash
git add src-tauri/tauri.conf.json src-tauri/Cargo.toml
git commit -m "chore: bump version to x.x.x"
git tag vx.x.x
git push origin dev --tags
```

### 3. 等待构建完成

推送 tag 后 GitHub Actions 自动触发。在 **Actions → Release** 页面查看进度。

- **build-windows**: ~10-15 分钟
- **build-android**: ~15-25 分钟（依赖 Windows Job 完成）

### 4. 审核并发布 Release

构建完成后，在 **Releases** 页面找到 Draft Release：

1. 检查产物是否完整：
   - `BedCode_x.x.x_x64-setup.exe`（Windows NSIS）
   - `*.apk`（Android）
2. 编辑 Release Body（可选）
3. 点击 **Publish release** 发布

---

## 手动触发（不推送 Tag）

1. 进入 **Actions → Release**
2. 点击 **Run workflow**
3. 版本号从 `tauri.conf.json` 自动读取
4. 构建完成后同样在 Releases 页面审核发布

---

## 常见问题

### Android 构建失败：签名相关错误

确认 `key.properties` 属性名与 `build.gradle.kts` 一致：
- `keyAlias` / `keyPassword` / `storeFile` / `storePassword`

确认 keystore 文件 base64 编码正确，可本地验证：
```bash
base64 -d <<< "$ANDROID_KEY_BASE64" | file -
# 应输出: ... Java KeyStore
```

### Windows 安装包未签名

需要配置 `WINDOWS_CERTIFICATE` 和 `WINDOWS_CERTIFICATE_PASSWORD` Secrets，并在 `tauri.conf.json` 中设置 `certificateThumbprint` 和 `timestampUrl`。

### 本地构建报 "A public key has been found, but no private key"

`tauri.conf.json` 启用了 updater 升级包生成，构建时要求签名私钥。桌面端 `pnpm run tauri:build`（`scripts/tauri-build.js`）已按环境自动处理：

- 提供了 `TAURI_SIGNING_PRIVATE_KEY`（或 `TAURI_SIGNING_PRIVATE_KEY_FILE`、项目 `.env`）：构建签名的升级包
- 未提供：自动以 `createUpdaterArtifacts=false` 构建不含升级包的本地安装包

正式发布包由 GitHub Actions 通过 Secrets 签名，本地构建无需私钥。注意：本地私钥必须与 `tauri.conf.json` 中的 `pubkey` 配对，用不配对的密钥签名会导致客户端更新校验失败。

### 构建超时

首次构建无 Rust 缓存，时间较长属正常。后续构建会利用 `swatinem/rust-cache` 加速。

---

## Secrets 速查

| Secret | 必需 | 用途 |
|--------|------|------|
| `GITHUB_TOKEN` | 自动提供 | 创建 Release |
| `ANDROID_KEY_BASE64` | 是 | Android keystore |
| `ANDROID_KEY_ALIAS` | 是 | Android 密钥别名 |
| `ANDROID_KEY_PASSWORD` | 是 | Android 密钥密码 |
| `WINDOWS_CERTIFICATE` | 否 | Windows 代码签名 PFX |
| `WINDOWS_CERTIFICATE_PASSWORD` | 否 | PFX 密码 |
| `TAURI_SIGNING_PRIVATE_KEY` | 是 | Updater 更新签名私钥 |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 是 | 私钥密码 |
