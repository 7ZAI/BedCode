# SDK 发布指南（npm + crates.io）

## 概述

`@binblink/plugin-sdk-desktop` 与 `@binblink/plugin-sdk-mobile` 是**双栈 SDK 包**，各包含前端 TS 部分和 Rust crate 部分，发布到两个 registry：

| SDK | npm 包 | crates.io crate |
|-----|--------|-----------------|
| 桌面端 | `@binblink/plugin-sdk-desktop` | `bedcode-plugin-api` |
| 移动端 | `@binblink/plugin-sdk-mobile` | `bedcode-plugin-api-mobile` |

发布由 `.github/workflows/sdk-publish.yml` 统一处理：一个 tag 联动发布 4 个产物。

**触发方式：**
- 推送 `sdk-v*` 格式 tag（如 `sdk-v0.1.0`）自动触发并发布
- GitHub Actions 页面手动触发（workflow_dispatch）只跑校验 + dry-run，**不发布**

---

## 前置配置（一次性）

### 1. GitHub Secrets（必需）

仓库 **Settings → Secrets and variables → Actions**：

| Secret | 说明 | 获取方式 |
|--------|------|---------|
| `NPM_TOKEN` | npmjs.com Access Token | npmjs.com → 头像 → Access Tokens → Generate New Token，类型选 **Automation**（CI 免 2FA） |
| `CARGO_REGISTRY_TOKEN` | crates.io API token | crates.io → Account Settings → API Tokens → New Token，scopes 留空 |

配置命令（需 `gh` CLI 已登录）：
```bash
gh secret set NPM_TOKEN --repo 7ZAI/BedCode --body "<token>"
gh secret set CARGO_REGISTRY_TOKEN --repo 7ZAI/BedCode --body "<token>"
```

### 2. 账号要求

- npm：注册 https://www.npmjs.com/signup（个人账号即可，scoped 包无需组织）
- crates.io：https://crates.io/ 用 GitHub 一键登录

---

## 发布流程

### 1. 修改版本号（两处必须一致）

tag 版本 = npm 版本 = Cargo 版本，CI 的 verify job 强制校验，不一致直接失败：

| 文件 | 字段 |
|------|------|
| `bedcode-desktop/packages/plugin-sdk-desktop/package.json` | `version` |
| `bedcode-mobile/packages/plugin-sdk-mobile/package.json` | `version` |
| `bedcode-desktop/packages/plugin-sdk-desktop/rust/Cargo.toml` | `version` |
| `bedcode-mobile/packages/plugin-sdk-mobile/rust/Cargo.toml` | `version` |

> desktop 与 mobile 各自独立版本号，可不同（但每个包内部 npm/cargo 必须相同）。

### 2. 提交并打 tag

```bash
git add -A
git commit -m "chore: release plugin sdk v0.1.0"
git push origin dev
git tag sdk-v0.1.0 && git push origin sdk-v0.1.0
```

### 3. 查看流水线结果

GitHub Actions → `SDK Publish` 工作流，三个 job：

| Job | 内容 | 失败后果 |
|-----|------|---------|
| `verify` | 版本一致性、构建、测试、`pnpm pack` / `cargo package` dry-run、wasm32 编译检查 | 直接中断，不发布 |
| `publish-npm` | 发布两个 npm 包（desktop 走 `pnpm publish --filter`，workspace 成员） | 独立 job，不阻塞 crates |
| `publish-crates` | 先 dry-run 再发布两个 crate | 独立 job |

---

## 发布后使用

外部插件开发者：

```bash
# 用已发布版本生成插件工程（--registry 引用 npm + crates.io 版本）
pnpm exec @binblink/plugin-sdk-mobile create com.example.demo "Demo" --registry
```

生成的依赖声明：
```jsonc
"@binblink/plugin-sdk-mobile": "^0.1.0"
```
```toml
bedcode-plugin-api-mobile = "0.1.0"
```

**两种依赖模式**（`create` 命令）：

| 模式 | 声明 | 场景 |
|------|------|------|
| 默认（相对路径） | `file:` / `{ path = ... }` | monorepo 内开发、SDK 未发布、调试 SDK 源码 |
| `--registry` | `^版本` / `"版本"` | 独立插件工程、面向外部用户 |

---

## 注意事项

- **首次发布前**只能用相对路径模式（registry 上查不到 `^0.1.0`，`pnpm install` / `cargo build` 会失败）
- **crates.io 名称占用**：发布前用 `cargo search <crate名>` 检查；发布后 crate 名不可更改，改名需新 crate
- **crates.io 强制 license 字段**：`Cargo.toml` 必须含 `license`，缺失时 `cargo publish` 直接拒绝（两 crate 已配 `MIT`）
- **本地预演**：`cargo publish --dry-run` 会校验 license + 编译 + 打包，未提交的改动需 `--allow-dirty`（CI 里 tag 是干净 checkout，不需要）
- **token 安全**：token 泄露（如贴入聊天）后应在对应网站删除重建，并用 `gh secret set` 更新
- **workspace 成员发布**：desktop SDK 是 `bedcode-desktop` pnpm workspace 成员（`packages/*`），发布用 `pnpm --filter @binblink/plugin-sdk-desktop publish --access public`；`prepublishOnly` 会自动执行 build + test
- **构建产物不入库**：`dist/` 已被 `.gitignore` 忽略，npm 发布走 `files` 白名单（dist/bin[/template]/dev-shell），CI 中 checkout 后由 `prepublishOnly` 现场构建
- **dev-shell 随包发布**：两个 SDK 的 `files` 均含 `dev-shell/`（浏览器开发环境，自包含 vite 工程，源码随包分发）；其 `node_modules` 不入包，插件开发者首次运行 `bedcode-plugin dev` 时 CLI 自动在 dev-shell 内执行 `pnpm install`（打印提示后自动安装，无需手动干预）

---

## 相关文件

| 文件 | 作用 |
|------|------|
| `.github/workflows/sdk-publish.yml` | 发布流水线 |
| `bedcode-desktop/packages/plugin-sdk-desktop/` | 桌面端 SDK（npm 包 + rust crate） |
| `bedcode-mobile/packages/plugin-sdk-mobile/` | 移动端 SDK（npm 包 + rust crate + 插件模板） |
| `bedcode-mobile/packages/plugin-sdk-mobile/bin/cli.js` | `create` 命令（`--registry` 模式） |
