# Performance

CLS 防御、字体加载策略、图片优化、长列表渲染——补充主 SKILL 未覆盖的运行时性能主题。

> **何时加载**：排查布局抖动、优化首屏、处理长列表/长表格、为图片/视频/动态内容预留空间时。

---

## Cumulative Layout Shift (CLS)

CLS 衡量"视觉元素在页面生命周期内意外移动的程度"。目标：**< 0.1**（Good）。

### 防御清单

#### 1. 图片 / 视频必须声明宽高比

```html
<!-- ❌ 加载完成前塌陷为 0 高度，加载后弹跳 -->
<img src="cover.jpg" class="w-full" />

<!-- ✅ 用 aspect-ratio 预设容器 -->
<img src="cover.jpg" class="w-full aspect-video" />  <!-- 16:9 -->
<img src="avatar.jpg" class="w-12 h-12 rounded-full" />  <!-- 固定尺寸 -->

<!-- ✅ 动态宽高比也用工具类 -->
<div class="aspect-[3/4]">  <!-- 3:4 竖图 -->
  <img src="..." class="w-full h-full object-cover" />
</div>
```

#### 2. 动态内容（Skeleton / Spinner）保持高度稳定

```vue
<!-- ❌ 加载前/后高度不同导致抖动 -->
<div v-if="loading">Loading...</div>
<div v-else>{{ content }}</div>

<!-- ✅ 容器固定最小高度，loading 与内容同高 -->
<div class="min-h-[120px]">
  <Skeleton v-if="loading" />
  <Content v-else :data="content" />
</div>
```

#### 3. Web Font 加载（详见 `I18N.md`）

- `font-display: swap`
- `size-adjust` / `ascent-override` 减少回流
- 预加载关键字体：`<link rel="preload" as="font" href="/fonts/inter-var.woff2" crossorigin>`

#### 4. 异步插入的 DOM 留出占位

```vue
<!-- Modal / Toast / Drawer 弹出时不挤压主内容 -->
<!-- 父容器预留空间或使用 Teleport + fixed 定位（推荐，后者不参与布局） -->
<Teleport to="body">
  <div class="fixed inset-0">...</div>
</Teleport>
```

---

## Image Optimization

### 加载策略

| 场景 | 属性组合 | 说明 |
|------|---------|------|
| 首屏关键图（Above the fold） | `loading="eager" decoding="sync" fetchpriority="high"` | 优先解码 |
| 折叠下方 | `loading="lazy" decoding="async"` | 视口临近时加载 |
| 离屏大图 | `loading="lazy" decoding="async"` + `content-visibility: auto` | 双重懒加载 |
| LCP 候选图 | 上述 eager + `fetchpriority="high"` | 强制优先级 |

### 格式选择

```
AVIF > WebP > JPEG/PNG（按兼容性降级）
```

```html
<picture>
  <source srcset="cover.avif" type="image/avif" />
  <source srcset="cover.webp" type="image/webp" />
  <img src="cover.jpg" alt="..." loading="lazy" decoding="async" />
</picture>
```

### 响应式图片

```html
<img
  srcset="
    cover-400.webp 400w,
    cover-800.webp 800w,
    cover-1600.webp 1600w
  "
  sizes="(max-width: 640px) 100vw, (max-width: 1024px) 50vw, 800px"
  src="cover-800.webp"
  alt="..."
/>
```

### SVG vs 位图

| 场景 | 选择 |
|------|------|
| Logo / Icon / 矢量插画 | **SVG**（任意缩放、CSS 控制颜色） |
| 照片 / 复杂图像 | **AVIF / WebP** |
| 大量重复装饰 | **CSS 渐变**（比图片更便宜） |

---

## Long List Optimization

> 50+ 行的列表/虚拟滚动之外的标准方案。

### `content-visibility: auto`（最简方案）

让屏幕外元素**不参与渲染**，接近虚拟滚动效果但无需额外库。

```css
.chat-list {
  content-visibility: auto;
  contain-intrinsic-size: 0 80px;  /* 预估行高，避免滚动条抖动 */
}
```

**适用**：
- 聊天记录列表
- 设置项列表
- 文件浏览器
- 日志流

**不适配**：
- 必须全量渲染的打印场景
- SEO 关键的列表（虽然 `content-visibility: auto` 对 SEO 友好，但要测试）

### `contain` 精细化控制

```css
.card {
  contain: layout paint;  /* 独立布局/重绘上下文 */
}

.complex-widget {
  contain: layout paint style;  /* 加上 style 隔离 */
}
```

### 虚拟滚动（极端场景）

> 1000+ 行的列表用 `@vueuse/core` 的 `useVirtualList` 或 `vue-virtual-scroller`。

仅在 `content-visibility` 收益不足时引入，**避免过早优化**。

---

## Animation Performance

补充主 SKILL "Tight Transitions" 章节：

### GPU-Composited Properties Only

✅ 动画友好：`transform` / `opacity` / `filter`
❌ 触发重排/重绘：`width` / `height` / `top` / `left` / `margin` / `padding`

### 替代方案

| 反模式 | 替代 |
|--------|------|
| 动画 `height: 0 → 100px` | `transform: scaleY(0 → 1)` + `transform-origin: top` |
| 动画 `width: 0 → 100%` | `transform: scaleX(0 → 1)` |
| 动画 `top: 0 → 100px` | `transform: translateY(0 → 100px)` |
| 动画 `opacity: 0 → 1` | ✅ 仍然推荐 |
| 动画 `max-height: 0 → 1000px` | ⚠️ 仅在无法用 transform 替代时（如高度内容动态） |

### `will-change` 正确用法

```css
/* ❌ 静态元素加 will-change = 永远占用合成层 */
.button { will-change: transform; }

/* ✅ 动画期间才加，结束移除 */
.button:active { will-change: transform; }
```

Vue 实践：

```vue
<script setup lang="ts">
import { ref } from 'vue'

const isAnimating = ref(false)

const onClick = () => {
  isAnimating.value = true
  setTimeout(() => { isAnimating.value = false }, 300)
}
</script>

<template>
  <div
    :class="{ 'will-animate': isAnimating }"
    @click="onClick"
  />
</template>

<style scoped>
.will-animate { will-change: transform; }
</style>
```

---

## Critical Render Path

### CSS 体积控制

- Tailwind 启用 JIT（v3.4+ 默认）：只打包用到的工具类
- 定期跑 `npx tailwindcss --purge` 验证未使用类不进入产物
- 移动端考虑拆分 critical CSS（首屏）与 lazy CSS（按需）

### 阻塞资源

```html
<!-- CSS 是 render-blocking，必须放 head -->
<head>
  <link rel="stylesheet" href="/main.css" />
</head>

<!-- JS 用 defer / async，不阻塞解析 -->
<script type="module" src="/main.js" defer></script>
```

### Tailwind v3 + Vite 配置

```js
// vite.config.ts
export default defineConfig({
  build: {
    cssCodeSplit: true,  // 按路由分包 CSS
    rollupOptions: {
      output: {
        manualChunks: { /* 路由级 chunk */ }
      }
    }
  }
})
```

---

## Performance Checklist

新组件 / 新页面：

- [ ] 图片声明 `width` / `height` 或 `aspect-ratio`
- [ ] 折叠下方图片用 `loading="lazy"`
- [ ] 关键图片加 `fetchpriority="high"`
- [ ] 长列表（50+）用 `content-visibility: auto` + `contain-intrinsic-size`
- [ ] 动画仅用 `transform` / `opacity`
- [ ] `will-change` 仅在动画期间
- [ ] 自托管字体 `font-display: swap` + `size-adjust`
- [ ] 骨架屏与真实内容同高度
- [ ] 关键 CSS 不被异步阻塞
