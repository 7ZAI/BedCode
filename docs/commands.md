# BedCode 命令参考

> **本文件是命令与字眼的唯一事实源**（AGENTS.md §3 只指向这里）。命令与实际不符时**先修本文件**再执行。

## 速查表

| 目标 | 目录 | 命令 |
| --- | --- | --- |
| 桌面端开发 | `bedcode-desktop` | `pnpm run tauri:dev` |
| 桌面端打包 | `bedcode-desktop` | `pnpm run tauri:build`（`-- --bundles deb` 只出 DEB） |
| 桌面端前端构建 | `bedcode-desktop` | `pnpm run build`（`build:fast` 跳过类型检查） |
| Android 开发 | `bedcode-mobile` | `pnpm run tauri:android:dev`（`:dev:log` 额外落盘 logcat） |
| Android Debug APK | `bedcode-mobile` | `pnpm run tauri:android:build`（实际产出 **release** APK；`:build:fast` / `:build:emulator` / `:build:all`） |
| 真机无线调试 | `bedcode-mobile` | `adb pair` / `adb connect` 后 `pnpm run tauri:android:dev`——详见 §4「无线 ADB 真机调试」 |
| 前端测试 | 各端根目录 | `pnpm run test:run` —— **禁 `pnpm run test`** |
| Rust 测试 | 见 §2 | `cargo test`（**每个 crate 在自己根目录跑**） |
| 跨端互连测试 | `cross-end-tests` | `cargo test` |
| 桌面 wasm 应用构建 | `bedcode-desktop/wasm-apps/<app-id>` | `pnpm run build` |
| 移动插件构建 | `bedcode-mobile/wasm-apps/<app-id>` | `pnpm run build` |
| 桌面插件全量构建 | `bedcode-desktop` | `pnpm run plugins:build` |
| 移动插件全量构建 | `bedcode-mobile` | `pnpm run plugins:build` |
| 代码质量 | 仓库根 / 各端 | `pnpm exec eslint .` · 各端 `pnpm run lint` · `src-tauri` 内 `cargo clippy` |

---

## 1. 通用约定

- **包管理器只有 pnpm**，全局禁止 npm。仓库根与两端各有独立 `pnpm-lock.yaml`。
- **仓库根没有 vitest 配置**：前端测试**必须在端目录执行**（`cd bedcode-desktop && pnpm run test:run`）。
- **Rust 无根 workspace**：没有「在根目录跑 cargo test」这种命令，每个 crate（两端宿主、wasm 应用、移动插件、`cross-end-tests`）都在**自己的根目录**执行，测试也各自跑。
- **sccache 是全仓 Rust 构建前置**（根 `.cargo/config.toml` 强制 `rustc-wrapper`），未装则一切 cargo 命令直接失败——这是显性失败设计，装法与缓存见 `docs/knowledge/build-process.md`。
- **可选包装器 `adaptive-run.mjs`**（仓库根 `scripts/`）：采样 CPU / 可用内存 / swap 后按档位注入 `CARGO_BUILD_JOBS`、`GRADLE_OPTS`、`NODE_OPTIONS`，防 OOM 优先；`--` 之后原样接任意命令，不改变其行为。

  ```bash
  cd bedcode-desktop && node ../scripts/adaptive-run.mjs -- pnpm run tauri:build
  cd bedcode-desktop/src-tauri && node ../../scripts/adaptive-run.mjs -- cargo test
  ```

  档位 `parallel` / `balanced` / `serial`（默认自动）。逃生阀：`BEDCODE_BUILD_PROFILE=parallel|balanced|serial|auto` 强制档位、`BEDCODE_ADAPTIVE=0` 完全关闭、`BEDCODE_JOBS_PER_GIB=1.5` 每 GiB 内存的并发预算。

---

## 2. 测试

### 2.1 前端（Vitest）

```bash
cd <端目录>            # 必须在端目录，仓库根无配置

pnpm run test:run      # 单次运行（= vitest run，跑完退出）—— 唯一允许的跑法
pnpm exec vitest run <测试文件路径>    # 开发中的针对性过滤
pnpm run test:coverage                # 覆盖率报告
pnpm run test:ui                      # 浏览器 UI
```

**禁 `pnpm run test`**：它是 vitest watch，挂起不退出，CI / agent 环境会直接卡死。

### 2.2 Rust

```bash
cd bedcode-desktop/src-tauri && cargo test      # 桌面宿主
cd bedcode-mobile/src-tauri  && cargo test      # 移动宿主

cd packages/bedcode-wasm-core && cargo test       # 插件机制整核 crate（2026-10-08 迁根至仓库根 packages/）
cd packages/bedcode-server-base && cargo test        # 任一能力域 / 传输面 crate 同理（仓库根 packages/，无根 workspace，各自根目录）

cd bedcode-desktop/wasm-apps/<app-id>/rust && cargo test   # wasm 应用（仅 terminal-session 另有 test:rust 脚本，其余直接跑 cargo test）
cd bedcode-mobile/wasm-apps/<app-id>/rust && cargo test      # 移动插件

cargo test <名称前缀>                            # 针对性过滤，cwd = 被测 crate 根
```

**宿主 `cargo test` 不覆盖** wasm 应用 / 移动插件 crate，也不覆盖未纳入 vitest include 的目录（以两端 `vitest.config.ts` 的 include 为准）——落在那些目录须自行运行并在交付说明里写明。

### 2.3 跨端互连（`cross-end-tests/`）

桌面真实服务器 + 移动真实客户端代码**同进程互连、零 mock**——两端各自的 mock 各自自洽，真实互连才是跨端契约的真正门禁。

```bash
cd cross-end-tests && cargo test                        # 全量
cd cross-end-tests && cargo test --test terminal_ws_flow # 单场景（每个场景 = 独立测试二进制）
```

前置：桌面随包 wasm 产物已构建（`cd bedcode-desktop && pnpm run plugins:build`）；**缺产物时测试显性失败，不静默 skip**。

### 2.4 两个必踩的坑

1. **wasm fixture 构建依赖 rustup shim**：宿主 wasm 闭环用例会在测试内 `cargo build --target wasm32-wasip3` 构建 fixture 并注入 `RUSTUP_TOOLCHAIN`（真源 `scripts/wasip3-toolchain.sh`）。因此**必须用 rustup shim 的 `cargo`（`~/.cargo/bin/cargo`）**；把 `~/.rustup/toolchains/*/bin` 前置进 PATH 会让注入失效（raw toolchain cargo 忽略该变量），依赖 fixture 的用例会成批红（`Test component WASM build failed`），**与代码无关**。
2. **跑完测试清理进程**：关闭测试开启的后台进程 / 监听端口（cargo 测试 spawn 的 mock server、vitest worker 残留、gradle daemon），否则会占端口与 CPU。

---

## 3. 桌面端（`bedcode-desktop/`）

```bash
pnpm run dev          # 仅前端 Vite（浏览器预览）
pnpm run tauri:dev    # 桌面端开发（含热更新）
pnpm run build        # 前端完整构建（vue-tsc 类型检查 + vite 打包）
pnpm run build:fast   # 跳过类型检查
pnpm run tauri:build  # 打包安装包；-- --bundles deb 只出 DEB
cd src-tauri && cargo check
```

- **`pnpm run tauri:dev -- --no-watch`**：关掉插件前端 watch。默认开着；插件 watch 会把 vite 产物复制进 `src-tauri/`，触发 Tauri 全量重启宿主并清当日日志。**必须经 pnpm 转发**（裸 `node scripts/dev-run.js` 在缺 `pnpm_execpath` 时 Linux ENOENT）。关掉后改插件前端自行 `cd wasm-apps/<app-id> && pnpm run build`。
- **安装包输出**：`src-tauri/target/release/bundle/{nsis/*.exe,deb/*.deb}`；构建成功后 `scripts/tauri-build.js` 会重命名为 `BedCode-<版本>-release-<arch>-*`。
- **updater 签名**：未配置 `TAURI_SIGNING_PRIVATE_KEY(_FILE)` / `.env` 时自动禁用升级包，本地构建无需私钥（发布见 `docs/knowledge/release-workflow.md`）。

---

## 4. 移动端（`bedcode-mobile/`）

```bash
pnpm run dev                      # 仅前端 Vite
pnpm run tauri:android:dev        # Android 热加载开发（真机 / 模拟器）
pnpm run tauri:android:dev:log    # 同上 + logcat 落盘 .dev-logs/android-dev.YYYY-MM-DD.log
pnpm run build                    # 前端完整构建
pnpm run tauri:android:init       # 初始化 Android 工程（首次）
pnpm run tauri:android:build      # Release APK（arm64，默认 profile）
pnpm run tauri:android:build:fast # 快速 Release APK
pnpm run tauri:android:build:emulator   # 模拟器 APK（x86_64）
pnpm run tauri:android:build:all  # 多架构 Debug APK
pnpm exec tauri android build --release   # Release APK（需签名）
cd src-tauri/gen/android && ./gradlew :app:compileUniversalDebugKotlin   # 仅编译 Kotlin
cd src-tauri && cargo check
```

- **`:dev:log`** 每次启动清空当天日志、Ctrl+C 前 flush；默认过滤非业务噪音（wasmtime / 框架 tag / 构建进展），`BEDCODE_LOG_NO_FILTER=1` 看全量。详见 `docs/knowledge/logging.md`。
- **`:dev:log` 的颜色**：控制台**原样透传**子进程输出（ANSI 颜色与 `\r` 进度条覆盖都保留），只有落盘文件是纯文本。但子进程 stdout 接的是管道不是 TTY，tauri CLI / cargo / Gradle 会判定「非交互」而不产色——想让上游真的吐色，加 `FORCE_COLOR=1 CLICOLOR_FORCE=1 CARGO_TERM_COLOR=always pnpm run tauri:android:dev:log`（管道透明、过滤与落盘行为不变）。
- **APK 输出**：`src-tauri/gen/android/app/build/outputs/apk/{debug,universal/debug}/`。Android Studio 打开 `src-tauri/gen/android`。
- **签名唯一真源 = 仓库根 `bedcode.keystore`**。

### 真机安装与调试

```bash
adb devices
adb install <apk 路径>            # 覆盖安装加 -r
adb uninstall com.bedcode.mobile
adb logcat -s BedCode:*
adb kill-server && adb start-server   # 设备 offline 时
```

| 现象 | 处理 |
| --- | --- |
| `adb devices` 为空 | 手机开 USB 调试，换数据线 |
| `unauthorized` | 手机上点「允许 USB 调试」 |
| `offline` | `adb kill-server && adb start-server` |
| `INSTALL_FAILED_UPDATE_INCOMPATIBLE` | 先 `adb uninstall` 再装 |

移动端没有日志时的排查见 `docs/knowledge/adb-fd0-bug.md`。

### 无线 ADB 真机调试（免数据线）

走的是 adb 通道，**开发命令与 USB 完全一致**（不换端口、不改配置），只多了配对这一步。

前置：**手机与电脑同一局域网**（手机 WiFi 必须开着、不能只有移动数据；企业网/访客 WiFi 常开 AP 隔离，连不上）；手机开「开发者选项 → USB 调试 + 无线调试」。先确认宿主机放行 adb server——设备是**主动回连**电脑的 `5037`（不是反向），挡住就永远 `failed to connect`：

```bash
ss -tlnp | grep 5037
sudo ufw allow 5037/tcp        # Debian/Ubuntu；firewalld 用 sudo firewall-cmd --add-port=5037/tcp
```

**方式 A：Android 11+ 无线调试**（推荐）

手机：关于手机页连点版本号 7 次 → 开发者选项 → 打开「USB 调试」「无线调试」→ 无线调试 →「使用配对码配对设备」，记下 **IP:配对端口** 与 6 位配对码；`adb pair` 后回无线调试主界面取 **IP 地址和端口**（与配对端口**不同**）：

```bash
adb pair 192.168.1.23:37123        # 提示时输入手机上的 6 位配对码（一次有效，约 60 秒过期）
adb connect 192.168.1.23:40561     # 用主界面那个端口，不是配对端口
adb devices                        # 期望 192.168.1.23:40561  device
adb mdns services                  # 可选：局域网自动发现无线调试端点（mDNS 被拦时用不了）
```

**方式 B：`adb tcpip 5555`**（Android 10 及以下、或机型没有无线调试开关）

```bash
adb devices                                   # 先用数据线连上并在手机上点授权
adb tcpip 5555
adb shell ip route                            # 从 wlan0 行的 src 读手机局域网 IP
adb shell ifconfig wlan0 | grep 'inet '       # 老 Android 没有 ip route 时用
# 拔掉数据线后
adb connect 192.168.1.23:5555
adb devices
```

**跑起来**（无线下热更新照常生效，无需把 vite 暴露到局域网）：

```bash
cd bedcode-mobile
adb devices                     # 先确认无线设备在册
pnpm run tauri:android:dev      # 每次启动预检：装 adb fd0 shim + 查 devUrl 端口 + 重建 adb reverse
pnpm run tauri:android:dev:log  # 同上 + logcat 落盘 .dev-logs/
```

生效原因：`scripts/dev-run.js` 给 CLI 传 `--host 127.0.0.1`，devUrl 固定成设备回环的 `http://localhost:1423`，再由它自己的 `precheckAdbReverse()` 建 `adb reverse tcp:1423 tcp:1423`——隧道跑在 adb 连接上（无线同样是 adb），因此不依赖宿主机 IP、不怕 DHCP 换地址。

**USB 与无线同时在册时选设备**：`tauri android dev` 支持位置参数 `[DEVICE]`，但 `dev-run.js` 固定了参数、不透传位置参数。二选一：

```bash
adb disconnect 192.168.1.23:5555     # 最省事：只留一台设备在册
# 或用 dev-run.js 的定制入口把序列号钉死（--host-cmd 字符串按空格切分，要自带完整子命令）：
pnpm run tauri:android:dev -- --host-cmd "pnpm run tauri android dev --host 127.0.0.1 192.168.1.23:5555"
```

> `:dev:log` 内部固定调 `pnpm run tauri:android:dev`，不透传 `--host-cmd`；要钉死设备就用 `:dev` + 另开 `adb logcat`。

**多设备并行操作**用 `-s` 钉序列号：

```bash
adb -s 192.168.1.23:5555 shell getprop ro.product.model     # 确认是哪台
adb -s 192.168.1.23:5555 logcat -s BedCode:*
adb -s 192.168.1.23:5555 reverse --list                    # 看热更新隧道
adb -s 192.168.1.23:5555 install -r <apk 路径>
adb -s 192.168.1.23:5555 reverse --remove-all && adb -s 192.168.1.23:5555 reverse tcp:1423 tcp:1423
```

| 现象 | 处理 |
| --- | --- |
| `adb pair` / `adb connect` 报 `failed to connect`、超时 | 宿主机防火墙挡了 `5037`（见上）；或不同网段 / AP 隔离 / 电脑挂着 VPN |
| `adb connect` 成功但 `adb devices` 显示 `offline` | `adb kill-server && adb start-server` 后重新 `adb connect`（server 重启会清掉所有无线连接）；手机锁屏休眠会断无线调试，调试期关掉「锁屏后休眠」或保持常亮 |
| `adb pair` 提示失败 / 端口拒绝连接 | 配对码与配对端口约 60 秒过期，回「无线调试」主界面重新取一对 |
| 手机重启后连不上 | 无线调试开关与端口会变（多数 ROM 重启即关），重新开启并 `adb pair` + `adb connect` |
| 真机白屏 / `Failed to request http://localhost:1423` | `adb reverse` 隧道丢了（设备重连、切 WiFi、重启手机都不自动恢复）：重跑 `pnpm run tauri:android:dev`，或用上面 `reverse` 命令手工重建 |
| 改前端代码不热更 | 同上，隧道断了 |
| `INSTALL_FAILED_UPDATE_INCOMPATIBLE` | `adb -s <serial> uninstall com.bedcode.mobile` 后重装 |
| dev 会话没有日志 | 见 `docs/knowledge/adb-fd0-bug.md`；fd0 shim 由 `dev-run.js` 每次自愈，platform-tools 升级后重跑一次 dev 即可 |

---

## 5. wasm 应用与插件

桌面是 **`wasm-apps/<app-id>/`**（4 个：`agent-hub` / `ai-chatbox` / `file-transfer` / `terminal-session`），移动端是 **`wasm-apps/<app-id>/`**（3 个：`ai-chatbox` / `file-transfer` / `terminal-session`）。插件 id 形如 `com.bedcode.<name>`。

### 5.1 工具链（桌面 wasm32-wasip3）

桌面 wasm 应用统一编译到 **`wasm32-wasip3`**（由 pin 住的 nightly 提供 std），**不是 `wasm32-unknown-unknown`**。目标与 toolchain 由 `scripts/wasip3-toolchain.sh` 单一维护（含 `rustup target add wasm32-wasip3 --toolchain <pinned>`），脚本会按需补装。背景与移动端待决策项见 `docs/knowledge/wasip3-toolchain.md`。

### 5.2 构建与测试

```bash
# 端内全量构建（复制产物进 src-tauri/resources/plugins/<end>/）
# 桌面 4 个 wasm 应用一次构建完（--all 遍历 scripts/plugin-build.js 的 PLUGINS 注册表，
# 名单单一真源，不在别处再抄一份；CI/release/test.yml 的 plugins:build 走的就是这条）
cd bedcode-desktop && pnpm run plugins:build       # agent-hub / ai-chatbox / file-transfer / terminal-session
cd bedcode-desktop && node scripts/plugin-build.js --plugin com.bedcode.agent-hub   # 单个应用（调试用）
cd bedcode-desktop && pnpm run plugins:dev         # watch 开发（默认 terminal-session）
cd bedcode-mobile  && pnpm run plugins:build
cd bedcode-mobile  && pnpm run build:all           # 插件 + 主应用
```

> 桌面 `pnpm tauri:dev` 会为**全部 4 个** wasm 应用起前端 watch（`scripts/dev-run.js`
> 的 `PLUGIN_WATCH_CMDS`），改前端自动重建并热重载；WASM 改动仍走
> `node scripts/build.js`（dev 会话启动时 `ensurePluginWasm()` 对缺失/陈旧产物自动补建）。

### 5.3 单个应用内部（`cd wasm-apps/<app-id>`）

```bash
# 桌面：node scripts/build.js 封装前端 + wasm + 产物复制
pnpm run build           # 完整构建
pnpm run dev             # watch 开发
pnpm run build:frontend  # 仅前端（vite build）
pnpm run build:rust      # 仅 Rust WASM 后端
node scripts/build.js --frontend-only / --rust-only
pnpm run test:rust       # rust/ 内 cargo test

# 移动：bedcode-plugin SDK CLI
pnpm run build           # vite + cargo wasm32
pnpm run dev             # 浏览器 Dev Shell（HMR，无需真机）
pnpm run package         # 产出 dist/<plugin-id>.zip
```

浏览器 Dev Shell 需要先构建 SDK（`cd packages/plugin-sdk-<end> && pnpm run build`），用法见 `bedcode-desktop/plugin-dev-desktop.md` 与 `bedcode-mobile/plugin-dev-mobile.md`。Dev Shell 只验前端，WASM 后端命令仍需真机。

### 5.4 产物落点

```text
bedcode-desktop/src-tauri/resources/plugins/desktop/<plugin-id>/{index.js,plugin.json,<lib>.wasm}
bedcode-mobile/src-tauri/resources/plugins/mobile/<plugin-id>/      # 进 APK 资源（首启解压）
bedcode-mobile/plugins/<name>/dist/<plugin-id>.zip                  # 可分发的插件包
dist/plugin-packages/<target>/<plugin-id>.zip                       # §5.5 的 zip 分发包
dist/sdk-packages/<target>/                                         # §5.5 的 SDK 产物
```

### 5.5 分发打包（仓库根执行）

```bash
node scripts/package-plugins.mjs     # 两端插件 zip（列表见 scripts/plugin-package-list.json）
node scripts/package-sdks.mjs        # 两端 SDK：npm tarball + crates.io crate + 聚合 zip
```

常用 flag（两脚本通用）：`--list` 只列不构建 · `--target desktop|mobile|all`（默认 all）· `--skip-build` 跳过构建 · `--only <plugin>` / `--plugin <p>` / `--exclude <p>`（插件脚本）· `--out <dir>` 改输出目录 · `--skip-tests`（SDK 脚本，跳过 vitest）· `--version <v>`（插件脚本，指定 zip 版本）。

CI 由 `.github/workflows/release.yml` 的 `package-plugins` / `package-sdks` job 执行并上传到 release，流程见 `docs/knowledge/release-workflow.md`。

插件 zip 可直接在桌面端「插件」页安装（加载插件 → 选 zip），落盘 `app_data_dir/plugins/<id>/`；加载校验 manifest 必填字段 + id 反向域名 + 路径穿越防护 + wasm 存在性，拒绝覆盖同 id（升级需先卸载），卸载会删除该插件全部数据。插件开发约束见 `docs/knowledge/plugin-development-checklist.md`。

---

## 6. Rust 与代码质量

```bash
cd <crate 根> && cargo build            # Debug
cd <crate 根> && cargo build --release  # Release（LTO + strip 等优化）
cd <crate 根> && cargo check            # 只查类型，不出产物，最快
cd <crate 根> && cargo clippy           # 静态分析
cd <crate 根> && cargo fmt              # 格式化（--check 只检查）
cd <crate 根> && cargo update           # 更新依赖（锁文件只经包管理器变更）

cd <端目录> && pnpm run lint            # ESLint（--fix 自动修）
cd <端目录> && pnpm run format          # Prettier
cd <端目录> && pnpm exec vue-tsc --noEmit   # TypeScript 类型检查
pnpm exec eslint .                      # 全仓前端 lint（根目录，CI 门禁口径）
```

---

## 7. 构建资源与 target 治理

```bash
cd <端目录> && pnpm run target:size      # 各 target 落点体积（两端脚本都有）
cd <端目录>/src-tauri && cargo clean     # 清该端编译缓存
```

- **`src-tauri/target` 超 15GB 就 `cargo clean`**，跑全量测试前必看磁盘（峰值十几 GB，磁盘满会以 Bus error 失败）。
- **新增 crate / 脚手架不得写死 `<crate>/target/`**：落点治理与决策见 `docs/knowledge/build-process.md`「Target 目录管理」+ 各 `.cargo/config.toml` 注释（改完用 `cargo metadata` 的 `target_directory` 核验）。
- sccache 让 clean 后的依赖重编走缓存（命中则只需链接），缓存位置 / 上限 / 清理见同文档「sccache 编译缓存」节。

---

## 8. 端口与遗留进程

| 端口 | 占用者 |
| --- | --- |
| `1420` | 桌面 Vite（`dev` / `tauri:dev`） |
| `1423` / `1424` | 移动 Vite（`tauri:android:dev`；真机经 `adb reverse tcp:1423 tcp:1423` 转发） |
| `5173` / `5199` | 移动插件 Dev Shell / SDK Dev Shell |
| `5037` / `9333` | adb server |
| 动态 | Gradle daemon |

**一键释放（Linux / WSL）**——先看占用与进程身份，再整棵杀（端口绑定未必是根进程，dev 树要连父进程一起杀，否则被 tauri CLI 看护逻辑重新拉起）：

```bash
ss -tlnp | grep -E ':(1420|1423|1424|5173|5199|5037|9333)\b'
ps -eo pid,ppid,etime,cmd | grep -E 'vite|tauri.js android|dev-shell|GradleDaemon' | grep -v grep

TARGETS='<PID1> <PID2> ...'
for pid in $TARGETS; do kill -TERM "$pid" 2>/dev/null; done
sleep 2
for pid in $TARGETS; do ps -p "$pid" >/dev/null 2>&1 && kill -KILL "$pid"; done

ss -tlnp | grep -E ':(1420|1423|1424|5173|5199|5037|9333)\b' || echo '全部端口已释放'
```

**Windows**：查找与终止用 `netstat -ano | findstr :1420` → `taskkill /PID <PID> /F`；PowerShell 用 `Stop-Process -Id (Get-NetTCPConnection -LocalPort 1420).OwningProcess -Force`。

**遗留 dev 进程 / 关不掉的黑框窗口**（`tauri:dev` 崩溃或被 kill -9 后，`target/debug/bedcode-desktop` 常被 systemd 收养继续跑，插件 `--watch` 也一起残留）：

```bash
ps -eo pid,ppid,etime,stat,cmd | grep -E 'bedcode-desktop|vite.*--watch' | grep -v grep
pkill -TERM -f 'target/debug/bedcode-desktop'; pkill -TERM -f 'vite.js build --watch'; sleep 2
pkill -KILL -f 'target/debug/bedcode-desktop'; pkill -KILL -f 'vite.js build --watch'
```

SIGKILL 兜底是必需的：`bedcode-desktop` 的 SIGTERM 处理链依赖窗口事件循环，dev 异常退出后事件循环可能已僵死。**注意**：窗口里若挂着 Tauri 内的 pi 会话，会随父进程一并退出。

其他清理：`rm -rf dist/`（前端产物）、`rm -rf node_modules && pnpm install`、`gen/android && ./gradlew clean`。

---

## 9. 依赖管理

```bash
cd <端目录> && pnpm install            # 安装
cd <端目录> && pnpm install <pkg>      # 添加依赖
cd <端目录> && pnpm install -D <pkg>   # 添加开发依赖
```

`Cargo.lock` / `pnpm-lock.yaml` **只经包管理器变更，禁止手工编辑**。

---

## 10. pi session 归档（`scripts/pi-session-archive.sh`）

把本项目 `.pi/sessions/` 中**距最新 session 超过 N 天**的 session jsonl（含复合 session 目录）移到 pi 安装目录的归档区（目录名由项目绝对路径推导，`/` 换 `-`）。

```bash
scripts/pi-session-archive.sh -n        # 只预览（推荐先跑）
scripts/pi-session-archive.sh           # 实际归档（默认 N=10）
scripts/pi-session-archive.sh -d 30     # 自定义阈值
PI_AGENT_DIR=/custom/pi scripts/pi-session-archive.sh -n
```

基准日期 = 本项目 `.pi/sessions/` 中最新 session 的时间戳（非今天）；只处理顶层 `*.jsonl` 与 `YYYY-MM-DDThh-mm-ss-msZ_<ulid>` 形式的 session 目录，`sol-pi` / `subagent-artifacts` 等目录绝不触碰；目标已有同名条目时跳过并警告，绝不覆盖。
