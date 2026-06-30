# 代码查看设置弹窗设计

## 概述

为移动端代码查看组件（CodeExplorerView 和 FileViewerModal）添加设置按钮和设置弹窗，支持动态调整代码字体大小、主题、Tab 缩进和行号显示。

## 需求

- CodeExplorerView 标题栏和 FileViewerModal 标题栏添加设置按钮
- 设置弹窗参考终端设置弹窗的 UI 模式
- 两个组件共享设置状态
- 设置持久化到 localStorage（纯前端，不经过 Rust 后端）

## 数据层

### Pinia Store: `useCodeViewerStore`

位置：`src/modules/shared/stores/codeViewer.ts`

```typescript
interface CodeViewerSettings {
  fontSize: number       // 10-24, 默认 13
  theme: string          // 主题 ID，默认 'vitesse-dark'
  tabSize: number        // 2 | 4 | 8, 默认 4
  showLineNumbers: boolean // 默认 true
}
```

- 使用 localStorage 手动持久化（`pinia-plugin-persistedstate` 需要额外依赖，手写更轻量）
- 提供 `saveSettings()` / `resetSettings()` 方法
- 默认值与当前硬编码行为一致

### 可选主题

| 主题 ID | 显示名 | 来源 |
|---------|--------|------|
| vitesse-dark | Vitesse Dark | `@shikijs/themes/vitesse-dark` |
| one-dark-pro | One Dark Pro | `@shikijs/themes/one-dark-pro` |
| nord | Nord | `@shikijs/themes/nord` |
| github-dark | GitHub Dark | `@shikijs/themes/github-dark` |
| monokai | Monokai | `@shikijs/themes/monokai` |

### useCodeHighlight 改造

- `ensureHighlighter()` 初始化时加载所有 5 个主题
- `highlight()` / `highlightDiff()` 新增 `theme` 参数，传给 `codeToHtml()`
- 默认 theme 参数为 `'vitesse-dark'`，保持向后兼容

## 设置弹窗组件

### `CodeViewerSettingsModal.vue`

位置：`src/modules/mobile/components/CodeViewerSettingsModal.vue`

**Props:**
```typescript
{
  visible: boolean
}
```

**Emits:**
```typescript
{
  close: []
  confirm: [settings: CodeViewerSettings]
}
```

**UI 结构：**

1. **字体大小** — `[-] 13px [+]` 按钮组，范围 10-24px
2. **代码主题** — 3 列网格，每个主题按钮显示背景色 + 前景色 + "Aa" 预览文字
3. **Tab 缩进** — `[2] [4] [8]` 按钮组，选中项高亮
4. **行号显示** — toggle 开关

**交互：**
- 打开时从 store 读取当前设置到临时变量
- 编辑只修改临时变量
- "确认"：保存到 store + emit confirm
- "取消"：丢弃修改 + emit close

**UI 风格：** 与终端设置弹窗保持一致（overlay + modal + header/content/footer）

## 集成方式

### CodeExplorerView

1. `header-meta` 区域添加齿轮图标设置按钮
2. 点击 → `showSettings = true` → 打开 `CodeViewerSettingsModal`
3. 字体大小和 Tab 缩进通过 CSS 变量注入，无需重新高亮
4. 主题和行号变化需要重新高亮

### FileViewerModal

1. `viewer-actions` 区域添加齿轮图标设置按钮（fullscreen 按钮左侧）
2. 同样的弹窗 + 重新高亮机制

### CSS 变量驱动字体大小和缩进

```css
.code-content {
  font-size: var(--code-font-size, 13px);
  tab-size: var(--code-tab-size, 4);
}
```

组件绑定 style：
```vue
<div class="code-content" :style="codeStyle" v-html="highlightedHtml"></div>
```

```typescript
const codeStyle = computed(() => ({
  '--code-font-size': `${store.settings.fontSize}px`,
  '--code-tab-size': store.settings.tabSize,
}))
```

### 行号开关（纯 CSS）

```css
.code-content.hide-line-numbers :deep(.line::before) {
  content: none;
}
.code-content.hide-line-numbers :deep(.line) {
  padding-left: 0.5em;
}
```

Diff 行号同理：
```css
.code-content.hide-line-numbers :deep(.diff-line-no) {
  display: none;
}
```

## 涉及文件

| 文件 | 变更 |
|------|------|
| `src/modules/shared/stores/codeViewer.ts` | 新建 - Pinia store |
| `src/modules/mobile/components/CodeViewerSettingsModal.vue` | 新建 - 设置弹窗 |
| `src/modules/mobile/composables/useCodeHighlight.ts` | 修改 - 多主题支持 + theme 参数 |
| `src/modules/mobile/views/CodeExplorerView.vue` | 修改 - 设置按钮 + 弹窗 + CSS 变量 |
| `src/modules/mobile/components/FileViewerModal.vue` | 修改 - 设置按钮 + 弹窗 + CSS 变量 |
