# Vue 3 Styling Capabilities

Vue 3 `<script setup>` 提供的原生样式能力——`v-bind()`、`useCssVars()`、scoped style 机制，**主 SKILL 未涉及**但能显著减少样板代码。

> **何时加载**：需要把脚本状态传到 CSS、做主题切换、构建高复用组件时。

---

## v-bind() in `<style>`

在 `<script setup>` 中定义的变量可在 `<style scoped>` 中通过 `v-bind()` 直接访问，**无需 props drilling**。

### 基础用法

```vue
<script setup lang="ts">
import { ref } from 'vue'

const isActive = ref(false)
const size = ref<'sm' | 'md' | 'lg'>('md')
</script>

<template>
  <div class="status-dot" :class="{ active: isActive }" />
</template>

<style scoped>
.status-dot {
  /* 直接绑定脚本中的 ref */
  --dot-color: v-bind('isActive ? "var(--color-success)" : "var(--text-tertiary)"');
  --dot-size: v-bind('size === "sm" ? "8px" : size === "md" ? "12px" : "16px"');

  width: var(--dot-size);
  height: var(--dot-size);
  background: var(--dot-color);
  transition: background-color 0.2s;
}
</style>
```

### 编译产物

Vue 编译器自动将 `v-bind('expression')` 编译为：

```css
.status-dot {
  --dot-color: var(--some-hash-isActive);
  --dot-size: var(--some-hash-size);
}
```

并在元素上通过 `style` 属性同步值，**CSS 变量本身可在子组件继承**。

### 适用场景

| 场景 | 用法 |
|------|------|
| 状态驱动的颜色/尺寸切换 | `--prop-color: v-bind('isActive ? ... : ...')` |
| 父组件传 prop 给子组件 CSS | 子组件用 `var(--color)` 读取父级注入的变量 |
| 主题色实时切换 | 根组件用 `v-bind()` 注入整套 token |
| 复杂计算后的样式值 | 避免手写 `:style` 拼字符串 |

### 不适用场景

- ❌ **不需要响应式**：直接用 `class` 切换（如 `:class="{ active: isActive }"`）
- ❌ **频繁变化的高频值**：每次变化都会更新 DOM `style` 属性（鼠标坐标等）
- ❌ **需要 CSS 动画的中间值**：CSS 变量动画需要 `@property` 注册（见 `MODERN-CSS.md`）

---

## useCssVars()（批量注入）

需要把整个对象注入到子组件 CSS 时使用：

```vue
<script setup lang="ts">
import { useCssVars } from 'vue'
import { computed } from 'vue'

const props = defineProps<{
  theme: {
    bg: string
    text: string
    border: string
    radius: string
  }
}>()

// 批量注入，子组件可直接 var(--card-bg) 读取
useCssVars(() => ({
  'card-bg': computed(() => props.theme.bg),
  'card-text': computed(() => props.theme.text),
  'card-border': computed(() => props.theme.border),
  'card-radius': computed(() => props.theme.radius),
}))
</script>
```

**适用**：
- 主题系统（用户自定义主题色）
- 多租户/多品牌样式注入
- 插件系统（宿主给插件注入品牌变量）

---

## Fallthrough Attributes & `inheritAttrs`

默认情况下，父组件传的 `class` / `style` 会自动合并到根元素。**只有**根元素能收到，多根组件需要手动绑定：

```vue
<script setup lang="ts">
defineOptions({ inheritAttrs: false })
</script>

<template>
  <header>...</header>
  <main>...</main>
</template>
```

此时 `class` / `style` **不会**自动应用。手动用 `$attrs`：

```vue
<template>
  <header v-bind="$attrs">...</header>
  <main>...</main>
</template>
```

---

## Scoped Style 机制

### 工作原理

```vue
<template>
  <div class="title">Hello</div>
</template>

<style scoped>
.title { color: red; }
</style>
```

编译后：

```html
<div class="title" data-v-abc123>Hello</div>
```

```css
.title[data-v-abc123] { color: red; }
```

每个组件的 hash 唯一，**隔离样式避免污染**。

### 跨组件影响子组件样式

> ⚠️ 反模式——优先用 props/events/透传，避免 `:deep()`。

| 情况 | 做法 |
|------|------|
| 父组件想改子组件根元素 | 透传 `class` 到子组件根 |
| 父组件想改子组件内部元素 | ❌ 用 `:deep()`（**仅在第三方组件无解时使用**） |
| 父组件想完全控制子组件样式 | ❌ 拆出子组件内容到父组件 |

```vue
<!-- ❌ 反模式：用 :deep 穿透自家组件 -->
<style scoped>
.my-button :deep(.icon) { color: red; }
</style>

<!-- ✅ 正确：给 Icon 子组件传 prop -->
<MyButton :icon-color="isActive ? 'red' : 'gray'" />
```

### `:slotted()`（选择 slot 内容）

```vue
<!-- Parent -->
<Card>
  <template #header>
    <h2>Title</h2>
  </template>
</Card>

<!-- Card.vue -->
<style scoped>
.header:slotted(h2) {
  font-size: 1.5rem;
  margin: 0;
}
</style>
```

仅在必须从父组件作用域选择 slot 内容时使用。

---

## CSS Modules in Vue

适用于需要稳定类名、避免全局污染的库组件：

```vue
<script setup lang="ts">
import styles from './Card.module.css'
</script>

<template>
  <div :class="styles.card">
    <h3 :class="styles.title">{{ title }}</h3>
  </div>
</template>
```

**对比**：

| 方案 | 优势 | 劣势 |
|------|------|------|
| `<style scoped>` | 简单、Vue 默认 | 类名 hash 化，调试不直观 |
| CSS Modules | 类名稳定、跨组件复用友好 | 失去 scoped 自动隔离 |
| 全局 CSS | 适合 token、reset | 无隔离，需约定 |

**BedCode 推荐**：组件内部用 scoped；共享样式/库组件用 CSS Modules；全局 token / reset 用普通 CSS。

---

## Composable Style Patterns

### 主题切换 composable

```ts
// composables/useTheme.ts
import { ref, watch } from 'vue'

export type Theme = 'light' | 'dark'
const theme = ref<Theme>(
  (localStorage.getItem('theme') as Theme) ?? 'light'
)

watch(theme, (val) => {
  document.documentElement.classList.toggle('dark', val === 'dark')
  localStorage.setItem('theme', val)
}, { immediate: true })

export function useTheme() {
  return {
    theme,
    setTheme: (t: Theme) => { theme.value = t },
    toggle: () => { theme.value = theme.value === 'light' ? 'dark' : 'light' },
  }
}
```

```vue
<!-- App.vue -->
<script setup lang="ts">
import { useTheme } from '@/composables/useTheme'
useTheme()  // 启动副作用
</script>
```

### 响应式 CSS 变量

```ts
// composables/useResponsiveTokens.ts
import { ref, onMounted, onUnmounted } from 'vue'

export function useResponsiveTokens() {
  const containerWidth = ref(0)

  let observer: ResizeObserver | null = null

  onMounted(() => {
    const root = document.documentElement
    observer = new ResizeObserver(([entry]) => {
      containerWidth.value = entry.contentRect.width
    })
    observer.observe(root)
  })

  onUnmounted(() => observer?.disconnect())

  return { containerWidth }
}
```

```vue
<script setup lang="ts">
import { useResponsiveTokens } from '@/composables/useResponsiveTokens'
const { containerWidth } = useResponsiveTokens()
</script>

<style scoped>
.grid {
  --columns: v-bind('Math.max(1, Math.floor(containerWidth / 200))');
  grid-template-columns: repeat(var(--columns), 1fr);
}
</style>
```

---

## Checklist

- [ ] 状态驱动的颜色/尺寸用 `v-bind()` 注入 CSS 变量，**不**用 `:style` 字符串
- [ ] 主题切换用 `useCssVars()` 批量注入
- [ ] 不滥用 `:deep()` 穿透自家组件——优先 props/events
- [ ] 多根组件手动 `v-bind="$attrs"`
- [ ] 库组件考虑用 CSS Modules 获得稳定类名
