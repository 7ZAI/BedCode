# BedCode 项目构建过程知识总结

本文档详细说明 BedCode 项目的编译构建流程，包括桌面端和移动端（Android）的构建机制。

---

## 目录

1. [项目架构概览](#项目架构概览)
2. [前端构建策略](#前端构建策略)
3. [Rust 后端编译](#rust-后端编译)
4. [Android 构建流程](#android-构建流程)
5. [桌面端构建流程](#桌面端构建流程)
6. [编译优化配置](#编译优化配置)
7. [常用构建命令](#常用构建命令)

---

## 项目架构概览

```
BedCode/
├── src/                      # Vue 3 前端
│   ├── components/
│   │   ├── common/           # 共享 UI 组件
│   │   ├── desktop/          # 桌面端专用组件
│   │   └── mobile/           # 移动端专用组件
│   ├── views/
│   │   ├── desktop/          # 桌面端页面
│   │   └── mobile/           # 移动端页面
│   ├── composables/          # Vue Composition API
│   ├── stores/               # Pinia 状态管理
│   └── router/               # 路由配置
│
├── src-tauri/                # Rust 后端
│   ├── src/
│   │   ├── auth/             # 设备配对认证
│   │   ├── commands.rs       # Tauri IPC 命令
│   │   ├── db/               # SQLite 数据库
│   │   ├── discovery/        # mDNS 设备发现
│   │   ├── pty/              # PTY 管理 (桌面端)
│   │   ├── session/          # 会话管理
│   │   └── websocket/        # WebSocket 服务器
│   ├── .cargo/config.toml    # Cargo 编译配置
│   ├── Cargo.toml            # Rust 依赖配置
│   └── tauri.conf.json       # Tauri 配置
│
└── docs/
    └── knowledge/            # 知识文档
```

---

## 前端构建策略

### 核心原则：一起打包，运行时区分

BedCode 的前端代码（桌面端和移动端）在构建时打包在一起，通过运行时平台检测来决定渲染哪套 UI。

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        构建产物 (dist/)                                  │
│                                                                         │
│   ┌─────────────────┐   ┌─────────────────┐   ┌─────────────────┐      │
│   │ desktop/*.vue   │   │ mobile/*.vue    │   │ 共享资源        │      │
│   │ SessionsView    │   │ TerminalView    │   │ composables     │      │
│   │ DevicesView     │   │ QuickActions    │   │ stores          │      │
│   │ SettingsView    │   │ HistoryView     │   │ router          │      │
│   └────────┬────────┘   └────────┬────────┘   └────────┬────────┘      │
│            │                     │                     │               │
│            └─────────────────────┼─────────────────────┘               │
│                                  │                                     │
│                                  ▼                                     │
│                    ┌─────────────────────────┐                         │
│                    │      打包在一起          │                         │
│                    │   assets/index-xxx.js   │                         │
│                    │   assets/index-xxx.css  │                         │
│                    └─────────────────────────┘                         │
│                                  │                                     │
│              ┌───────────────────┼───────────────────┐                 │
│              ▼                   ▼                   ▼                 │
│        Desktop App         Android APK          iOS App               │
│        (Windows/Mac)       (相同代码)           (相同代码)              │
└─────────────────────────────────────────────────────────────────────────┘
```

### 平台检测机制

#### usePlatform Composable

位置：`src/composables/usePlatform.ts`

```typescript
/**
 * 平台检测逻辑
 * 
 * 生产环境 (Tauri 运行时):
 *   - 使用 @tauri-apps/plugin-os 获取真实平台信息
 * 
 * 开发环境 (浏览器):
 *   - 通过 URL 参数 ?platform=mobile 模拟移动端
 *   - 通过 localStorage.setItem('platform-mode', 'mobile') 持久化
 */
export function usePlatform() {
  const platformInfo = ref<PlatformInfo>({
    platform: null,      // 'windows' | 'macos' | 'linux' | 'android' | 'ios'
    arch: null,          // 'x86_64' | 'aarch64' | 'arm'
    isDesktop: false,    // 是否为桌面端
    isMobile: false,     // 是否为移动端
    // ...
  })

  // 检测是否在 Tauri 运行时
  function isTauriRuntime(): boolean {
    return typeof window !== 'undefined' && '__TAURI__' in window
  }

  // 从 Tauri OS 插件获取平台信息
  async function detectFromTauri(): Promise<PlatformInfo | null> {
    const { platform } = await import('@tauri-apps/plugin-os')
    const p = platform()
    return {
      isDesktop: !['android', 'ios'].includes(p),
      isMobile: ['android', 'ios'].includes(p),
      // ...
    }
  }
}
```

#### 检测流程图

```
                    ┌─────────────────┐
                    │ usePlatform()   │
                    └────────┬────────┘
                             │
                             ▼
                    ┌─────────────────┐
                    │ isTauriRuntime? │
                    │ '__TAURI__' in  │
                    │    window       │
                    └────────┬────────┘
                             │
              ┌──────────────┴──────────────┐
              │                             │
              ▼                             ▼
     ┌─────────────────┐          ┌─────────────────┐
     │  生产环境 (Tauri) │          │  开发环境 (浏览器) │
     │                 │          │                 │
     │ @tauri-apps/    │          │ URL 参数模拟    │
     │ plugin-os       │          │ ?platform=mobile│
     │                 │          │                 │
     │ platform() →    │          │ localStorage    │
     │ 'windows' |     │          │ 'platform-mode' │
     │ 'android' | ... │          │                 │
     └────────┬────────┘          └────────┬────────┘
              │                             │
              └──────────────┬──────────────┘
                             │
                             ▼
                   ┌───────────────────┐
                   │ PlatformInfo      │
                   │ {                 │
                   │   isDesktop: bool │
                   │   isMobile: bool  │
                   │   platform: ...   │
                   │ }                 │
                   └───────────────────┘
```

### 条件渲染布局

位置：`src/App.vue`

```vue
<template>
  <div class="min-h-screen bg-dark-900 text-dark-100">
    <!-- 桌面端布局 -->
    <template v-if="isDesktop">
      <div class="flex flex-col h-screen">
        <TitleBar />
        <div class="flex flex-1 overflow-hidden">
          <Sidebar />
          <main class="flex-1 overflow-hidden">
            <router-view />  <!-- /sessions, /devices, /settings -->
          </main>
        </div>
      </div>
    </template>

    <!-- 移动端布局 -->
    <template v-else>
      <div class="flex flex-col h-screen">
        <main class="flex-1 overflow-hidden">
          <router-view />  <!-- /mobile/* 路由 -->
        </main>
        <MobileNav v-if="!isTerminalRoute" />
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { usePlatform } from './composables/usePlatform'

const { platformInfo } = usePlatform()
const isDesktop = computed(() => platformInfo.value.isDesktop)
</script>
```

### 路由配置

位置：`src/router/index.ts`

```typescript
const router = createRouter({
  history: createWebHistory(),
  routes: [
    // 桌面端路由
    { path: '/', redirect: '/sessions' },
    { path: '/sessions', component: () => import('@/views/desktop/SessionsView.vue') },
    { path: '/devices', component: () => import('@/views/desktop/DevicesView.vue') },
    { path: '/settings', component: () => import('@/views/desktop/SettingsView.vue') },
    
    // 移动端路由
    { path: '/mobile/devices', component: () => import('@/views/mobile/DevicesView.vue') },
    { path: '/mobile/terminal/:id', component: () => import('@/views/mobile/TerminalView.vue') },
    { path: '/mobile/quick-actions', component: () => import('@/views/mobile/QuickActionsView.vue') },
    { path: '/mobile/history', component: () => import('@/views/mobile/HistoryView.vue') },
    { path: '/mobile/settings', component: () => import('@/views/mobile/SettingsView.vue') },
  ],
})
```

### Tree-shaking 优化

虽然所有代码都打包，但 Vite 的 tree-shaking 会优化未使用的代码：

```
实际加载的代码:

Desktop 运行时:
├── App.vue (桌面布局分支)
├── TitleBar.vue ✓
├── Sidebar.vue ✓
├── desktop/SessionsView.vue ✓
├── mobile/*.vue ✗ (存在于包中，但未加载)

Android 运行时:
├── App.vue (移动布局分支)
├── MobileNav.vue ✓
├── mobile/TerminalView.vue ✓
├── desktop/*.vue ✗ (存在于包中，但未加载)
```

---

## Rust 后端编译

### Cargo.toml 配置

位置：`src-tauri/Cargo.toml`

```toml
[package]
name = "bedcode"
version = "0.1.0"
edition = "2021"

[lib]
# lib 名跟随所属端（2026-09-30 改名）：两端曾同为 `bedcode_lib`，导致任何同时依赖
# 两端的工程（跨端集成测试）crate 名冲突。同名是隐患不是风格问题。
name = "bedcode_desktop_lib"   # 移动端为 "bedcode_mobile_lib"
# 关键：生成多种类型的库文件
# - staticlib: 静态库 (iOS 使用)
# - cdylib: 动态库 (Android 使用)
# - rlib: Rust 库 (桌面端使用)
crate-type = ["staticlib", "cdylib", "rlib"]

# 通用依赖 (所有平台)
[dependencies]
tauri = { version = "2", features = ["tray-icon"] }
tauri-plugin-shell = "2"
tauri-plugin-notification = "2"
tauri-plugin-dialog = "2"
tauri-plugin-os = "2"
tokio = { version = "1", features = ["full"] }
rusqlite = { version = "0.32", features = ["bundled"] }
# ... 其他通用依赖

# 仅桌面端依赖 (条件编译)
[target.'cfg(not(any(target_os = "android", target_os = "ios")))'.dependencies]
portable-pty = "0.8"           # PTY 管理 (移动端不支持)
tokio-tungstenite = "0.24"     # WebSocket 服务器
futures-util = "0.3"
mdns-sd = "0.11"               # mDNS 发现
keyring = "3"                  # 安全存储
```

### 条件编译机制

Rust 通过 `cfg` 属性实现条件编译：

```rust
// 仅在桌面端编译
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod pty;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod websocket;

// 所有平台都编译
mod auth;
mod db;
mod session;
```

### 编译产物

| 端 | 目标平台 | 编译目标 | 产物类型 | 产物名称 |
|----|---------|---------|---------|---------|
| 桌面 | Windows | x86_64-pc-windows-msvc | .dll | bedcode_desktop_lib.dll |
| 桌面 | macOS | aarch64-apple-darwin | .dylib | libbedcode_desktop_lib.dylib |
| 桌面 | Linux | x86_64-unknown-linux-gnu | .so | libbedcode_desktop_lib.so |
| 移动 | Android | aarch64-linux-android | .so | libbedcode_mobile_lib.so |
| 移动 | iOS | aarch64-apple-ios | .a | libbedcode_mobile_lib.a |

> 产物名跟随 `[lib] name`（tauri build 自动生成 `System.loadLibrary` / jniLibs
> 文件名）。改名后 Android `gen/android/.../generated/Rust.kt` 的
> `System.loadLibrary("bedcode_mobile_lib")` 随之变化——该目录被 gitignore，
> `tauri android init` 重建时自动跟随；**手工备份副本
> `android-backup/app-java/generated/Rust.kt` 需同步改**，否则恢复即失配。

---

## Android 构建流程

### 整体架构

```
┌──────────────────┐     ┌──────────────────┐     ┌──────────────────┐
│   Vue 3 Frontend │     │   Rust Backend   │     │  Android Project │
│   (src/)         │     │   (src-tauri/)   │     │  (gen/android/)  │
└────────┬─────────┘     └────────┬─────────┘     └────────┬─────────┘
         │                        │                        │
         │ 1. pnpm run build       │                        │
         │ ──────────────────────>│                        │
         │   生成 dist/           │                        │
         │                        │                        │
         │                        │ 2. cargo build         │
         │                        │   --target aarch64...  │
         │                        │   生成 .so 文件        │
         │                        │                        │
         │                        │<───────────────────────│
         │                        │ 3. Gradle 触发 RustPlugin│
         │                        │                        │
         │                        │───────────────────────>│
         │                        │ 4. .so → jniLibs/      │
         │                        │                        │
         │<───────────────────────────────────────────────>│
         │          5. APK 打包 (dist/ + .so)              │
         │                        │                        │
```

### 关键配置文件

| 文件 | 作用 |
|------|------|
| `tauri.conf.json` | 定义 `frontendDist: "../dist"`，指定前端产物路径 |
| `Cargo.toml` | `crate-type = ["cdylib"]` 生成 Android 可用的动态库 |
| `gen/android/app/build.gradle.kts` | RustPlugin 配置，关联 Rust 编译 |
| `gen/android/buildSrc/.../RustPlugin.kt` | 核心：调用 `pnpm run tauri android` 编译 Rust |
| `gen/android/.../assets/tauri.conf.json` | 复制到 APK 的配置，运行时读取 |

### 详细构建步骤

```
pnpm run tauri:android:build
       │
       ▼
┌─────────────────────────────────────────────────────────────┐
│ 1. 前端编译                                                  │
│    pnpm run build → dist/                                    │
│    (Vue 3 + Vite 打包所有前端代码)                           │
└─────────────────────────────────────────────────────────────┘
       │
       ▼
┌─────────────────────────────────────────────────────────────┐
│ 2. Gradle 构建 Android 项目                                  │
│    cd src-tauri/gen/android && ./gradlew assembleDebug      │
└─────────────────────────────────────────────────────────────┘
       │
       ▼
┌─────────────────────────────────────────────────────────────┐
│ 3. RustPlugin 触发 Rust 编译                                 │
│    为每个 CPU 架构创建构建任务:                               │
│    - rustBuildArm64Debug → cargo build --target aarch64     │
│    - rustBuildX86_64Debug → cargo build --target x86_64     │
│    - rustBuildArmDebug → cargo build --target armv7         │
│    输出: libbedcode_mobile_lib.so                           │
└─────────────────────────────────────────────────────────────┘
       │
       ▼
┌─────────────────────────────────────────────────────────────┐
│ 4. 合并资源                                                  │
│    - dist/ → assets/ (WebView 加载前端代码)                  │
│    - *.so → jniLibs/{arch}/ (Rust 原生库)                    │
│    - tauri.conf.json → assets/ (Tauri 运行时配置)            │
└─────────────────────────────────────────────────────────────┘
       │
       ▼
┌─────────────────────────────────────────────────────────────┐
│ 5. 打包 APK                                                  │
│    app/build/outputs/apk/universal/debug/app-universal-debug.apk │
└─────────────────────────────────────────────────────────────┘
```

### RustPlugin 核心逻辑

位置：`src-tauri/gen/android/buildSrc/src/main/java/com/bedcode/app/kotlin/RustPlugin.kt`

```kotlin
open class RustPlugin : Plugin<Project> {
    override fun apply(project: Project) {
        // 支持的 CPU 架构
        val defaultAbiList = listOf("arm64-v8a", "armeabi-v7a", "x86", "x86_64")
        val targetsList = listOf("aarch64", "armv7", "i686", "x86_64")

        // 为每个架构创建 Product Flavor
        extensions.configure<ApplicationExtension> {
            flavorDimensions.add("abi")
            productFlavors {
                create("universal") {
                    dimension = "abi"
                    ndk { abiFilters += defaultAbiList }
                }
                // arm64, arm, x86, x86_64 单架构变体
            }
        }

        // 创建 Rust 编译任务
        afterEvaluate {
            for (profile in listOf("debug", "release")) {
                for (targetPair in targetsList.withIndex()) {
                    val targetBuildTask = project.tasks.maybeCreate(
                        "rustBuild${arch}Debug",
                        BuildTask::class.java
                    ).apply {
                        rootDirRel = "../../../"  // 指向项目根目录
                        target = targetName
                        release = false
                    }
                    // 关联到 Gradle 的 JNI 合并任务
                    tasks["merge${arch}JniLibFolders"].dependsOn(targetBuildTask)
                }
            }
        }
    }
}
```

### BuildTask 执行逻辑

位置：`src-tauri/gen/android/buildSrc/src/main/java/com/bedcode/app/kotlin/BuildTask.kt`

```kotlin
open class BuildTask : DefaultTask() {
    @Input var rootDirRel: String? = null
    @Input var target: String? = null
    @Input var release: Boolean? = null

    @TaskAction
    fun assemble() {
        // 执行: pnpm run tauri android android-studio-script --target aarch64
        project.exec {
            workingDir(File(project.projectDir, rootDirRel))
            executable("pnpm")
            args("run", "tauri", "android", "android-studio-script")
            if (release) args("--release")
            args("--target", target)
        }.assertNormalExitValue()
    }
}
```

### Android 项目结构

```
src-tauri/gen/android/
├── app/
│   ├── src/main/
│   │   ├── assets/
│   │   │   └── tauri.conf.json     # Tauri 运行时配置
│   │   ├── java/com/bedcode/app/
│   │   │   └── MainActivity.kt     # 入口 Activity
│   │   └── AndroidManifest.xml
│   ├── build.gradle.kts            # 应用级构建配置
│   └── tauri.build.gradle.kts      # Tauri 插件依赖
├── buildSrc/
│   └── src/main/java/.../
│       ├── RustPlugin.kt           # Rust 编译插件
│       └── BuildTask.kt            # 编译任务
├── build.gradle.kts                # 项目级构建配置
├── tauri.settings.gradle           # Tauri 插件路径
└── gradlew                         # Gradle Wrapper
```

---

## 桌面端构建流程

### 构建步骤

```
pnpm run tauri:build
       │
       ▼
┌─────────────────────────────────────────────────────────────┐
│ 1. 前端编译                                                  │
│    pnpm run build → dist/                                    │
└─────────────────────────────────────────────────────────────┘
       │
       ▼
┌─────────────────────────────────────────────────────────────┐
│ 2. Rust 编译                                                 │
│    cargo build --release                                     │
│    产物: libbedcode_desktop_lib.so / .dll / .dylib          │
└─────────────────────────────────────────────────────────────┘
       │
       ▼
┌─────────────────────────────────────────────────────────────┐
│ 3. 打包安装程序                                              │
│    Windows: NSIS 安装包 (.exe)                               │
│    macOS: DMG 磁盘镜像                                       │
│    Linux: AppImage / DEB 包                                  │
└─────────────────────────────────────────────────────────────┘
```

### 平台特定产物

| 平台 | 编译目标 | 安装包格式 |
|------|---------|-----------|
| Windows | x86_64-pc-windows-msvc | .exe (NSIS), .msi |
| macOS (Intel) | x86_64-apple-darwin | .dmg |
| macOS (Apple Silicon) | aarch64-apple-darwin | .dmg |
| Linux | x86_64-unknown-linux-gnu | .AppImage, .deb |

---

## 编译优化配置

### Cargo 编译配置

#### 编译资源限制（防 CPU 满载 / 防 OOM）

位置：仓库根 `.cargo/config.toml`（**不是** `src-tauri/.cargo/config.toml`；仓库根配置对两端 src-tauri 同时生效）

```toml
# cargo 默认 jobs = 逻辑核心数（本机 16），wasmtime/cranelift 等巨型 crate 单 rustc 峰值 1~2GiB，
# 16 路并发需 20GiB+，无 swap 时直接 OOM kill。按「可用内存GiB / 1.5」估算上限。
# CI（ubuntu-latest / windows-latest = 4 核 16GiB）jobs 恰好等于核数，零性能损失。
[build]
jobs = 4
```

取值经验公式：`jobs = min(核心数, floor(可用内存GiB / 1.5))`

**覆盖方式**（Cargo 优先级：环境变量 > 本机 `~/.cargo/config.toml` > 仓库根本文件）：

| 场景 | 命令 |
|------|------|
| 移动端 Android 全量构建 / 同时开着模拟器（最重，历史 OOM 点） | `CARGO_BUILD_JOBS=3 pnpm run tauri:android:dev` |
| 内存 ≥ 32GiB 的机器想提速 | `~/.cargo/config.toml` 写 `[build] jobs = 8` |

验证（`cargo config` 子命令需 `-Z`）：`RUSTC_BOOTSTRAP=1 cargo -Z unstable-options config get build.jobs`

> OOM 症状识别：rustc 报 `memory allocation of N bytes failed` 后会级联出成百上千个 `can't find crate` 假错误，极易误判为依赖不兼容 —— 先看内存再看依赖。

#### Cargo profile 配置（profile 段只能写在 Cargo.toml，cargo config 不支持）

位置：`src-tauri/Cargo.toml`（两端一致）

```toml
[profile.release]
# unwind：panic 可被 catch_unwind（错误边界/插件宿主）捕获，abort 会让一切 panic 处理失效
panic = "unwind"
# fat LTO + 单 codegen unit 会在链接阶段合并全部 bitcode，峰值 8GiB+（wasmtime 更甚），
# 12~16GiB 机器上触发 OOM；改 thin LTO + 并行 codegen，性能损失 ~2-5%
codegen-units = 16
lto = "thin"
opt-level = "s"
strip = true

[profile.dev]
# debug = 1 全量 DWARF 使巨型 crate 在 collect_and_partition_mono_items 阶段 OOM
# （0xc0000409，禁用页面文件的机器上必现）；line-tables-only 保留行号回溯
debug = "line-tables-only"
incremental = true
split-debuginfo = "packed"

[profile.dev.package."*"]
opt-level = 2        # 依赖项优化（提升 dev 下测试/插件运行速度，代价是依赖编译内存更高）
```

```toml
# 可选：使用更快的链接器（Linux 开发机，未启用）
# [target.x86_64-unknown-linux-gnu]
# linker = "clang"
# rustflags = ["-C", "link-arg=-fuse-ld=mold"]
```

### Target 目录管理

Rust 增量编译会导致 `target` 目录持续增长。**本仓库无根 workspace**（两端 30+ 个独立
`Cargo.toml`），所以「哪个 crate 的 target」会直接影响磁盘占用：per-crate target 会把
**相同的依赖图重复编译 N 遍**。2026-09-26 治理后，产物收敛为下列落点（2026-09-30
server-lib 拆出后新增仓库根 `target/server-libs`；2026-10-04 wasm-core-lib-split 拆出
机制内核 + 能力域后新增仓库根 `target/host-kits`，共 8 个）：

| 落点 | 内容 | 路径真源 |
| --- | --- | --- |
| `bedcode-desktop/src-tauri/target/` | 桌面宿主（含 15G 阈值治理） | Tauri / cargo 默认 |
| `bedcode-mobile/src-tauri/target/` | 移动端宿主（含 15G 阈值治理） | Tauri / cargo 默认 |
| `bedcode-desktop/target/fixtures/` | 桌面 9 个测试夹具共享 | `src-tauri/.../runtime/fixture_target.rs` + `packages/.cargo/config.toml` |
| `bedcode-desktop/target/wasm-apps/` | 桌面 4 个 wasm 应用共享 | `scripts/plugin-wasm-config.mjs`（`WASM_TARGET_DIR`）+ `wasm-apps/.cargo/config.toml` |
| `bedcode-mobile/target/fixtures/` | 移动 2 个夹具 / 插件共享 | `bedcode-mobile/src-tauri/.../component.rs` 的 `fixture_target_dir()` |
| `target/server-libs/`（仓库根） | 桌面 6 个 `bedcode-server-*` + `bedcode-crypto-engine` 共享 | 各 crate 的 `.cargo/config.toml`（`../../../target/server-libs`，相对**crate 根**） |
| `target/host-kits/`（仓库根） | 「机制内核 + 能力域」同族共享：`packages/bedcode-host-kit`、`bedcode-desktop/packages/bedcode-discovery-engine`（wasm-core-lib-split 票 03；sqlite 域票 07 已由 ADR 0036 撤销，不在此桶） | 各 crate 的 `.cargo/config.toml`（根 `packages/` 下写 `../../target/host-kits`，桌面 `packages/` 下写 `../../../target/host-kits`——两者同一目录） |
| `cross-end-tests/target/` | 跨端互连测试（仓库根工程，依赖两端 lib） | cargo 默认；**刻意独立**（见下） |

**`.cargo/config.toml` 里 `target-dir` 的相对路径基准 = 该 `.cargo` 目录的父目录**
（不是 cwd、也不是 config 文件所在目录）。这条是本节最容易踩的坑：写错一层 `..`
不会有任何报错，只是安静地把产物写到另一个目录去（2026-10-04 修：`wasm-apps/` 与
`packages/` 两份 config 都多写了一个 `..`，实际落到**仓库根** `target/wasm-apps`
与 `target/fixtures`，于是「`pnpm run build` 的产物」与「`cargo test` 的产物」分裂成
两套目录，而注释与本文档都写着同一个目录——**只有 `cargo metadata` 的
`target_directory` 字段说真话**）。改动任何一份 `target-dir` 后必须核验：

```bash
cd bedcode-desktop/wasm-apps/<app-id>/rust && cargo metadata --no-deps --format-version 1
cd bedcode-desktop/packages/<fixture-crate> && cargo metadata --no-deps --format-version 1
```

两者的 `target_directory` 应分别是 `bedcode-desktop/target/wasm-apps` 与
`bedcode-desktop/target/fixtures`；server-lib 六个 crate 应为仓库根 `target/server-libs`，
机制内核 + 能力域两 crate（`packages/bedcode-host-kit` 与桌面 `packages/` 下的
`bedcode-discovery-engine`）应为仓库根 `target/host-kits`。

同一串 `../../target/wasm-apps` 在两处含义不同，别混：`build.js` 把它作为
`--target-dir` 传给 cargo，命令行参数按**进程 cwd**（应用根 `wasm-apps/<app>/`）解析；
`.cargo/config.toml` 的 `target-dir` 按**config 所在 `.cargo/` 的父目录**解析。两处
恰好都指向 `bedcode-desktop/target/wasm-apps`，但这是「基准不同 + 层数相同」的巧合，
改任一处都要重算层数。

`cross-end-tests/target/` **不并入任何端内目录**：它的依赖图是两端 lib 的**并集**
再加自己的 dev 依赖（tauri / actix-web 用于取类型），与任一端都不同——并入会驱逐该端
缓存；且端内 target 有 15G 自动 `cargo clean` 阈值，混在一起会统计失真并连带清掉
跨端缓存（与 `fixtures` / `wasm-apps` 不并入宿主同因）。代价是它的依赖图**整份重新
编译一次**（首次构建分钟级、峰值十几 GB）——换来的是两端缓存互不干扰。

`fixtures` 与 `wasm-apps` **刻意不合并**：夹具 crate 有
`[profile.release] opt-level="s"/lto=true`，wasm 应用无 `[profile.*]`（cargo 默认）；
profile 参与 cargo 产物指纹，同一目录会为同一份依赖图产出两份产物。
两个目录也**不并入**各自宿主 `src-tauri/target`：那里有 15G 阈值与自动 `cargo clean`，
混在一起会统计失真且清宿主缓存时连带清掉夹具缓存。

治理前后实测（2026-09-26，本机 157G ext4）：

| 状态 | 整仓 Rust 产物 | 备注 |
| --- | --- | --- |
| 治理前峰值 | **15.3 GiB**（11 夹具 6.0G + 4 应用 5.8G + 两端宿主 15G） | 跑一次全量测试会多出十余个 per-crate 目录 |
| 治理后稳态 | 夹具 417M（9 个）+ 应用 150M（4 个）合入 2 个目录 | 依赖图各只编译一份 |

**自动检查脚本**：`bedcode-{desktop,mobile}/scripts/check-target-size.js`

```javascript
const CONFIG = {
  maxSizeGB: 15,        // 仅约束宿主 src-tauri/target，超阈值执行 cargo clean
  sharedTargetDirs: [...],    // 共享目录：只报告，不自动删（删=丢共享编译缓存）
  legacyTargetParents: [...], // 遗留 per-crate 目录：报告为「可安全删除」
  // 仓库根级落点：只报告（依赖图各自独立；删=下次重编）
  rootTargetDirs: ['target/server-libs', 'cross-end-tests/target'],
}
```

**pnpm scripts**：

```bash
pnpm run target:size    # 检查：宿主走阈值判定，其余 target 目录逐个列出大小
pnpm run target:clean   # 手动清理宿主 target（cd src-tauri && cargo clean）
```

**构建前自动检查**：`pnpm run build` 会自动执行检查。

### Target 治理方案决策记录（2026-09-26）

> 整理自 2026-09-26 target 目录治理专项 spec（2026-09-27 迁入 docs）。

**为什么宿主侧不能共享**（先排除的方案）：两端 wasmtime 版本分叉（桌面 48 / 移动 48，cargo 按
版本分产物）、目标三元组不同（移动端 `aarch64-linux-android`）、feature 集不同（tauri /
tauri-plugin-*）——宿主侧 14G 是**活产物**（`deps/` 里几乎每个 crate 只有 1 个哈希版本，陈旧残留仅
~10M），不是垃圾，是必要成本。真正的浪费在 12~15 个小独立 target 上。

**方案评估（采纳 / 否决）**：

| 方案 | 预计回收 | 决策 |
| --- | --- | --- |
| A 夹具共享 target | 6.0G → ~1G | **采纳** |
| B 桌面 wasm 应用共享 target | 5.8G → ~4G | **采纳** |
| C 移动端夹具共享 target | ~0.35G → ~0.2G | **采纳**（低成本，同构） |
| D 两端宿主共享 target | 估 2~3G | **不做**：编译期独占锁使并发构建串行化；`cargo clean` 爆炸半径覆盖全端；且去重空间有限（见上） |
| E 单一根 workspace | 增量有限 | **不做**：单一 `Cargo.lock` 耦合 wasmtime 分叉；stable vs nightly 工具链冲突；workspace feature 统一污染 wasm 产物；`cargo build --workspace` 会按宿主三元组编译 wasm 应用 |
| F sccache | 不省空间 | **2026-09-26 否决 → 2026-10-05 复核后引入**（见下「sccache 编译缓存」节）：当初理由「sccache 不缓存增量编译单元（需 `CARGO_INCREMENTAL=0`）、dev 迭代更慢」在现代 sccache（1.7+ 检测 `-Zincremental` 透传、依赖全量照常缓存）已不成立；引入动机不是省空间，而是**clean / 换桶后依赖不重编** |
| G btrfs + compress=zstd | 14G → 5~7G | **不做**：需独立分区，loop 挂载性能损失不可接受 |
| H 定期回收 | 立即 ~4G | **采纳**（Step 0 + 监控脚本扩展） |

**实施中发现的关键事实**：clippy / rust-analyzer 之类的工具链探针会在应用 crate 目录内跑
`cargo check`，绕过 `build.js` 显式传的 `--target-dir`，静默重建 per-crate 目录——因此
`packages/.cargo/config.toml` 与 `wasm-apps/.cargo/config.toml` 也是真源（cargo 按 cwd 祖先链
发现配置，与 `--manifest-path` 无关）；两份 config 均**不影响宿主构建**（`packages/` /
`wasm-apps/` 不是 `src-tauri/` 的祖先）。

**已知遗留**：移动端发布态 SDK CLI（`packages/plugin-sdk-mobile/bin/cli.js`）三处硬编码
`rust/target/...` 产物路径未共享（约 500M 量级，改动需连带 SDK 评估）；`[profile.release.build-override]`
（宿主导 proc-macro 降优化，理论可再省数百 M）未做——留作后续评估项。

**验证结果（2026-09-26）**：夹具共享（桌面 9→1 目录 417M、移动 2→1 目录 334M）、wasm 应用共享
（4→1 目录 150M）后过滤测试全绿；桌面 `cargo test --no-fail-fast` 890 + 集成 9 项（2 项失败均非
本任务）；前端桌面 844 / 移动 467 passed；`pnpm exec eslint .` 0 errors；Step 0 即时回收 3.9G
（15.3G → 11.4G）。

**增量目录可随时删**（代价：本地 crate 下次全量重编，依赖产物不受影响）：

```bash
rm -rf bedcode-desktop/src-tauri/target/debug/incremental
rm -rf bedcode-mobile/src-tauri/target/debug/incremental
```

> 提醒：`cargo test` 会把 `test` profile 的依赖产物（`debug_assertions` 开启）与 `dev`
> profile 的并排存一份，宿主 target 因此会在「构建 + 跑测试」后明显增长（实测
> `incremental` 单项可达 2.5G）。这是正常成本，不是泄漏。

### sccache 编译缓存（2026-10-05 引入，决策记录方案 F 的复核落地）

**动机**：上节治理解决「同一依赖图不重复占盘」，但两个痛点仍在——① 宿主 target 15G
阈值自动 `cargo clean` 后依赖全量重编（分钟~小时级）；② 8 个落点 + 30+ 独立 crate 的
依赖图互不共享（cross-end 的并集图、server-libs 与 host-kits 各编一份 wasmtime）。
sccache 按 rustc 调用内容哈希缓存编译产物，**独立于 target 目录**，两者一并解决。

**配置**（仓库根 `.cargo/config.toml`，对所有 crate 生效——cargo 按 cwd 祖先链合并
config，子 config 无 `rustc-wrapper` 时继承根的）：

```toml
[build]
rustc-wrapper = "sccache"   # bare 名经 PATH 解析（~/.cargo/bin 已装）

[env]
SCCACHE_CACHE_SIZE = { value = "20GiB", force = false }  # 外部变量优先（CI 设 2GiB）
```

**与增量编译共存**（复核否决理由的关键）：cargo 只对本地 crate 开 incremental
（`-Zincremental`），sccache 检测到该参数时透传不缓存；依赖 crate 的全量编译照常缓存。
本地 dev 迭代速度不变，`cargo clean` / 删 target / 换桶后依赖秒级恢复（2026-10-05 实测
`bedcode-host-kit`：wasmtime 全量编译 3m34s → clean 后缓存命中重建 37s，244 次编译 100% 命中）。

**安装（显性失败设计）**：sccache 未安装时 cargo 直接报错（找不到 wrapper），不会静默
降级——装好即用，装法与各平台差异见根 `.cargo/config.toml` 注释。

**CI 与本地差异**：CI 不启用 sccache 的 GHA cache 后端（不设 `SCCACHE_GHA_ENABLED`）——
跨 run 缓存由 `Swatinem/rust-cache`（target）负责，sccache 只做 job 内跨 crate 命中
（server-libs / wasm-apps 循环共享依赖）；CI job 级 `SCCACHE_CACHE_SIZE=2GiB`
（ubuntu-latest 磁盘 ~14G）经根 config 的 `force=false` 覆盖 20GiB 默认。

**缓存管理**：缓存目录 `~/.cache/sccache`（`SCCACHE_DIR` 可改），与 check-target-size.js
**互不相关**——不在 target 内，15G 阈值 `cargo clean` 不碰它（这正是设计意图：clean 后
由 sccache 恢复依赖）。清缓存：`sccache --stop-server && rm -rf ~/.cache/sccache`。
磁盘占用是 target 之外的额外一份压缩产物（zstd，约 target 的 1/3），20GiB 上限封顶，
紧张时降 8~10GiB。

---

## 常用构建命令

### 开发模式

```bash
# 启动前端开发服务器
pnpm run dev

# 启动 Tauri 开发模式 (桌面端)
pnpm run tauri:dev

# 浏览器模拟移动端
# 访问 http://localhost:1420?platform=mobile
```

### 桌面端构建

```bash
# 构建生产版本
pnpm run tauri:build

# 仅构建前端
pnpm run build
```

### Android 构建

```bash
# 初始化 Android 项目 (首次)
pnpm run tauri:android:init

# 构建 Android APK
pnpm run tauri:android:build

# 构建 Release 版本
pnpm exec tauri android build --release

# 使用 Android Studio 打开
# File → Open → src-tauri/gen/android
```

### 清理命令

```bash
# 清理前端构建产物
rm -rf dist/

# 清理 Rust 构建产物
pnpm run target:clean
# 或
cd src-tauri && cargo clean

# 清理 Android 构建产物
cd src-tauri/gen/android && ./gradlew clean
```

### 测试命令

```bash
# 前端单元测试
pnpm run test

# 前端测试覆盖率
pnpm run test:coverage

# Rust 测试
cd src-tauri && cargo test

# E2E 测试
pnpm run test:e2e
```

---

## 总结

| 组件 | 桌面端 | Android |
|------|--------|---------|
| **前端代码** | 打包在一起，运行时区分 | 相同 |
| **Rust 库** | rlib 静态链接 | cdylib 动态库 (.so) |
| **平台检测** | `@tauri-apps/plugin-os` | 相同 |
| **布局渲染** | `v-if="isDesktop"` | `v-else` |
| **特有功能** | PTY, WebSocket 服务端, mDNS | 无 (客户端模式) |
| **构建工具** | cargo + tauri-cli | Gradle + RustPlugin |

**核心设计原则**：

1. **一份前端代码，多平台运行** - 通过运行时平台检测实现
2. **条件编译隔离平台差异** - Rust 通过 `cfg` 属性实现
3. **统一的构建入口** - `tauri-cli` 协调所有平台的构建流程
