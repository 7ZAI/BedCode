# 代码查看器（Code Viewer）Spec

状态：设计已与用户全部敲定（grill-with-docs 两轮，14 项决策）
领域词汇：见 `CONTEXT.md` → 代码查看 (Code Viewer) 节；架构决策：`docs/adr/0023-*.md`（窗口扩大几何）、`docs/adr/0024-*.md`（宿主内置非插件）

## 背景

桌面端终端窗口是独立 WebviewWindow（`terminal-<sessionId>`，约主窗口 60% 宽、贴主窗口右侧）。vibe coding 场景：Claude Code 在终端产出，用户需要并排查看代码文件。核心张力：终端渲染画面必须稳定（PTY cols/rows 重协商会整屏 reflow）。

## 需求基线（已敲定决策，全部生效）

### 形态与归属
1. **宿主内置功能**（非 WASM 插件）——ADR 0024：新增 Rust 命令域 + Vue 组件 + composable，工具栏按钮直进终端窗口 header（插件扩展点之前）；不扩展插件 fs 能力面
2. **只读查看器**，不提供编辑（v2 再议）
3. 面向前端的最小组件集：`CodeViewerPanel`（容器）/ `FileTree` / `CodeTabs` / `CodeViewer`；业务逻辑在 `useCodeViewer` composable（AGENTS.md 规范）

### 根目录锚定（Root Anchoring）
4. 默认根 = 会话 `SessionConfig.working_dir`（PTY 启动目录）；提供文件夹切换按钮；按 sessionId 记忆 root
5. 切换锚点**不**自动跳到 git 仓库根

### 窗口几何（ADR 0023）
6. 展开 = 终端窗口向右扩大固定 420 逻辑像素，代码面板占新空间、位于终端视口右侧
7. 展开期间终端视口保持展开前像素尺寸——PTY cols/rows 不重协商
8. 面板打开后用户手动拖拽变宽 → 终端视口锚定不再谈判，面板吸收全部变化
9. 收起严格恢复展开前窗口矩形（位置+尺寸）
10. 屏幕右缘钳制；剩余空间 < 300 逻辑像素则拒绝展开并提示（不左移压主窗口）

### 标签与状态
11. 标签上限 16，超出提示；关闭 = × 按钮 + 鼠标中键；无右键菜单（v2）
12. 状态按 sessionId 持久化 localStorage：root / 打开的标签 / 活动标签（窗口关闭重启恢复）

### 刷新机制
13. 手动刷新按钮（覆盖整树）+ 活动标签文件 mtime 轮询（~1s）自动重载（`stat_code_file` 轻量轮询，变化才重新 read）——ADR 0023 的「自动重载」术语

### 视图能力
14. 行号 + `highlight.js` core 按需加载（约 15 种常用语言）+ wrap 开关 + 独立字号（复用 wb-mono 体系）
15. 大文件 cap：1MB / 2 万行，超限**不自动打开**并提示「文件过大，仅显示前 N 行」；Rust 端读命令做防御性截断
16. 文件内 Ctrl+F（面板独立搜索条；xterm 焦点键路由不做——无全局快捷键，v1 纯工具栏按钮入口）
17. 文件编码：UTF-8 优先，非法字节自动回退 GBK 解码（`encoding_rs`）

### 树行为
18. 单击文件打开标签、单击目录展开/收起（VS Code 风格）；懒加载
19. 默认隐藏 dotfiles +「显示隐藏文件」开关；`node_modules` 不做特殊处理

## 非目标（明确排除，v1.5+ 再议）

- 全仓 grep 全局搜索（架构留口：根目录锚定机制复用之）
- 文件编辑 / diff / 虚拟滚动 / 分割线拖拽宽度
- 全局快捷键 / 右键菜单 / git 仓库根自动跳转
- WASM 插件形态 / 插件感知代码面板（走既有消息总线，不改归属）

## 文件地图（预估）

| 动作 | 路径 |
|------|------|
| 新增 | `src-tauri/src/commands/code_viewer.rs` + `code_viewer_test.rs` |
| 新增 | `src/components/CodeViewerPanel.vue` / `FileTree.vue` / `CodeTabs.vue` / `CodeViewer.vue` |
| 新增 | `src/composables/useCodeViewer.ts`（+ 单测） |
| 修改 | `src-tauri/src/lib.rs`（模块声明 + invoke_handler 注册） |
| 修改 | `src/composables/useSessionWindows.ts`（展开/收起几何，pre-expand rect） |
| 修改 | `src/views/TerminalWindowView.vue`（工具栏按钮 + 布局：终端视口固定宽度、面板 flex） |
| 修改 | `src/locales/zh-CN.ts` / `en.ts`（desktop.terminal.* 下新增 key） |
| 依赖 | 前端 `highlight.js`；Rust `encoding_rs` |

## 验收总纲

- `cargo test` 全绿（含 code_viewer 单测：目录穿越/符号链接逃逸/GBK 回退/截断）
- `pnpm run test:run` 全绿（composable 单测：上限/持久化/root）
- i18n key 双端同步出现
- 展开/收起几何：终端视口像素尺寸在展开期间不变（手动 dev 验证）；收起恢复原矩形
- UI 改动遵循 `frontend-styles` skill（token-bound、无原生控件外观）
- 错误处理 `AppError` 带上下文，禁止裸字符串

## 实施顺序（blockers-first，见 tickets）

01 后端命令域（无依赖）→ 02 窗口几何（无依赖）→ 03 状态 composable（←01）→ 04 面板 UI 集成（←02,03）→ 05 i18n 与端到端收尾（←04）