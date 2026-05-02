# BedCode

通过移动设备远程控制 Claude Code 的跨平台应用。

## 功能特性

- 🖥️ **桌面端**: 管理会话配置、配对设备、系统托盘
- 📱 **移动端**: 设备发现、终端显示、快捷指令、历史记录
- 🔐 **安全配对**: 6位数字配对码验证
- 🌐 **局域网通信**: WebSocket (WSS) + mDNS 自动发现
- 🐧 **WSL2 支持**: 在 Windows 上运行 WSL2 环境
- 💻 **Tmux 支持**: 连接现有 Tmux 会话
- 📝 **输出解析**: ANSI 解析、Markdown 渲染、代码高亮
- 🔔 **智能通知**: 等待输入提醒、设备连接通知

## 技术栈

| 层级 | 技术 |
|------|------|
| 桌面端 | Tauri 2.0 + Vue 3 + TypeScript + TailwindCSS |
| 移动端 | Tauri 2.0 (Android) / Capacitor (备选) |
| 后端 | Rust |
| 数据库 | SQLite |
| 通信 | WebSocket (WSS) + mDNS |
| 安全 | X25519 密钥交换 + AES-GCM 加密 |

## 项目结构

```
bedcode/
├── src/                    # Vue 前端源码
│   ├── components/         # Vue 组件
│   │   ├── desktop/        # 桌面端组件
│   │   ├── mobile/         # 移动端组件
│   │   └── common/         # 通用组件
│   ├── views/              # 页面视图
│   │   ├── desktop/        # 桌面端视图
│   │   └── mobile/         # 移动端视图
│   ├── stores/             # Pinia 状态管理
│   ├── router/             # Vue Router 路由
│   └── composables/        # Vue 组合式函数
├── src-tauri/              # Rust 后端源码
│   ├── src/
│   │   ├── db/             # 数据库模块
│   │   ├── pty/            # PTY 管理
│   │   ├── session/        # 会话管理
│   │   ├── auth/           # 认证配对
│   │   ├── discovery/      # mDNS 发现
│   │   ├── websocket/      # WebSocket 服务
│   │   ├── parser/         # 输出解析
│   │   └── notify/         # 通知服务
│   └── tauri.conf.json     # Tauri 配置
└── docs/                   # 文档
    ├── superpowers/specs/  # 设计文档
    └── implementation-plans/ # 实现计划
```

## 快速开始

### 前置要求

1. **Node.js** >= 18
2. **Rust** >= 1.70
3. **Tauri CLI** 2.0

### 安装

```bash
# 克隆仓库
git clone https://github.com/your-repo/bedcode.git
cd bedcode

# 安装依赖
npm install
```

### 开发

```bash
# 开发模式
npm run tauri dev
```

### 构建

```bash
# 构建生产版本
npm run tauri build
```

## 平台特定说明

### Windows

Windows 平台原生支持，无需额外配置。

### WSL2

支持在 WSL2 环境中运行 Claude Code：
- 自动检测 WSL 发行版
- 路径自动转换 (Windows ↔ WSL)
- 支持所有 WSL 发行版

### Linux

```bash
# 安装系统依赖
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev
```

### Android

```bash
# 添加 Android 目标
npm run tauri android init

# 构建 Android APK
npm run tauri android build
```

## 使用指南

### 桌面端

1. 启动应用后，创建会话配置
2. 选择执行环境（Windows 原生或 WSL2）
3. 设置工作目录和启动命令
4. 点击"生成配对码"
5. 在移动端输入配对码完成配对

### 移动端

1. 打开应用，自动扫描附近设备
2. 或手动输入设备 IP 地址
3. 输入桌面端显示的配对码
4. 配对成功后，点击设备连接
5. 查看终端输出，发送输入

### 快捷指令

预设指令：
- ▶️ 继续 - 发送 "请继续"
- 📝 解释代码 - 发送 "请解释这段代码的作用"
- 🔧 修复 Bug - 发送 "请帮我修复这个 Bug"
- 📤 提交代码 - 发送 "请帮我提交代码"

可自定义添加更多快捷指令。

## 开发命令

```bash
# 前端开发服务器
npm run dev

# Tauri 开发模式
npm run tauri dev

# 构建生产版本
npm run tauri build

# 代码检查
npm run lint

# 代码格式化
npm run format
```

## 实现进度

| 阶段 | 名称 | 状态 |
|------|------|------|
| Phase 1 | 项目骨架和核心基础设施 | ✅ 已完成 |
| Phase 2 | PTY Manager 和会话管理 | ✅ 已完成 |
| Phase 3 | 网络通信和安全 | ✅ 已完成 |
| Phase 4 | 桌面端 UI | ✅ 已完成 |
| Phase 5 | 移动端 UI | ✅ 已完成 |
| Phase 6 | 增强功能和完善 | 🔄 进行中 |

详见 [实现计划](./docs/implementation-plans/README.md)

## 文档

- [设计文档](./docs/superpowers/specs/2026-04-30-bedcode-design.md)
- [实现计划总览](./docs/implementation-plans/README.md)

## 安全说明

- 所有通信使用 WebSocket Secure (WSS)
- 设备配对使用 X25519 密钥交换
- 配对码 60 秒后自动过期
- 密钥存储使用系统安全存储 (Windows Credential Manager / Linux Secret Service)

## 已知问题

- Tauri Android 支持仍在完善中，部分功能可能不稳定
- 大量输出时可能需要手动滚动

## 贡献

欢迎提交 Issue 和 Pull Request！

## 许可证

MIT
