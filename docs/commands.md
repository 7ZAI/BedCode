# BedCode 常用命令参考

项目已拆分为 `bedcode-desktop/` 和 `bedcode-mobile/` 两个独立 Tauri 项目，命令需在对应目录下执行。

---

## 自适应构建（跨平台，可选包装器）

构建/测试命令启动前自动采样系统资源（CPU 负载 / 可用内存 / swap 抖动），按双指标
分档注入编译参数：CPU 空闲且内存充裕 → 并行；任一指标紧张 → 降档串行（防 OOM 优先）。
内存为硬约束：cargo jobs 恒 ≤ min(核数, 可用内存GiB ÷ 1.5)。

```bash
# 在对应应用目录执行（脚本位于仓库根 scripts/，cwd 继承保证子命令在应用目录运行），
# -- 之后接任意原生命令，不改变原命令行为
cd bedcode-desktop && node ../scripts/adaptive-run.mjs -- pnpm run tauri:build
cd bedcode-mobile && node ../scripts/adaptive-run.mjs -- pnpm run tauri:android:build
cd bedcode-desktop/src-tauri && node ../../scripts/adaptive-run.mjs -- cargo test

# 仓库根目录跑根测试命令
node scripts/adaptive-run.mjs -- pnpm run test:run
```

注入的编译参数（env，自动透传给 cargo / gradle / Node）：

| 变量 | 说明 |
| --- | --- |
| `CARGO_BUILD_JOBS` | cargo 并行数（覆盖根 `.cargo/config.toml` 的 `jobs=4`） |
| `GRADLE_OPTS` | 追加/替换 `-Dorg.gradle.workers.max=N`（gradle 工作线程） |
| `NODE_OPTIONS` | 原位替换/追加 `--max-old-space-size`，保留其他参数 |

档位：`parallel`（cargo jobs 全开 / gradle 4 worker / Node 堆 4G）、`balanced`（jobs≤2 /
worker 2 / 堆 1.5G）、`serial`（jobs=1 / worker 1 / 堆 1G）。

覆盖与逃生阀（env，`BEDCODE_*` 前缀）：

| 变量 | 作用 |
| --- | --- |
| `BEDCODE_BUILD_PROFILE=parallel \| balanced \| serial \| auto` | 手动强制档位（默认 auto） |
| `BEDCODE_ADAPTIVE=0` | 完全禁用自适应，行为等同原生命令 |
| `BEDCODE_JOBS_PER_GIB=1.5` | 每 GiB 可用内存的 rustc 并发预算 |

平台：Linux 完整指标（/proc）；macOS / Windows 经 `os.loadavg` /
PowerShell `Win32_Processor.LoadPercentage`（PowerShell 不可用时仅按内存分档，多保守一档）。
测试：`pnpm run test:run`（仓库根目录，`node --test` 零依赖）。

## bedcode-desktop

### 开发模式

```bash
cd bedcode-desktop

# 启动前端 Vite 开发服务器（浏览器预览）
pnpm run dev

# 启动 Tauri 桌面端开发模式（含热更新）
pnpm run tauri:dev

# 仅 Rust 编译检查
cd src-tauri && cargo check
```

### 前端构建

```bash
cd bedcode-desktop

# 完整构建（TypeScript 类型检查 + Vite 打包）
pnpm run build

# 快速构建（跳过类型检查）
pnpm run build:fast
```

### 桌面端打包

```bash
cd bedcode-desktop

# 构建生产版本安装包（默认全部目标）
# Windows → NSIS 安装包 (.exe)
# macOS → DMG 镜像
# Linux → .deb（tauri.conf.json 的 bundle.targets 为 ["nsis", "deb"]）
pnpm run tauri:build

# 自适应构建（按当前资源分档并行度，不改变默认构建行为；见「自适应构建」章节）
node ../scripts/adaptive-run.mjs -- pnpm run tauri:build

# 仅构建 Linux DEB 安装包（--bundles 后的参数原样透传给 tauri CLI）
pnpm run tauri:build -- --bundles deb
```

**安装包输出路径：**
```
bedcode-desktop/src-tauri/target/release/bundle/nsis/BedCode_2.1.0_x64-setup.exe
bedcode-desktop/src-tauri/target/release/bundle/deb/BedCode_2.1.0_amd64.deb
```

> 构建成功后 `scripts/tauri-build.js` 会把安装包重命名为带 release 标记的格式（与移动端 APK 命名风格一致）：`BedCode-2.1.0-release-x64-setup.exe` / `BedCode-2.1.0-release-amd64.deb`

### 插件构建与打包

插件位于 `plugins/`（ai-chatbox、file-transfer、session）。构建时各插件先自行编译前端（Vite）与 Rust 后端（WASM），再由脚本把产物复制到 `src-tauri/resources/plugins/desktop/`，随桌面端安装包一起分发。

```bash
cd bedcode-desktop

# 构建全部插件（3 个：ai-chatbox / file-transfer / session）
pnpm run plugins:build

# 构建指定插件（--plugin 接插件 id）
node scripts/plugin-build.js --plugin com.bedcode.ai-chatbox
node scripts/plugin-build.js --plugin com.bedcode.session
node scripts/plugin-build.js --plugin com.bedcode.file-transfer

# 仅构建默认插件（com.bedcode.session）
pnpm run plugins:build:release

# 插件开发模式（watch，默认 com.bedcode.session）
pnpm run plugins:dev

# 指定插件开发模式
node scripts/plugin-dev.js --plugin com.bedcode.session
```

**前置条件**（Rust WASM 编译目标）：

```bash
rustup target add wasm32-unknown-unknown
```

**单插件内部命令**（`cd plugins/<name>`）：

```bash
pnpm run build                 # 完整构建：Vite + cargo(WASM) + 复制产物
pnpm run dev                   # 开发模式（build.js --watch）
pnpm run build:frontend       # 仅前端（Vite）
pnpm run build:rust           # 仅 Rust WASM 后端
node scripts/build.js --frontend-only  # 仅前端并复制产物
node scripts/build.js --rust-only      # 仅 Rust 并复制产物

# 浏览器开发环境（SDK Dev Shell，前端 HMR 实时预览，无需打包）
# 需先构建 SDK：cd bedcode-desktop/packages/plugin-sdk-desktop && pnpm run build
pnpm exec bedcode-plugin-desktop dev   # 或 pnpm add -D @binblink/bedcode-plugin-sdk-desktop 后在插件目录运行
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

**两端插件统一打包（zip 分发包，release 独立产物）**：

```bash
# 构建两端全部插件（前端 + WASM）并为每个插件各打一个 zip（仓库根目录执行）
node scripts/package-plugins.mjs

# 只看将打包的插件清单（不构建不打包）
node scripts/package-plugins.mjs --list

# 只打包一端：--target desktop | mobile | all（默认 all）
node scripts/package-plugins.mjs --target mobile

# 只打包指定插件（--only 忽略配置列表；--plugin 追加；--exclude 排除；
# 同名插件两端自动匹配）
node scripts/package-plugins.mjs --only agent-hub
node scripts/package-plugins.mjs --plugin file-transfer --exclude ai-chatbox

# 跳过构建直接打包已有产物；指定 zip 版本号；只构建收集产物、不打 zip
node scripts/package-plugins.mjs --skip-build --version 2.1.0
node scripts/package-plugins.mjs --no-zip
```

- **插件列表**：默认 `scripts/plugin-package-list.json`
  （desktop: `agent-hub`/`ai-chatbox`/`file-transfer`/`session`，
  mobile: `ai-chatbox`/`auto-task`/`file-transfer`——移动端任务插件仍是独立实现），增删插件改该文件即可；
  也可用 `--config <file>` 换列表文件
- **产物**：`dist/plugin-packages/<target>/<plugin-id>.zip`（一个插件一个 zip，zip 根 = 插件文件，
  与移动端 SDK `bedcode-plugin package` 分发格式一致）；`--out <dir>` 可改输出目录
- **CI**：`.github/workflows/release.yml` 的 `package-plugins` job 构建并上传全部插件 zip
  到 release（详见 `docs/knowledge/release-workflow.md`）

**两端 SDK 统一打包（npm tarball + crates.io 产物，release 独立附件）**：

```bash
# 构建两端 SDK（TS 构建 + vitest + cargo check）并打包 npm / cargo 产物（仓库根目录执行）
node scripts/package-sdks.mjs

# 只看将打包的 SDK 与版本（不构建不打包）
node scripts/package-sdks.mjs --list

# 只打包一端：--target desktop | mobile | all（默认 all）
node scripts/package-sdks.mjs --target desktop

# 跳过 vitest / 跳过构建仅重新打包 / 追加 wasm32 guest 编译检查（CI 默认开启）
node scripts/package-sdks.mjs --skip-tests
node scripts/package-sdks.mjs --skip-build
node scripts/package-sdks.mjs --rust-wasm
```

- **产物**：`dist/sdk-packages/<target>/`（按端分目录）：
  - `*.tgz`：`pnpm pack` 的 npm 包（TS 前端 + CLI + template + dev-shell）
  - `*.crate`：`cargo package --no-verify` 的 crates.io 包（per crate：desktop 含
    `bedcode-plugin-api` 与 `bedcode-plugin-api-macros`，mobile 含 `bedcode-plugin-api-mobile`）
  - `SHA256SUMS`：全部产物校验和
  - `<sdk>-<ver>.zip`：聚合包（上述产物 + README + WIT 契约 + index.md 说明）
- **版本**：产物以各自 SDK 自身版本命名（npm package.json 与 Cargo.toml 必须一致，
  校验不一致即失败），与应用版本无关
- **CI**：`.github/workflows/release.yml` 的 `package-sdks` job 构建并上传全部 SDK 产物到 release（详见 `docs/knowledge/release-workflow.md`）

**桌面端加载 / 卸载插件（zip 分发包）**：

```bash
# 打包脚本产出的 zip 可直接在桌面端「插件」页安装：
# 工具栏「加载插件」→ 选择 zip 包（产物 dist/plugin-packages/desktop/<id>.zip）
```

- 安装落盘：`app_data_dir/plugins/<id>/`（用户插件目录，独立于只读的内置目录）
- 加载校验：manifest 必填字段 + id 反向域名 + 路径穿越防护 + wasm 存在性（声明时），拒绝覆盖已安装同 id（升级需先卸载）
- 卸载：插件详情页「卸载」按钮（仅用户安装插件显示）→ 危险确认弹窗 → 删除插件所有数据（存储 + 激活状态 + 安装目录）

---

## bedcode-mobile

### 开发模式

```bash
cd bedcode-mobile

# 启动前端 Vite 开发服务器
pnpm run dev

# Android 热加载开发模式（真机/模拟器）
pnpm run tauri:android:dev

# Android 开发模式 + 电脑端日志落盘
# 普通 tauri:android:dev 只打控制台；本命令额外把 Tauri CLI 转发的 logcat
# 实时写入 .dev-logs/android-dev.YYYY-MM-DD.log（本地日期轮转，无 ANSI 码，可 grep）。
# 每次启动清空当天日志文件；Ctrl+C 退出前 flush 落盘；退出时打印过滤统计。
# 落盘内容默认过滤非业务噪音（wasmtime/cranelift·框架 tag·Gradle/Vite 构建进展），
# 业务与链路日志全保留；控制台与落盘同一套过滤（BEDCODE_LOG_NO_FILTER=1 可关闭看全量）
pnpm run tauri:android:dev:log

# 仅 Rust 编译检查
cd src-tauri && cargo check
```

### 前端构建

```bash
cd bedcode-mobile

# 完整构建
pnpm run build

# 快速构建
pnpm run build:fast
```

### Android 构建

```bash
cd bedcode-mobile

# 初始化 Android 项目（首次运行）
pnpm run tauri:android:init

# 构建 Debug APK（仅 arm64）
pnpm run tauri:android:build

# 自适应构建（按当前资源分档并行度，不改变默认构建行为；见「自适应构建」章节）
node ../scripts/adaptive-run.mjs -- pnpm run tauri:android:build

# 模拟器构建（x86_64）
pnpm run tauri:android:build:emulator

# 快速构建 Debug APK（仅 arm64，不优化）
pnpm run tauri:android:build:fast

# 构建多架构 Debug APK
pnpm run tauri:android:build:all

# 构建 Release APK（需配置签名）
pnpm exec tauri android build --release

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
pnpm run plugins:build

# 构建指定插件
node scripts/plugin-build.js --plugin com.bedcode.ai-chatbox

# 插件 + 主应用一起构建
pnpm run build:all
```

**单插件内部命令**（`cd plugins/<name>`，基于 `bedcode-plugin` SDK CLI）：

```bash
pnpm run dev       # = bedcode-plugin dev：浏览器开发环境（Dev Shell，HMR，无需真机）
pnpm run build     # = bedcode-plugin build：vite + cargo wasm32
pnpm run package   # = bedcode-plugin package：产出 dist/{id}.zip 插件包
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
cd <project> && pnpm run lint

# Prettier 格式化（前端）
cd <project> && pnpm run format

# TypeScript 类型检查
cd <project> && pnpm exec vue-tsc --noEmit

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
pnpm run test

# 单次运行
pnpm run test:run

# 自适应测试（按资源调 Node 堆，见「自适应构建」章节）
node ../scripts/adaptive-run.mjs -- pnpm run test:run

# 带覆盖率报告
pnpm run test:coverage

# UI 模式（浏览器查看测试结果）
pnpm run test:ui
```

---

## 端口管理

```bash
# Windows：查找 1420 端口被哪个进程占用
netstat -ano | findstr :1420

# Windows：终止占用 1420 端口的进程
taskkill /PID <PID> /F

# Windows PowerShell 版
Stop-Process -Id (Get-NetTCPConnection -LocalPort 1420).OwningProcess -Force
```

### BedCode dev 端口速查与一键释放（Linux / WSL）

BedCode 开发态各端口对应关系：

| 端口 | 占用者 | 说明 |
| ---- | ------ | ---- |
| `1420` | `bedcode-desktop` Vite | `pnpm run dev` / `pnpm run tauri:dev` |
| `1423` / `1424` | `bedcode-mobile` Vite | `pnpm run tauri:android:dev`；真机经 `adb reverse tcp:1423 tcp:1423` 转发 |
| `5173` | 移动端插件 Dev Shell | `cd plugins/<name> && pnpm run dev`（`bedcode-plugin dev`） |
| `5199` | `packages/plugin-sdk-mobile/dev-shell` | SDK 自带 Dev Shell |
| `5037` / `9333` | adb server | Android 调试桥（kill 后下次 adb 命令自动重启，不影响已连设备） |
| `36537`（动态） | Gradle daemon | Android 构建守护进程，杀掉后下次构建自动重启 |

```bash
# 1) 查看端口占用（确认 PID）
ss -tlnp | grep -E ':(1420|1423|1424|5173|5199|5037|9333)\b'

# 2) 确认进程身份（端口绑定未必是根进程，dev 树要整棵杀）
#    例如 1420 对应的 vite 父进程是 nohup 启动脚本；移动端 tauri android dev
#    本身不绑端口，但会拉起并看护 vite，需连父进程一起结束以免被重新拉起
ps -eo pid,ppid,etime,cmd | grep -E 'vite|tauri.js android|dev-shell|GradleDaemon' | grep -v grep

# 3) 整棵进程树温和关闭（SIGTERM → 2 秒 → SIGKILL 兜底）
#    按实际 PID 替换；惯用组合：桌面 vite + 启动脚本、tauri android dev 子树、
#    两个 Dev Shell 子树、adb server、Gradle daemon
TARGETS='<PID1> <PID2> ...'
for pid in $TARGETS; do kill -TERM "$pid" 2>/dev/null; done
sleep 2
for pid in $TARGETS; do ps -p "$pid" >/dev/null 2>&1 && kill -KILL "$pid"; done

# 4) 复核端口已释放（无输出即干净）
ss -tlnp | grep -E ':(1420|1423|1424|5173|5199|5037|9333)\b' || echo '全部端口已释放'
```

**说明：**

- 优先杀进程树根（如 `sh -c 'tauri android dev'` → `tauri.js android dev` → `vite`），避免 tauri CLI 看护逻辑把 vite 重新拉起
- adb server 与 Gradle daemon 都是自动重启型守护进程，杀掉不会破坏环境，只为释放端口/内存
- SIGKILL 兜底与「遗留 dev 进程清理」同因：异常退出后的进程事件循环可能已僵死，SIGTERM 不响应（见下方「遗留 dev 进程」节）

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
cd <project> && rm -rf node_modules && pnpm install

# 移动端：清理 Android 构建产物
cd bedcode-mobile/src-tauri/gen/android && ./gradlew clean
```

### 遗留 dev 进程 / 黑框窗口清理

`pnpm run tauri:dev` 意外退出（崩溃 / 强制关终端 / kill -9）后，`target/debug/bedcode-desktop` 主进程常常被 systemd 收养继续运行，表现为桌面上一片关不掉的黑框窗口；同时插件 `--watch` 进程也会一起残留。以下命令一次性清干净（Linux / WSL）。

```bash
# 1) 查看当前遗留（不匹配则无输出）
ps -eo pid,ppid,etime,stat,cmd | grep -E 'bedcode-desktop|vite.*--watch' | grep -v grep

# 2) 温和关闭所有遗留（先 SIGTERM，等 2 秒，未响应再 SIGKILL）
pkill -TERM -f 'target/debug/bedcode-desktop' ; \
pkill -TERM -f 'vite.js build --watch' ; \
sleep 2 ; \
pkill -KILL -f 'target/debug/bedcode-desktop' ; \
pkill -KILL -f 'vite.js build --watch'

# 3) 复核
ps -eo pid,etime,cmd | grep -E 'bedcode-desktop|vite.*--watch' | grep -v grep
echo "(空 = 干净)"
```

如果只想处理单个已知 PID（推荐用于窗口确认阶段），把上面 `pkill -f` 替换为：

```bash
TARGET=<PID>
kill -TERM $TARGET ; sleep 2
ps -p $TARGET >/dev/null && kill -KILL $TARGET
```

**为什么需要 SIGKILL 兜底**：`bedcode-desktop` 的 SIGTERM 处理链依赖窗口事件循环，dev 异常退出后事件循环可能已僵死，SIGTERM 不响应，必须 KILL。

**注意**：窗口里如果同时挂着 pi 子进程（Tauri 里的 pi 会话），会随父进程一并退出——**若该 pi 会话就是你要清理的目标，正常；否则先关那个终端**再杀主进程。

---

## 依赖管理

```bash
# 安装前端依赖
cd <project> && pnpm install

# 添加前端依赖
cd <project> && pnpm install <package-name>

# 添加开发依赖
cd <project> && pnpm install -D <package-name>

# 添加 Rust 依赖（编辑 Cargo.toml 后）
cd <project>/src-tauri && cargo build
```

---

## 开发快速参考

| 目标 | 目录 | 命令 | 产物 |
|------|------|------|------|
| 桌面端开发 | `bedcode-desktop` | `pnpm run tauri:dev` | 桌面窗口 + 热更新 |
| 桌面端打包 | `bedcode-desktop` | `pnpm run tauri:build` | `.exe` / `.dmg` / `.deb` |
| 桌面端打包（仅 DEB） | `bedcode-desktop` | `pnpm run tauri:build -- --bundles deb` | `.deb` |
| 插件构建（桌面） | `bedcode-desktop` | `pnpm run plugins:build` | 产物复制到 `src-tauri/resources/plugins/desktop/` |
| 插件构建（移动） | `bedcode-mobile` | `pnpm run plugins:build` | 产物复制到 `src-tauri/resources/plugins/mobile/` |
| Android 开发 | `bedcode-mobile` | `pnpm run tauri:android:dev` | 真机/模拟器 + 热更新 |
| Android 开发（日志落盘） | `bedcode-mobile` | `pnpm run tauri:android:dev:log` | logcat 写入 `.dev-logs/android-dev.*.log`（默认过滤非业务噪音，`BEDCODE_LOG_NO_FILTER=1` 关闭） |
| Android APK | `bedcode-mobile` | `pnpm run tauri:android:build` | `.apk` |
| Android 快速构建 | `bedcode-mobile` | `pnpm run tauri:android:build:fast` | Debug `.apk` |
| 前端测试 | `<project>` | `pnpm run test:run` | 终端输出 |
| Rust 测试 | `<project>/src-tauri` | `cargo test` | 终端输出 |
| Rust 检查 | `<project>/src-tauri` | `cargo check` | 编译检查 |

---

## pi session 归档（scripts/pi-session-archive.sh）

将本项目 `.pi/sessions/` 中**距离最新 session 超过 N 天**的 session jsonl 日志（含复合 session 目录）移动到 pi 安装目录的 session 归档区。归档文件夹以项目全路径命名（`/` 替换为 `-`，前后加 `--`，与 pi 自身约定一致）：

```
项目 /home/binblink/project/tauriProject/BedCode
  → ~/.pi/agent/sessions/--home-binblink-project-tauriProject-BedCode--
```

- **基准日期** = 本项目 `.pi/sessions/` 中最新 session 的时间戳（非今天），早于（基准 − N 天）的视为过期；默认 N=10
- 只处理顶层 `*.jsonl` 与 `YYYY-MM-DDThh-mm-ss-msZ_<ulid>` 形式的 session 目录；`sol-pi` / `subagent-artifacts` 等非 session 目录绝不触碰
- 目标已有同名条目时跳过并警告，绝不覆盖
- 脚本由 `scripts/` 位置推导项目根，天然只在项目范围内生效；可在任意目录用绝对路径执行

```bash
# 实际归档（默认 10 天）
scripts/pi-session-archive.sh

# 只预览不移动（推荐先跑）
scripts/pi-session-archive.sh -n

# 自定义阈值（如 30 天）
scripts/pi-session-archive.sh -d 30

# 覆盖 pi 安装目录（默认 ~/.pi/agent）
PI_AGENT_DIR=/custom/pi scripts/pi-session-archive.sh -n
```
