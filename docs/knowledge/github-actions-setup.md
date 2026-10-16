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

工作流文件共 5 个（`.github/workflows/`）：

| 文件 | 触发 | 作用 |
| --- | --- | --- |
| `lint.yml` | push / PR（非 `dev`）+ 改动 `bedcode-{desktop,mobile}/**`、`scripts/**`、`eslint.config.js` | 根 `pnpm exec eslint .`（0-error 基线）+ 根 `pnpm run test:run`（`scripts/*.test.mjs`，含「ESLint 锁 glob ↔ 真实目录」结构锁） |
| `test.yml` | 同上 + `cross-end-tests/**`、`packages/**`、`contracts/**` | `rust-desktop` / `rust-capability-crates`（根 `packages/*/` 各 crate 单测，清单由目录推导）/ `rust-mobile` / `cross-end` / `frontend-desktop` / `frontend-mobile` 六个并列 job |
| `wit-drift-lock.yml` | 同上 + `packages/**`、`contracts/**`、双端 SDK WIT 目录、`scripts/compose-wit.mjs` | `node scripts/compose-wit.mjs --all --check` 只读漂移锁（与 husky pre-commit 同判据） |
| `release.yml` | tag `v*` / workflow_dispatch | 发布流水线（见下） |
| `sdk-publish.yml` | tag `sdk-v*` / workflow_dispatch | SDK 发布（npm + crates.io）：版本一致性校验 + 构建测试 + dry-run + 幂等发布 |

> `dev` 是本地集成主线：对它的 push 不触发任何 workflow（`branches-ignore: dev`），CI 在合并到
> `master` / `uat` 时接管。根 `packages/**`、`contracts/**` 只在 `test.yml` / `wit-drift-lock.yml`
> 的 paths 里——只改机制内核 / 能力域 crate 或契约版本真源的 PR 也必须有门禁。
>
> `frontend-desktop` / `frontend-mobile` 在 vitest **之前**先构建各自 SDK 的 `dist/`
> （`packages/plugin-sdk-{desktop,mobile}`）：wasm 应用测试 import SDK 包入口（`main` → `dist/index.js`），
> 而 `dist/` 不入库——不先构建就只有 CI 会红（本地因遗留 dist 在场而假绿）。同理，
> `package-sdks` 需先装 mobile SDK dev-shell 的依赖（独立 workspace），否则其单测报找不到 `vue-i18n`。

### release.yml 的 job 图

平台构建与产物打包**全部并行**（各自 `needs: prepare-release`），不是串行：

```
prepare-release（提取版本 + 生成双语 changelog）
   ├─ build-windows   (windows-latest)  -> 签名 NSIS + latest.json 记录
   ├─ build-linux     (ubuntu-latest)   -> 签名 deb
   ├─ build-android   (ubuntu-latest)   -> 签名 APK
   ├─ package-plugins (ubuntu-latest)   -> dist/plugin-packages/{desktop,mobile}/<id>.zip
   └─ package-sdks    (ubuntu-latest)   -> dist/sdk-packages/{desktop,mobile}/*
            ▼
   finalize-release（同步 release body + 校验 latest.json 已上传）
            ▼
   publish-release（仅 tag 在 master 祖先上：draft -> 正式）
```

**sccache 是硬前置**：根 `.cargo/config.toml` 强制 `rustc-wrapper = sccache`，任何跑 cargo 的 job 都必须先装
（`mozilla-actions/sccache-action@v0.0.11`）——漏装 = cargo 直接失败，不是降速。

**桌面插件构建链的工具链（易漏配）**：桌面插件不是单一 target——`agent-hub` / `ai-chatbox` /
`terminal-session` 是 `wasm32-wasip3`（构建脚本注入 `RUSTUP_TOOLCHAIN=nightly-2026-09-16`，真源
`scripts/plugin-wasm-config.mjs` + `scripts/wasip3-toolchain.sh`），`file-transfer` 是
`wasm32-unknown-unknown` + 仓库级组件化（`packages/bedcode-componentize`）。因此**所有会跑
`pnpm run plugins:build` 的 job**（`test.yml` 的 `rust-desktop` / `cross-end`，`release.yml` 的
`build-windows` / `build-linux` / `package-plugins`）都必须装：stable 的 `wasm32-unknown-unknown`
（+ `wasm32-wasip2`，wasm-core 的 WASI preopen 夹具需要）与 pinned nightly 的 `wasm32-wasip3`。

### 构建步骤概览

**Windows / Linux（桌面端）**：Checkout -> pnpm/Node -> Rust stable（`wasm32-unknown-unknown,
wasm32-wasip2`）-> pinned nightly `wasm32-wasip3` -> sccache -> rust-cache ->（Windows 导入签名证书）
-> `pnpm install` -> 构建桌面 SDK dist -> `pnpm run plugins:build` -> 校验
`createUpdaterArtifacts` 与签名密钥 -> tauri-action 构建并写入 draft release。

**Android（移动端）**：Checkout -> pnpm/Node -> Rust stable（`aarch64-linux-android,
wasm32-unknown-unknown, wasm32-wasip2`）-> sccache -> rust-cache -> Java 17 -> Android NDK -> 签名
（`src-tauri/gen/android/key.properties`）-> `pnpm install` -> 构建移动 SDK dist -> 各移动插件依赖
（清单由 `wasm-apps/*/` 目录推导，不再手写）-> `tauri android build --apk -t aarch64` -> 上传 APK 到
draft release。

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

> **sccache（2026-10-05 引入）**：仓库根 `.cargo/config.toml` 强制 `rustc-wrapper = "sccache"`，
> 因此**所有跑 cargo 的 job 必须安装 sccache**（test.yml 的 rust-desktop/rust-mobile/cross-end、
> release.yml 的 build-windows/build-android/build-linux/package-plugins/package-sdks、
> sdk-publish.yml 的 verify/publish-crates 已用 `mozilla-actions/sccache-action@v0.0.11`，
> 漏装 = cargo 直接失败）。CI 不启用 sccache 的 GHA cache 后端（不设 `SCCACHE_GHA_ENABLED`）：
> 跨 run 缓存仍由 `swatinem/rust-cache` 负责，sccache 只做 job 内跨 crate 命中（server-libs /
> wasm-apps 循环共享依赖），job 级 `SCCACHE_CACHE_SIZE=2GiB`（根 config `[env] force=false`
> 让外部值优先于 20GiB 默认）。

### 忘记 keystore 密码

查看项目中已有的 `key.properties` 文件（通常在 `src-tauri/gen/android/key.properties`），里面包含密码信息。
