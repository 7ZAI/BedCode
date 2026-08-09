# Modern CSS Features

CSS Layers、@property、:has()、View Transitions——主 SKILL 之外能显著提升代码质量的现代特性。

> **何时加载**：解决优先级冲突、动画 CSS 变量、简化条件样式、页面切换动画时。
>
> **支持基线**：2024+ 主流浏览器，**移动端 Android 5+ / iOS 15+** 可用。

---

## CSS Layers (`@layer`)

控制样式优先级，**避免 `!important` 滥用**和"工具类被覆盖"的混乱。

### 三层结构（推荐）

```css
/* main.css */
@layer reset, base, components, utilities;

@layer reset {
  *, *::before, *::after { box-sizing: border-box; }
  body { margin: 0; }
}

@layer base {
  body {
    font-family: 'Inter', system-ui, sans-serif;
    color: var(--text-primary);
  }
}

@layer components {
  .btn { /* 永远低于 utilities */ }
  .card { /* 永远低于 utilities */ }
}

@layer utilities {
  .hidden { display: none; }
  .bg-card { background: var(--bg-card); }
}
```

**优先级规则**：`reset < base < components < utilities`（后定义层级高的优先级低，但**同层内后定义覆盖前定义**）。

**这意味着**：
- 任何 `utilities` 层的工具类永远赢过 `components` 层
- 无需 `!important`
- 第三方组件样式自动隔离

### Tailwind 集成

Tailwind v3 默认在 `utilities` 层。把项目自定义组件放进 `components` 层：

```css
/* main.css */
@layer tailwind-base, components, tailwind-utilities;

@import 'tailwindcss/base' layer(tailwind-base);
@import 'tailwindcss/components' layer(tailwind-utilities);  /* 注意：Tailwind 的 components 工具类 */
@import 'tailwindcss/utilities' layer(tailwind-utilities);

/* 自己的组件放进 components 层 */
@layer components {
  .btn { ... }
  .card { ... }
}
```

### 在 `<style scoped>` 中指定层级

```vue
<style scoped>
@layer components {
  .my-card { ... }
}
</style>
```

---

## `@property`（类型化 CSS 变量）

让 CSS 变量有类型、可动画、可继承初始值。

### 基础注册

```css
@property --brand-hue {
  syntax: '<number>';
  inherits: true;
  initial-value: 220;
}

@property --brand-saturation {
  syntax: '<percentage>';
  inherits: false;
  initial-value: 70%;
}
```

### 典型用途 1：动画 CSS 变量

```css
.theme-shifter {
  --brand-hue: 220;
  background: hsl(var(--brand-hue) 70% 50%);
  /* 现在可以动画了 */
  transition: --brand-hue 0.3s;
}

.theme-shifter:hover {
  --brand-hue: 280;
}
```

**没注册时**：`transition: --brand-hue 0.3s` 被浏览器忽略，**直接跳变**。

### 典型用途 2：颜色空间

```css
@property --glow {
  syntax: '<color>';
  inherits: false;
  initial-value: transparent;
}

.card:hover {
  --glow: oklch(70% 0.2 250);
  box-shadow: 0 0 20px var(--glow);
  transition: --glow 0.2s;
}
```

### 语法类型

| `syntax` | 取值 |
|----------|------|
| `'<number>'` | 任意数字 |
| `'<integer>'` | 整数 |
| `'<percentage>'` | 百分比 |
| `'<length>'` | 长度 |
| `'<color>'` | 颜色 |
| `'<angle>'` | 角度 |
| `'*"'` | 任意 token（默认） |

### BedCode 应用场景

- **主题切换**：用 `transition: --bg-card 0.2s` 平滑过渡（比 `transition: background-color 0.2s` 更通用）
- **手风琴/折叠动画**：注册 `--expanded: <number> 0..1`，用 `height: calc(var(--expanded) * 100px)`
- **加载进度条**：注册 `--progress: <percentage>`

---

## `:has()` 选择器

**父选择器**——2023 年起广泛支持。简化"子元素状态决定父元素样式"的场景。

### 基础用法

```css
/* 卡片包含图片时减小内边距 */
.card:has(img) {
  padding: 0.5rem;
}

/* 表单字段无效时高亮标签 */
.form-field:has(input:invalid) .label {
  color: var(--color-error);
}

/* 列表为空时显示占位 */
.list:has(> :empty) {
  display: flex;
  align-items: center;
  justify-content: center;
}

/* 父容器获得焦点时强调边框 */
.input-group:has(input:focus) {
  border-color: var(--color-primary);
}
```

### 性能注意

`:has()` **比常规选择器慢**（浏览器要遍历子元素）。规则：
- ✅ 用于静态条件（`:has(img)`、`:has(input:invalid)`）
- ⚠️ 避免用在动画频繁的元素
- ❌ 不要写 `:has(*)`

### BedCode 应用场景

| 场景 | 写法 |
|------|------|
| 表单验证状态 | `.form-field:has(input:invalid) .label` |
| 卡片有图/无图 | `.card:has(> .cover-image) { ... }` |
| 输入框获焦强调 | `.search-bar:has(input:focus) { ... }` |
| 空列表占位 | `.list:has(> :only-child:empty) { ... }` |

---

## View Transitions API

**页面/路由切换动画**——传统方案需要 Vue `<Transition>` 配合路由钩子，原生 API 更直接。

### 单页面过渡

```js
// 路由切换时
function navigate(href: string) {
  if (!document.startViewTransition) {
    // 降级到直接跳转
    location.href = href
    return
  }

  document.startViewTransition(async () => {
    location.href = href
    // 或 history.pushState 触发 SPA 路由
  })
}
```

### 命名元素过渡（跨页共享元素）

```html
<!-- 列表页 -->
<div class="card">
  <img src="thumb.jpg" style="view-transition-name: hero-1" />
</div>

<!-- 详情页 -->
<div class="hero">
  <img src="full.jpg" style="view-transition-name: hero-1" />
</div>
```

浏览器自动从缩略图位置过渡到全图位置。

### BedCode 应用

- 桌面端路由切换（终端、设置、文件浏览器）
- 移动端页面切换（首页 → 详情 → 设置）

> **当前状态**：实验性 API，Chromium 全支持、Safari 18+、Firefox 需开启 `layout.css.view-transitions.enabled`。**做渐进增强**，特性检测后降级。

---

## `subgrid`

嵌套网格中**继承父级轨道**——解决卡片网格"标题长度不一导致参差不齐"的问题。

```css
.grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 1rem;
}

.card {
  display: grid;
  /* 关键：继承父级列定义 */
  grid-template-columns: subgrid;
  grid-row: span 3;  /* title + image + meta */
}
```

**支持现状**（2024+）：Firefox、Safari 16+、Chromium 117+（已稳定）。**移动端可用**。

### BedCode 应用

- 设置项列表（标题 + 描述 + 操作按钮三行对齐）
- 文件浏览器（图标 + 文件名 + 元信息）

---

## `:focus-visible`

> **排除 a11y 章节后，单独说明键盘焦点可见性**——产品面向"正常人"，但仍要考虑鼠标用户用键盘切换（快捷键党、外接键盘等）。

```css
/* 仅键盘 Tab 聚焦时显示焦点环，鼠标点击不显示 */
button:focus-visible {
  outline: 2px solid var(--color-primary);
  outline-offset: 2px;
}
```

Tailwind v3 内置：`focus-visible:ring-2 focus-visible:ring-primary`

**BedCode 建议**：所有交互元素保留 `:focus-visible` 焦点环，**不**完全移除——这是低成本高收益的可用性提升。

---

## Container Queries（补充主 SKILL Mobile 章节）

桌面端也需要容器查询——侧边栏、抽屉、分屏视图里的组件应跟着**父容器**缩放，而非视口。

```css
.sidebar-widget {
  container-type: inline-size;
  container-name: widget;
}

@container widget (min-width: 300px) {
  .sidebar-widget .title {
    display: block;  /* 宽容器显示标题 */
  }
}
```

**适用**：
- 桌面端分屏布局（左 30% / 右 70%）
- 桌面端侧边栏组件
- 桌面端嵌入预览面板

---

## `aspect-ratio`（防御 CLS）

替代 `padding-top` hack 维持宽高比：

```css
.video-frame {
  aspect-ratio: 16 / 9;
}
```

```html
<iframe class="w-full aspect-video" src="..." />
```

---

## Checklist

新组件 / 主题切换 / 复杂表单：

- [ ] 全局样式用 `@layer` 分层（reset / base / components / utilities）
- [ ] 主题过渡考虑 `@property` + `transition: --token 0.2s`
- [ ] 条件样式优先用 `:has()`（表单验证、卡片状态）
- [ ] 路由切换考虑 View Transitions API（带特性检测降级）
- [ ] 跨组件对齐考虑 `subgrid`
- [ ] 焦点环保留 `:focus-visible`
- [ ] 桌面端组件跟父容器缩放用 `@container`
