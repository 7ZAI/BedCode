# {{NAME}}

{{NAME}} wasm 应用 — BedCode Desktop wasm 应用工程（由 `bedcode-plugin-desktop create` 生成）。

> 术语：本仓把「插件」称为 wasm 应用；`plugin.json` / CLI 命令 / SDK 包名仍用 `plugin` 原词。

## 快速开始

```bash
pnpm install        # 安装依赖（含 SDK、vite、vue）
pnpm run dev        # 浏览器开发环境（Dev Shell：空壳宿主 + 桌面端骨架，HMR）
pnpm run build      # 构建：vite（前端）+ cargo（WASM，仅 rust-ts 插件）
pnpm run manifest   # 按源码自动填充 plugin.json 的 contributes/permissions
```

## 目录结构

```
├── plugin.json        # 应用清单（id/name/权限/扩展点声明）
├── package.json       # 工程脚本：dev / build / manifest
├── vite.config.ts     # 前端构建配置（产物 dist/index.js，vue 等外部化到宿主）
├── tsconfig.json
├── src/
│   ├── index.ts       # 前端入口：activate(context) / deactivate()
│   └── env.d.ts       # 环境类型声明
└── rust/              # WASM 后端（--rust 创建；ts-only 应用无此目录）
    ├── Cargo.toml
    └── src/lib.rs     # WasmPlugin 实现（含 manifest 声明）
```

## 安装到 BedCode Desktop

`pnpm run build -- --resources-dir <宿主资源父目录>` 把产物复制到
`<父目录>/{id}/`（内置应用随安装包分发）；用户安装走宿主应用管理。

> 权限提醒：桌面端**无默认授予**——`storage` 等权限均需写入 `plugin.json` 的 `permissions`，
> 未声明即被拒（内置应用均显式声明了 `storage`）。

完整开发指南见仓库文档 `../../../wasm-app-dev-desktop.md`。
