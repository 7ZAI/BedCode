# {{NAME}}

{{NAME}} 插件 — BedCode Mobile 插件工程（由 `bedcode-plugin create` 生成）。

## 快速开始

```bash
npm install        # 安装依赖（含 SDK、vite、vue）
npm run dev        # 浏览器开发环境（Dev Shell：mock 宿主 + 移动端骨架，HMR）
npm run build      # 构建：vite（前端）+ cargo（WASM）
npm run package    # 打包 dist/{{ID}}.zip 插件包
```

`npm run dev` 首次运行自动安装 dev-shell 依赖，浏览器打开 http://localhost:5173；
WASM 后端命令与真机专属能力仍需真机验证（见下方指南）。

## 安装到 BedCode Mobile

1. 将 `dist/{{ID}}.zip` 传到手机
2. BedCode Mobile → 插件管理 → 从文件安装，选择 zip

## 目录结构

```
├── plugin.json        # 插件清单（id/name/权限/扩展点声明）
├── vite.config.ts     # 前端构建配置（产物 dist/index.js）
├── src/index.ts       # 前端入口：activate(context) / deactivate()
├── rust/
│   ├── Cargo.toml     # Rust 后端（wasm32-unknown-unknown）
│   └── src/lib.rs     # WasmPlugin 实现（含 manifest 声明）
└── dist/              # 构建产物 + 插件包 zip
```

## 常用修改点

- **权限**：`plugin.json` 的 `permissions` 数组（`storage` 默认授予，其余按需声明）
- **扩展点**：`plugin.json` 的 `contributes`（命令、视图、navTab、终端工具栏、设置区）
- **前端逻辑**：`src/index.ts` 使用 `context` 提供的 API（`ui`/`events`/`storage`/`logger`/`dialogs` 等）

完整开发指南见仓库文档 `../../../plugin-dev-mobile.md`。
