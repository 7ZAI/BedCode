# GitHub Actions 完整配置指南

从零开始配置 GitHub Actions CI/CD 的完整步骤，包括认证、代理、Secrets 配置、工作流执行和发布。

---

## 1. 安装 GitHub CLI

下载地址：https://cli.github.com

Windows 下载 `.msi` 安装包，双击安装。安装后**重启终端**使 PATH 生效。

验证安装：

```bash
gh --version
```

---

## 2. GitHub CLI 登录认证

```bash
gh auth login
```

按提示选择：
1. **GitHub.com**
2. **HTTPS**
3. **Login with a web browser**（推荐，会生成验证码，浏览器打开后粘贴）

验证登录状态：

```bash
gh auth status
```

应输出类似：

```
✓ Logged in to github.com account 7ZAI (keyring)
  - Git operations protocol: https
  - Token scopes: 'gist', 'read:org', 'repo', 'workflow'
```

> **注意：** Token scopes 必须包含 `repo` 和 `workflow`，否则无法推送和触发 Actions。

---

## 3. 配置 Git 代理（国内网络必须）

如果无法直连 GitHub，需要配置代理：

```bash
git config --global http.proxy http://127.0.0.1:端口号
git config --global https.proxy http://127.0.0.1:端口号
```

例如端口号 10808：

```bash
git config --global http.proxy http://127.0.0.1:10808
git config --global https.proxy http://127.0.0.1:10808
```

取消代理：

```bash
git config --global --unset http.proxy
git config --global --unset https.proxy
```

---

## 4. 推送代码到 GitHub

首次推送：

```bash
git remote add origin https://github.com/7ZAI/BedCode.git
git push -u origin master
```

后续推送：

```bash
git push origin master
```

---

## 5. 配置 GitHub Secrets

Secrets 是加密的环境变量，工作流中使用 `${{ secrets.SECRET_NAME }}` 读取。

### 5.1 通过 GitHub CLI 配置（推荐）

```bash
# 设置单个 Secret（运行后输入值，不会回显）
gh secret set SECRET_NAME -R 7ZAI/BedCode

# 通过管道直接传入值
echo "bedcode" | gh secret set ANDROID_KEY_ALIAS -R 7ZAI/BedCode
echo "bedcode123" | gh secret set ANDROID_KEY_PASSWORD -R 7ZAI/BedCode

# keystore 转 base64 并直接设置
# ⚠️ 必须是仓库根目录的 bedcode.keystore（发布签名唯一真源，证书 SHA-256 a85e2f1bc552...），
# 本地 gen/android/ 与 android-backup/ 下的 bedcode-keystore.jks / bedcode.keystore 均须与其同证书副本，
# 混用其他证书签名的 APK 会与已发布版本「签名不一致」而无法覆盖升级。
base64 -w 0 bedcode.keystore | gh secret set ANDROID_KEY_BASE64 -R 7ZAI/BedCode
```

查看已配置的 Secrets：

```bash
gh secret list -R 7ZAI/BedCode
```

### 5.2 通过 GitHub 网页配置

仓库 → **Settings** → **Secrets and variables** → **Actions** → **New repository secret**

### 5.3 所需 Secrets 清单

#### 必需（Android 签名）

| Secret | 说明 | 获取方式 |
|--------|------|---------|
| `ANDROID_KEY_BASE64` | keystore 文件的 base64 编码 | `base64 -w 0 your-keystore.jks` |
| `ANDROID_KEY_ALIAS` | 密钥别名 | 查看 keystore：`keytool -list -keystore your-keystore.jks` |
| `ANDROID_KEY_PASSWORD` | 密钥密码 | 生成 keystore 时设定的密码 |

> **注意：** `storePassword` 与 `keyPassword` 在工作流中使用同一个 Secret `ANDROID_KEY_PASSWORD`，如需不同密码请分别配置。

#### 可选（Windows 代码签名）

| Secret | 说明 | 获取方式 |
|--------|------|---------|
| `WINDOWS_CERTIFICATE` | PFX 证书的 base64 编码 | PowerShell: `[Convert]::ToBase64String([IO.File]::ReadAllBytes("cert.pfx"))` |
| `WINDOWS_CERTIFICATE_PASSWORD` | PFX 证书密码 | — |

未配置时 Windows 构建跳过签名步骤，生成未签名的安装包。

#### 自动提供

| Secret | 说明 |
|--------|------|
| `GITHUB_TOKEN` | GitHub 自动生成，无需手动配置，用于创建 Release |

---

## 6. Keystore 操作参考

### 生成新 keystore

```bash
keytool -genkey -v -keystore bedcode.keystore -alias bedcode \
  -keyalg RSA -keysize 2048 -validity 10000
```

### 查看已有 keystore 信息

```bash
keytool -list -keystore bedcode.keystore
```

### Keystore 转 base64

```bash
# Linux / macOS / Git Bash
base64 -w 0 bedcode.keystore

# Windows PowerShell
[Convert]::ToBase64String([IO.File]::ReadAllBytes("bedcode.keystore"))
```

### 验证 base64 编码正确性

```bash
base64 -d <<< "$ANDROID_KEY_BASE64" | file -
# 应输出: ... Java KeyStore
```

---

## 7. 工作流说明

当前工作流文件：`.github/workflows/release.yml`

### 触发方式

- **推送 `v*` 格式 tag** 自动触发
- **GitHub Actions 页面手动触发** (workflow_dispatch)

### 构建流程

```
build-windows (windows-latest)
    │
    │  ~10-15 分钟
    │
    ▼
build-android (ubuntu-latest, needs: build-windows)
    │
    │  ~15-25 分钟
    │
    ▼
Draft Release (包含 Windows 安装包 + Android APK)
```

### 构建步骤概览

**Windows 构建：**
1. Checkout 代码
2. 安装 Node.js LTS
3. 安装 Rust stable
4. Rust 编译缓存
5. 导入 Windows 签名证书（如已配置）
6. 安装前端依赖 (pnpm install)
7. Tauri 构建并创建 Draft Release

**Android 构建：**
1. Checkout 代码
2. 安装 Node.js LTS
3. 安装 Rust stable (target: aarch64-linux-android)
4. Rust 编译缓存
5. 安装 Java 17
6. 安装 Android SDK + NDK
7. 配置 Android 签名 (从 Secrets 生成 key.properties)
8. 安装前端依赖 (pnpm install)
9. 构建 Android APK
10. 上传 APK 到 Release

---

## 8. 执行工作流

### 方式一：手动触发（测试用）

```bash
# 通过 GitHub CLI 触发
gh workflow run release.yml -R 7ZAI/BedCode
```

或在 GitHub 网页：**Actions** → **Release** → **Run workflow**

手动触发时版本号从 `tauri.conf.json` 自动读取。

### 方式二：打 Tag 触发（正式发布）

```bash
# 1. 更新版本号（两处必须一致）
#    src-tauri/tauri.conf.json → "version": "1.0.1"
#    src-tauri/Cargo.toml → version = "1.0.1"

# 2. 提交并打 Tag
git add src-tauri/tauri.conf.json src-tauri/Cargo.toml
git commit -m "chore: bump version to 1.0.1"
git tag v1.0.1
git push origin master --tags
```

### 查看运行状态

```bash
# 列出最近的运行
gh run list -R 7ZAI/BedCode

# 查看特定运行的详情
gh run view <run-id> -R 7ZAI/BedCode

# 实时查看日志
gh run watch <run-id> -R 7ZAI/BedCode
```

---

## 9. 发布 Release

构建完成后，在 **Releases** 页面找到 Draft Release：

1. 检查产物是否完整：
   - `BedCode_x.x.x_x64-setup.exe`（Windows NSIS）
   - `*.apk`（Android）
2. 编辑 Release Body（可选）
3. 点击 **Publish release** 发布

---

## 10. 常见问题

### push 超时 / Connection reset

国内网络需要配置代理，参见 [第 3 节](#3-配置-git-代理国内网络必须)。

### gh: command not found

安装 GitHub CLI 后需**重启终端**使 PATH 生效。如仍找不到，使用完整路径：

```bash
"/c/Program Files/GitHub CLI/gh.exe" auth status
```

### Android 构建失败：签名相关错误

1. 确认 Secrets 中 `ANDROID_KEY_BASE64` 编码正确
2. 确认 `key.properties` 属性名与 `build.gradle.kts` 一致：
   - `keyAlias` / `keyPassword` / `storeFile` / `storePassword`
3. 本地验证 base64 编码：
   ```bash
   base64 -d <<< "$ANDROID_KEY_BASE64" | file -
   # 应输出: ... Java KeyStore
   ```

### Windows 安装包未签名

需要配置 `WINDOWS_CERTIFICATE` 和 `WINDOWS_CERTIFICATE_PASSWORD` Secrets，并在 `tauri.conf.json` 中设置 `certificateThumbprint` 和 `timestampUrl`。

### 构建超时

首次构建无 Rust 缓存，时间较长属正常。后续构建会利用 `swatinem/rust-cache` 加速。

### 忘记 keystore 密码

查看项目中已有的 `key.properties` 文件（通常在 `src-tauri/gen/android/key.properties`），里面包含密码信息。
