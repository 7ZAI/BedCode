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
name = "bedcode_lib"
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

| 目标平台 | 编译目标 | 产物类型 | 产物名称 |
|---------|---------|---------|---------|
| Windows | x86_64-pc-windows-msvc | .dll | bedcode_lib.dll |
| macOS | aarch64-apple-darwin | .dylib | libbedcode_lib.dylib |
| Linux | x86_64-unknown-linux-gnu | .so | libbedcode_lib.so |
| Android | aarch64-linux-android | .so | libbedcode_lib.so |
| iOS | aarch64-apple-ios | .a | libbedcode_lib.a |

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
│    输出: libbedcode_lib.so                                   │
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
│    产物: libbedcode_lib.so / .dll / .dylib                   │
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

Rust 增量编译会导致 `target` 目录持续增长。

**自动清理脚本**：`scripts/check-target-size.js`

```javascript
const CONFIG = {
  maxSizeGB: 15,        // 最大允许 15GB
  targetDir: 'src-tauri/target',
  autoClean: true,      // 超过阈值自动清理
}
```

**pnpm scripts**：

```bash
pnpm run target:size    # 检查 target 目录大小
pnpm run target:clean   # 手动清理 target 目录
```

**构建前自动检查**：`pnpm run build` 会自动执行检查。

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
