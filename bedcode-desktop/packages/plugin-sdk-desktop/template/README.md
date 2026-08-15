# {{NAME}}

{{NAME}} 插件 — BedCode Desktop 插件工程（由 `bedcode-plugin-desktop create` 生成）。

## 快速开始

```bash
npm install        # 安装依赖（含 SDK、vite、vue）
npm run dev        # 浏览器开发环境（Dev Shell：mock 宿主 + 桌面端骨架，HMR）
npm run build      # 构建：vite（前端）+ cargo（WASM，仅 rust-ts 插件）
npm run manifest   # 按源码自动填充 plugin.json 的 contributes/permissions
```

## 目录结构

```
├── plugin.json        # 插件清单（id/name/权限/扩展点声明）
├── vite.config.ts     # 前端构建配置（产物 dist/index.js，vue 等外部化到宿主）
├── src/index.ts       # 前端入口：activate(context) / deactivate()
└── rust/              # WASM 后端（--rust 创建；ts-only 插件无此目录）
    ├── Cargo.toml
    └── src/lib.rs     # WasmPlugin 实现（含 manifest 声明）
```

## 安装到 BedCode Desktop

`npm run build -- --resources-dir <宿主资源父目录>` 把产物复制到
`<父目录>/{id}/`（内置插件随安装包分发）；用户安装走宿主插件管理。

完整开发指南见仓库文档 `../../../plugin-dev-desktop.md`。
