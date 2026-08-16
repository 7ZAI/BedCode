# BedCode 常用命令参考

项目已拆分为 `bedcode-desktop/` 和 `bedcode-mobile/` 两个独立 Tauri 项目，命令需在对应目录下执行。

---

## bedcode-desktop

### 开发模式

```bash
cd bedcode-desktop

# 启动前端 Vite 开发服务器（浏览器预览）
npm run dev

# 启动 Tauri 桌面端开发模式（含热更新）
npm run tauri:dev

# 仅 Rust 编译检查
cd src-tauri && cargo check
```

### 前端构建

```bash
cd bedcode-desktop

# 完整构建（TypeScript 类型检查 + Vite 打包）
npm run build

# 快速构建（跳过类型检查）
npm run build:fast
```

### 桌面端打包

```bash
cd bedcode-desktop

# 构建生产版本安装包
# Windows → NSIS 安装包 (.exe)
# macOS → DMG 镜像
# Linux → AppImage / .deb
npm run tauri:build
```

**安装包输出路径：**
```
bedcode-desktop/src-tauri/target/release/bundle/nsis/BedCode_0.1.0_x64-setup.exe
```

### 插件构建与打包

插件位于 `plugins/`（ai-chatbox、auto-task、file-transfer）。构建时各插件先自行编译前端（Vite）与 Rust 后端（WASM），再由脚本把产物复制到 `src-tauri/resources/plugins/desktop/`，随桌面端安装包一起分发。

```bash
cd bedcode-desktop

# 构建全部插件（3 个）
npm run plugins:build

# 构建指定插件（--plugin 接插件 id）
node scripts/plugin-build.js --plugin com.bedcode.ai-chatbox
node scripts/plugin-build.js --plugin com.bedcode.auto-task
node scripts/plugin-build.js --plugin com.bedcode.file-transfer

# 仅构建默认插件（ai-chatbox）
npm run plugins:build:release

# 插件开发模式（watch，默认 ai-chatbox）
npm run plugins:dev

# 指定插件开发模式
node scripts/plugin-dev.js --plugin com.bedcode.auto-task
```

**前置条件**（Rust WASM 编译目标）：

```bash
rustup target add wasm32-unknown-unknown
```

**单插件内部命令**（`cd plugins/<name>`）：

```bash
npm run build                 # 完整构建：Vite + cargo(WASM) + 复制产物
npm run dev                   # 开发模式（build.js --watch）
npm run build:frontend       # 仅前端（Vite）
npm run build:rust           # 仅 Rust WASM 后端
node scripts/build.js --frontend-only  # 仅前端并复制产物
node scripts/build.js --rust-only      # 仅 Rust 并复制产物

# 浏览器开发环境（SDK Dev Shell，前端 HMR 实时预览，无需打包）
# 需先构建 SDK：cd bedcode-desktop/packages/plugin-sdk-desktop && npm run build
npx bedcode-plugin-desktop dev   # 或 npm i -D @binblink/plugin-sdk-desktop 后在插件目录运行
# 首次运行自动安装 dev-shell 依赖，浏览器打开 http://localhost:5173
# 详见 ../bedcode-desktop/plugin-dev-desktop.md
```

**产物输出路径**（随桌面端安装包分发）：

```
bedcode-desktop/src-tauri/resources/plugins/desktop/{plugin-id}/
├── index.js        # 前端打包产物
├── plugin.json     # 插件清单
└── {lib}.wasm      # Rust WASM 后端
```

---

## bedcode-mobile

### 开发模式

```bash
cd bedcode-mobile

# 启动前端 Vite 开发服务器
npm run dev

# Android 热加载开发模式（真机/模拟器）
npm run tauri:android:dev

# Android 开发模式 + 电脑端日志落盘
# 普通 tauri:android:dev 只打控制台；本命令额外把 Tauri CLI 转发的 logcat
# 实时写入 .dev-logs/android-dev.YYYY-MM-DD.log（UTC 日期，无 ANSI 码，可 grep）。
# 每次启动清空当天日志文件（跨天按 UTC 轮转新文件）；Ctrl+C 退出前 flush 落盘
npm run tauri:android:dev:log

# 仅 Rust 编译检查
cd src-tauri && cargo check
```

### 前端构建

```bash
cd bedcode-mobile

# 完整构建
npm run build

# 快速构建
npm run build:fast
```

### Android 构建

```bash
cd bedcode-mobile

# 初始化 Android 项目（首次运行）
npm run tauri:android:init

# 构建 Debug APK（仅 arm64）
npm run tauri:android:build

# 模拟器构建（x86_64）
npm run tauri:android:build:emulator

# 快速构建 Debug APK（仅 arm64，不优化）
npm run tauri:android:build:fast

# 构建多架构 Debug APK
npm run tauri:android:build:all

# 构建 Release APK（需配置签名）
npx tauri android build --release

# 使用 Android Studio 打开项目
# File → Open → bedcode-mobile/src-tauri/gen/android
```

**APK 输出路径：**
```
bedcode-mobile/src-tauri/gen/android/app/build/outputs/apk/
├── debug/app-universal-debug.apk
└── release/app-release.apk
```

### 插件构建与打包

```bash
cd bedcode-mobile

# 构建全部插件（扫描 plugins/，产物复制到 APK 资源目录）
npm run plugins:build

# 构建指定插件
node scripts/plugin-build.js --plugin com.bedcode.ai-chatbox

# 插件 + 主应用一起构建
npm run build:all
```

**单插件内部命令**（`cd plugins/<name>`，基于 `bedcode-plugin` SDK CLI）：

```bash
npm run dev       # = bedcode-plugin dev：浏览器开发环境（Dev Shell，HMR，无需真机）
npm run build     # = bedcode-plugin build：vite + cargo wasm32
npm run package   # = bedcode-plugin package：产出 dist/{id}.zip 插件包
```

> Dev Shell 用 mock 宿主 + 移动端页面骨架在浏览器预览插件前端，WASM 后端命令需真机验证；详见 `../bedcode-mobile/plugin-dev-mobile.md`。

**产物输出路径**：

```
bedcode-mobile/src-tauri/resources/plugins/mobile/{plugin-id}/   # 进 APK 资源（首启解压）
plugins/{name}/dist/{plugin-id}.zip                              # 可分发的插件包
```

---

### Android 真机安装与调试

```bash
# 检查已连接的设备
adb devices

# 安装 APK 到真机（arm64）
adb install bedcode-mobile/src-tauri/gen/android/app/build/outputs/apk/arm64/debug/app-arm64-debug.apk
            
# 安装 APK 到模拟器
adb install bedcode-mobile/src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk

# 覆盖安装（保留数据）
adb install -r bedcode-mobile/src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk

# 卸载应用
adb uninstall com.bedcode.mobile

# 获取日志
adb logcat -s BedCode:*

# 保存日志到文件
adb logcat > bedcode_log.txt

# 重启 adb 服务（设备 offline 时）
adb kill-server && adb start-server
```

**真机连接常见问题：**

| 现象 | 解决 |
|------|------|
| `adb devices` 为空 | 手机上开启 USB 调试，换根数据线 |
| 状态显示 `unauthorized` | 手机上点"允许 USB 调试"弹窗 |
| 状态显示 `offline` | `adb kill-server && adb start-server` |
| 安装报 `INSTALL_FAILED_UPDATE_INCOMPATIBLE` | 先 `adb uninstall com.bedcode.mobile` 再安装 |

---

## Rust 后端编译（通用）

```bash
# 编译 Debug 模式
cd <project>/src-tauri && cargo build

# 编译 Release 模式（启用 LTO、Strip 等优化）
cd <project>/src-tauri && cargo build --release

# 仅检查语法和类型（不生成产物，速度最快）
cd <project>/src-tauri && cargo check

# 运行 Clippy 静态分析
cd <project>/src-tauri && cargo clippy

# 格式化 Rust 代码
cd <project>/src-tauri && cargo fmt

# 运行 Rust 测试
cd <project>/src-tauri && cargo test

# 更新 Rust 依赖
cd <project>/src-tauri && cargo update
```

> `<project>` 替换为 `bedcode-desktop` 或 `bedcode-mobile`

---

## 代码质量

```bash
# ESLint 检查（前端）
cd <project> && npm run lint

# Prettier 格式化（前端）
cd <project> && npm run format

# TypeScript 类型检查
cd <project> && npx vue-tsc --noEmit

# Rust Clippy 检查
cd <project>/src-tauri && cargo clippy

# Rust 代码格式化检查
cd <project>/src-tauri && cargo fmt --check
```

---

## 前端测试（Vitest）

```bash
cd <project>

# 监听模式（开发时使用）
npm run test

# 单次运行
npm run test:run

# 带覆盖率报告
npm run test:coverage

# UI 模式（浏览器查看测试结果）
npm run test:ui
```

---

## 端口管理

```bash
# 查找 1420 端口被哪个进程占用
netstat -ano | findstr :1420

# 终止占用 1420 端口的进程（Windows）
taskkill /PID <PID> /F

# PowerShell 版
Stop-Process -Id (Get-NetTCPConnection -LocalPort 1420).OwningProcess -Force
```

---

## 清理

```bash
# 桌面端：清理 Rust 编译缓存
cd bedcode-desktop/src-tauri && cargo clean

# 移动端：清理 Rust 编译缓存
cd bedcode-mobile/src-tauri && cargo clean

# 清理前端构建产物
cd <project> && rm -rf dist/

# 清理 node_modules 重新安装
cd <project> && rm -rf node_modules && npm install

# 移动端：清理 Android 构建产物
cd bedcode-mobile/src-tauri/gen/android && ./gradlew clean
```

---

## 依赖管理

```bash
# 安装前端依赖
cd <project> && npm install

# 添加前端依赖
cd <project> && npm install <package-name>

# 添加开发依赖
cd <project> && npm install -D <package-name>

# 添加 Rust 依赖（编辑 Cargo.toml 后）
cd <project>/src-tauri && cargo build
```

---

## 开发快速参考

| 目标 | 目录 | 命令 | 产物 |
|------|------|------|------|
| 桌面端开发 | `bedcode-desktop` | `npm run tauri:dev` | 桌面窗口 + 热更新 |
| 桌面端打包 | `bedcode-desktop` | `npm run tauri:build` | `.exe` / `.dmg` / `.AppImage` |
| 插件构建（桌面） | `bedcode-desktop` | `npm run plugins:build` | 产物复制到 `src-tauri/resources/plugins/desktop/` |
| 插件构建（移动） | `bedcode-mobile` | `npm run plugins:build` | 产物复制到 `src-tauri/resources/plugins/mobile/` |
| Android 开发 | `bedcode-mobile` | `npm run tauri:android:dev` | 真机/模拟器 + 热更新 |
| Android 开发（日志落盘） | `bedcode-mobile` | `npm run tauri:android:dev:log` | logcat 写入 `.dev-logs/android-dev.*.log` |
| Android APK | `bedcode-mobile` | `npm run tauri:android:build` | `.apk` |
| Android 快速构建 | `bedcode-mobile` | `npm run tauri:android:build:fast` | Debug `.apk` |
| 前端测试 | `<project>` | `npm run test:run` | 终端输出 |
| Rust 测试 | `<project>/src-tauri` | `cargo test` | 终端输出 |
| Rust 检查 | `<project>/src-tauri` | `cargo check` | 编译检查 |
