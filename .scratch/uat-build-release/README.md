# uat 双端编译 / 运行测试 / 本地打包

任务：在 uat 分支上双端编译 → 双端运行测试 → 本地打包 deb（桌面）+ APK（移动端）。

- 分支：`uat`（HEAD `4cbd3b68c` chore: 同步 dev 代码至 uat）
- 版本：桌面 / 移动端均 `2.1.0`
- 工作区未提交改动：仅 `scripts/pi-session-archive.sh`（他人/无关改动，不动）+ 受保护文档未跟踪（uat 分支 hooks 行为，正常）

## 环境事实

| 项 | 值 |
| --- | --- |
| Node / pnpm | v24.20.0 / 12.2.1 |
| Rust | stable 1.98.1 |
| CPU / 内存 | 16 核 / 13GB（常态可用 ~9GB） |
| Android SDK | `/home/binblink/Android/Sdk`，platforms android-34/36，build-tools 35/36 |
| NDK | `29.0.14206865`（r29），toolchain `toolchains/llvm/prebuilt/linux-x86_64` |
| JDK | 17.0.12 |
| Tauri CLI | 2.11.4 |
| cmake | **未安装**（Cargo.lock 无 `cmake` crate → 本仓库不需要） |
| Android 设备 | **无**（`adb devices` 空；无 emulator / 无 AVD）→ 移动端真机运行测试无法执行 |

## 结论速览

| 阶段 | 项 | 结果 |
| --- | --- | --- |
| 编译 | 桌面 前端 `pnpm run build`（vue-tsc + vite） | ✅ |
| 编译 | 移动端 前端 `pnpm run build`（vue-tsc + vite） | ✅ |
| 编译 | 桌面 Rust `cargo check` | ✅ EXIT 0 |
| 编译 | 移动端 Rust `cargo ndk -t arm64-v8a -P 24 check` | ✅ EXIT 0（2m33s） |
| 测试 | 桌面 vitest 672 用例 / 71 文件 | ✅ |
| 测试 | 移动端 vitest 443 用例 / 50 文件 | ✅ |
| 测试 | eslint 根目录 | ✅ 0 error / 119 warning |
| 测试 | 桌面 `cargo test` | ✅ EXIT 0 |
| 测试 | 移动端 `cargo test` | ⚠️ 默认并行下 1 例 flake；`--test-threads=1` 全绿（339 用例） |
| 运行 | 桌面 app 启动 + 端口/插件/mDNS 验证 | ✅ 见「运行测试证据」 |
| 运行 | 移动端真机运行 | ⛔ 无设备（无 adb 设备、无 emulator/AVD），跳过 |
| 打包 | 桌面 deb | ✅ `BedCode-2.1.0-release-amd64.deb` 19.4MB |
| 打包 | 移动端 APK | ✅ `BedCode-v2.1.0-release.apk` 38.9MB（bedcode.keystore 签名） |

## 日志

- `desktop-cargo-check.log` / `mobile-cargo-ndk-check.log` — 编译检查
- `desktop-vitest.log` / `mobile-vitest.log` — 前端测试
- `desktop-cargo-test.log` / `mobile-cargo-test.log` — Rust 测试
- 打包阶段日志见下方「打包记录」

## 踩坑记录（本次）

### 1. 移动端裸跑 `cargo check --target aarch64-linux-android` 必失败

报错：`cc-rs: failed to find tool "aarch64-linux-android-clang"`。
原因：NDK r29 的 clang 二进制名带 API 后缀（`aarch64-linux-android24-clang`），裸 `cargo`
不设置 `CC_<target>`，cc-rs 只能找 PATH 里的裸名字。Tauri CLI 内部走 **cargo-mobile2**
（strings 见 `cargo-mobile2-0.22.4`、`cargo_mobile2::android::target`、`TARGET_CC`、
`ANDROID_NATIVE_API_LEVEL`、`-C linker=...-C link-arg=-landroid`）才把环境搭好。
**正确做法**：本机用 `cargo ndk -t arm64-v8a -P 24 check`（cargo-ndk 已装），或直接用
`pnpm exec tauri android build`（走 cargo-mobile2）。

### 2. 桌面 vitest 在低 worker 数下 JS 堆 OOM

`pnpm run test:run`（默认）与 `--no-file-parallelism` 均报
`FATAL ERROR: Ineffective mark-compacts near heap limit Allocation failed - JavaScript heap out of memory`。
`--logHeapUsage` 显示单文件仅 22-34MB，但单个 fork 累计到 3562MB 才崩 —— 是 Vue 宿主 +
插件模块图被一个 worker 全量驻留所致。
**正确做法**：`--pool=forks --minWorkers=4 --maxWorkers=8` + `NODE_OPTIONS=--max-old-space-size=2048`
→ 71 文件 672 用例全过，9s。属环境/并发特性，非代码缺陷。

### 3. cargo 与 gradle / vitest 并行有系统级 OOM 前科

`gen/android/gradle.properties` 注释明确记录「4G 堆曾与 Kotlin daemon / cargo 并行构建挤压导致
系统级 OOM」。打包阶段避免让 `tauri android build` 的 gradle 与另一个 cargo 进程同时跑。

## 打包记录

### 前置：插件产物重建（必要）

桌面 `beforeBuildCommand` 只有 `pnpm run build`，**不重建插件**，而 `src-tauri/resources/`
在 git 里只跟踪 `config.properties` + `deb/`，插件产物完全不入库。uat 同步提交（HEAD
`4cbd3b68c`）改了插件源码后本地产物即过期，直接打 deb 会打进旧插件。故按 CI 做法先重建：

1. `cd packages/plugin-sdk-desktop && pnpm run build`（SDK dist 不入库，必须先构建）
2. `cd plugins/agent-hub && pnpm run build` —— **CI 的 `plugins:build` 漏了 agent-hub**：
   `scripts/plugin-build.js` 的 `PLUGINS` 只有 ai-chatbox / auto-task / file-transfer 三家，
   而 agent-hub 是正式内置插件（形态 rust-ts，见 `.scratch/2026-09-13-agent-hub/`）。
   本地若不单独构建，deb 会缺 agent-hub 或带旧版；本次 deb 已含当前源码版本
3. `pnpm run plugins:build`（ai-chatbox / auto-task / file-transfer）

移动端无需手动处理：`beforeBuildCommand` = `pnpm run build:all` = `plugins:build && build`，自动重建。

### 桌面 deb

命令：`cd bedcode-desktop && pnpm run tauri:build -- --bundles deb`

- release 编译 10m56s，打包成功
- **updater 产物被自动禁用**：`.env` 只有 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`、无私钥，
  脚本 `resolveSigningEnv()` 返回 null → 追加 `--config {"bundle":{"createUpdaterArtifacts":false}}`。
  这是脚本设计的本地构建路径（正式发布由 release.yml 的 Secrets 签名）
- deb 内容已核：4 个插件全部入包（含 agent-hub），control 段 Depends 与 tauri.conf.json 一致
- SHA256 `51062bcdedae4a577c4ce8ea7704564738b0b9bf2314a3c5c060c9349bb0ceea`

### 移动端 APK

命令：`pnpm exec tauri android build --apk -t aarch64`（与 CI build-android 一致），
env 显式导出 `ANDROID_HOME / ANDROID_SDK_ROOT / NDK_HOME / ANDROID_NDK_HOME / ANDROID_NDK`

- release 编译 1m42s（cargo-mobile2 自动设置 aarch64 工具链）
- 日志显示中间产物 `app-universal-release.apk`，最终落盘为 `BedCode-v2.1.0-release.apk`
- **签名已验证**（`apksigner verify --print-certs`）：
  `CN=BedCode, OU=Development, O=BedCode, L=Beijing, ST=Beijing, C=CN`
  SHA-256 `a85e2f1bc552408f841f0be128bff9a37c6f44218a94d625fd65ec0e4378a037`，
  即仓库根 `bedcode.keystore`（§9 签名唯一真源）
- output-metadata.json：applicationId `com.bedcode.mobile`，versionCode 2001000，
  versionName 2.1.0，minSdk 24 —— 与配置一致
- SHA256 `5143a317c107e2e5ac0f66a665bc95b7add5f92117efea9086ecf742cb2524b1`
- 注：CI 装的是 `ndk;27.0.12077973`，本机为 `29.0.14206865`（r29，较新），构建与签名均通过

## 运行测试证据（桌面端）

启动 `target/release/bedcode-desktop`（DISPLAY=:0），实测：

- `ss -ltnp` → `LISTEN 0.0.0.0:8765 users:("bedcode-desktop",pid=1915428,fd=66)`
- `curl /api/health` → `{"port":8765,"status":"ok","uptime_secs":40}`
- 受保护端点 401（`/api/version` `/api/info` `/api/status` `/api/sessions` `/api/plugins`），
  认证过滤链生效；`/` 与 `/api/auth/status` 404
- 日志：`Actix Web server (HTTP + WS) started on port 8765`（16 workers）
- mDNS：`[MdnsAdvertiser] Advertising _bedcode._tcp.local. as BedCode-binblink-PC on port 8765`
- **4 个插件全部激活**，agent-hub 有完整 guest 生命周期：
  `Plugin database created/opened` → `[plugin:com.bedcode.agent-hub] Plugin activated (wasm)` →
  `Plugin startup init completed` → `[PluginHost] Plugin activated ... persist=false`。
  这直接验证了 uat 上的修复 `e0472444f`（user-installed rust-ts 插件执行完整 guest 生命周期）
- **无 ERROR 级日志**。唯一 WARN 是 agent-hub 的 `fs_auth: batch auth request timed out`
  → `directory authorization declined; agent-hub degrades until granted`：无人点授权弹窗时的
  预期优雅降级，非缺陷
- 关停：SIGTERM 后 actix 16 workers + accept thread 全部 `shutting down idle worker` /
  `accept thread stopped`，端口释放；进程随后残留在 GTK 事件循环（GUI 应用无窗口关闭事件），
  SIGINT 无效后 SIGKILL 清理。属无头启动的预期行为，非优雅关停缺陷

清理：app 进程、gradle daemon（`./gradlew --stop`，1 stopped）、kotlin daemon、
8765 端口均已确认释放；未留下 vitest/cargo/tauri 残留进程。

## 待用户决策的两个问题

1. **`tauri-build.js` DEB 重命名静默失效**（真 bug，未改仓库文件）
   - 现象：`pnpm run tauri:build` 成功后 deb 仍叫 `BedCode_2.1.0_amd64.deb`，
     日志无「DEB 包已重命名」也无任何 warn —— 静默 no-op
   - 根因：`scripts/tauri-build.js:169` 的正则写在**模板字符串**里，`\w` 与 `\.` 的反斜杠被
     JS 字符串字面量吃掉。插桩实测输出：
     `pattern=/^BedCode_2\.1\.0_(w+).deb$/`（`\w` 退化成字面量 `w`，`\.deb` 退化成 `.deb`）
     → `file.match(pattern) === null`（`match=null` 对 `BedCode_2.1.0_amd64.deb` 亦然）
     → 循环空转，既不 rename 也不 catch
   - 对照：同文件 `:125` 的 NSIS 版本写的是 `\\w+` / `\\.exe`（双反斜杠，正确），所以 exe 能改名
   - 修复（1 行 2 字符）：`:169` 改为
     `^${escapeRegExp(productName)}_${escapeRegExp(version)}_(\\w+)\\.deb$`
   - 影响面：仅本地/自定义构建的产物命名（注释已写明 CI 走 tauri-action 不经本脚本）；
     产物内容与可安装性完全不受影响。本次已手动重命名为 `BedCode-2.1.0-release-amd64.deb`
2. **移动端 `http_auth_flow` 测试在默认并行下 flake**（测试隔离缺陷，非产品 bug）
   - `auth_manager_verify_rejection_returns_false` 断言 `拒绝不得写全局 token`
     在 `tests/http_auth_flow.rs:605` 失败；单独跑 `-- --exact` 通过，
     整库 `--test-threads=1` 通过（17/17）→ 跨线程串扰：`get_global_token()` 是进程级全局，
     同 test binary 内并行用例互相写清
   - **CI 风险**：test.yml 的 `rust-mobile` 跑裸 `cargo test`（默认并行），该 flake 可能在 CI 触发
   - 建议修法：给这些用全局 token 的用例加串行标记（如 `#[serial]` / 独立 test binary /
     用 token 作用域 RAII guard），而非改产品代码
   - 同类风险：test.yml 的 `frontend-desktop` 跑裸 `pnpm run test:run`（默认 worker 数），
     本机在低 worker 数下 JS 堆 OOM（见踩坑 2），CI 资源若更紧同样可能炸

## 磁盘与资源管理记录

`/` 一度到 93%（剩 11G）。为腾空间按阶段清理了不再需要的产物：

- `bedcode-desktop/src-tauri/target/debug`（12G，测试产物，测试结果已记录）
- `bedcode-mobile/src-tauri/target/debug`（8.2G，同上）
- `bedcode-mobile/src-tauri/gen/android/app/build`（2.5G，上一轮构建输出，gradle 会重建；
  其缓存本体在 `~/.gradle/build-cache-1`，不受影响）

最终 `target` 只保留打包所需的 release / aarch64-linux-android，剩 28G 空闲。

---

## 后续：两项修复（2026-09-15 12:0x，已落 dev + uat）

用户确认修复并指明必须同步到 dev，测试项用串行标记。

### 提交

| 分支 | 提交 | 内容 |
| --- | --- | --- |
| uat | `0c52395fe` | fix(desktop): tauri-build.js DEB 重命名正则转义 |
| uat | `8cd9e2554` | fix(mobile): http_auth_flow 全局 token 用例加串行闸 |
| dev | `b462bff4e` | 同 0c52395fe（cherry-pick） |
| dev | `c48f2bb92` | 同 8cd9e2554（cherry-pick） |

`git diff uat dev -- <两文件>` 为空 → 两分支内容完全一致。

### 修复 1：tauri-build.js:169

```diff
-    `^${escapeRegExp(productName)}_${escapeRegExp(version)}_(\w+)\.deb$`,
+    `^${escapeRegExp(productName)}_${escapeRegExp(version)}_(\\w+)\\.deb$`,
```

附注一行注释说明模板字符串转义陷阱，防止回归。

**验证（端到端，跑真实命令）**：`pnpm run tauri:build -- --bundles deb` 输出
`[tauri-build] DEB 包已重命名: BedCode_2.1.0_amd64.deb -> BedCode-2.1.0-release-amd64.deb`

### 修复 2：http_auth_flow.rs 串行闸

沿用 `http_proxy_flow.rs:19` 既有约定（零新增依赖）：

```rust
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
```

7 个触碰全局 token 的用例，函数体首句：

```rust
let _serial = SERIAL.lock().unwrap();
clear_global_token();
```

7 个用例：`resolve_base_url_happy_path_with_target`、`auth_manager_pairing_verify_full_flow`、
`auth_manager_verify_rejection_returns_false`、`auth_manager_reauth_refreshes_token`、
`auth_manager_reauth_rejection_returns_err`、`auth_manager_no_target_is_error`、
`auth_manager_transport_failure_marks_failed`

除串行闸外还加了**取锁后立即 clear_global_token()**：单用例中途 panic 也不会把脏全局泄漏给
下一个用例（skill 要求测试独立确定性，`共享状态/顺序依赖` 列为反模式）。

**验证**：
- `cargo test`（**默认并行**，修复前必失败）→ EXIT 0，339 用例全过
- `cargo test --test http_auth_flow` 默认并行**连跑 10 轮 10/10 通过**（flake 是概率性的，
  单轮通过不足为证）
- `cargo fmt --check`：`http_auth_flow.rs` 0 差异；其余 fmt 漂移全在 `src/**`（存量，未动）
- eslint：`scripts/` 被 ignore（0 error）

### 未采用 / 待定

- 未改成 `unwrap_or_else(PoisonedError::into_inner)`：与 `http_proxy_flow.rs` 既有 `.unwrap()`
  约定保持一致。代价：若某用例真失败，Mutex 中毒会让后续用例报「poisoned」而非真错，
  CI 上 1 个失败会放大成多个误导性失败。若要改进，建议两个文件一起改。
- `scripts/plugin-build.js` 的 PLUGINS 漏 agent-hub（本次手工补构建）——未改，需用户决策
- desktop vitest worker 上限（`test.yml` 跑裸 `pnpm run test:run`，低 worker 数会 OOM）
  ——未改，需用户决策
