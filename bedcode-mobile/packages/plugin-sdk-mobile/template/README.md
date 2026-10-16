# {{NAME}}

{{NAME}} wasm 应用 — BedCode Mobile wasm 应用工程（由 `bedcode-plugin create` 生成）。

> 术语：本仓把「插件」称为 wasm 应用；`plugin.json` / CLI 命令 / SDK 包名仍用 `plugin` 原词。

## 快速开始

```bash
pnpm install        # 安装依赖（含 SDK、vite、vue）
pnpm run dev        # 浏览器开发环境（Dev Shell：空壳宿主 + 移动端骨架，HMR）
pnpm run build      # 构建：vite（前端）+ cargo（WASM）
pnpm run package    # 打包 dist/{{ID}}.zip 插件包
```

`pnpm run dev` 首次运行自动安装 dev-shell 依赖，浏览器打开 http://localhost:5173；
WASM 后端命令与真机专属能力仍需真机验证（见下方指南）。

## 安装到 BedCode Mobile

1. 将 `dist/{{ID}}.zip` 传到手机
2. BedCode Mobile → 插件管理 → 从文件安装，选择 zip（支持远程下载 + SHA256 校验）

## 目录结构

```
├── plugin.json        # 应用清单（id/name/权限/扩展点声明）
├── package.json       # 工程脚本：dev / build / package
├── vite.config.ts     # 前端构建配置（产物 dist/index.js）
├── tsconfig.json
├── src/
│   ├── index.ts       # 前端入口：activate(context) / deactivate()
│   └── env.d.ts       # 环境类型声明
├── rust/
│   ├── Cargo.toml     # Rust 后端（wasm32-unknown-unknown；--ts-only 创建时无此目录）
│   └── src/lib.rs     # WasmPlugin 实现（含 manifest 声明）
└── dist/              # 构建产物 + 应用包 zip
```

## 常用修改点

- **权限**：`plugin.json` 的 `permissions` 数组按需声明（**移动端 `storage` 默认授予且不可关闭**，无需声明；其余权限未声明即被拒）
- **扩展点**：`plugin.json` 的 `contributes`（命令、视图、navTab、终端工具栏、设置区）
- **前端逻辑**：`src/index.ts` 使用 `context` 提供的 API（`ui`/`events`/`storage`/`logger`/`dialogs` 等）

完整开发指南见仓库文档 `../../../wasm-app-dev-mobile.md`。
