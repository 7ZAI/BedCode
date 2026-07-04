# 移动端弹窗动画统一 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 bedcode-mobile 所有弹窗/模态框添加统一的打开/关闭动画，消除"闪现"问题和不一致的动画参数。

**Architecture:** 在全局 `mobile.css` 中定义两套 Vue `<Transition>` CSS 规则（`center-modal` 和 `bottom-sheet`），所有弹窗组件统一引用，删除各组件的 scoped transition CSS 和 `@keyframes`。内容面板通过 `.modal-panel` class 被全局选择器匹配。

**Tech Stack:** Vue 3 `<Transition>`, CSS transitions (transform + opacity), TailwindCSS

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `bedcode-mobile/src/styles/mobile.css` | Modify | 添加 `center-modal` 和 `bottom-sheet` 全局 transition CSS |
| `bedcode-mobile/src/components/Modal.vue` | Modify | 替换 transition name，删除 scoped transition CSS，加 `.modal-panel` |
| `bedcode-mobile/src/components/ConfirmDialog.vue` | Modify | 替换 transition name，删除 scoped transition CSS，加 `.modal-panel` |
| `bedcode-mobile/src/components/BottomSheet.vue` | Modify | 替换 transition name，删除 scoped transition CSS，加 `.modal-panel` |
| `bedcode-mobile/src/components/TaskEditDialog.vue` | Modify | 替换 transition name，删除 scoped transition CSS，加 `.modal-panel` |
| `bedcode-mobile/src/components/FileViewerModal.vue` | Modify | 替换 transition name，删除 scoped transition CSS，加 `.modal-panel` |
| `bedcode-mobile/src/components/TaskPickerModal.vue` | Modify | 加 `<Transition name="bottom-sheet">` + `v-if`，删除 `@keyframes modal-in`，加 `.modal-panel` |
| `bedcode-mobile/src/components/ShortcutConfigModal.vue` | Modify | 加 `<Transition name="bottom-sheet">` + `v-if`，删除 `@keyframes slide-up`，加 `.modal-panel` |
| `bedcode-mobile/src/components/ShortcutHelpModal.vue` | Modify | 加 `<Transition name="bottom-sheet">` + `v-if`，删除 `@keyframes slide-up`，加 `.modal-panel` |
| `bedcode-mobile/src/components/TerminalConfirmModal.vue` | Modify | 加 `<Teleport>` + `<Transition name="center-modal">` + `v-if`，加 `.modal-panel` |
| `bedcode-mobile/src/components/TerminalSettingsModal.vue` | Modify | 加 `<Teleport>` + `<Transition name="center-modal">` + `v-if`，加 `.modal-panel` |
| `bedcode-mobile/src/components/CodeViewerSettingsModal.vue` | Modify | 加 `<Transition name="center-modal">` + `v-if`，加 `.modal-panel` |
| `bedcode-mobile/src/components/SettingsModal.vue` | Modify | 加 `<Transition name="center-modal">` + `v-if`，加 `.modal-panel` |
| `bedcode-mobile/src/views/ToolboxView.vue` | Modify | session picker 改用 `bottom-sheet` transition，confirm dialog 改用 `ConfirmDialog` 组件 |
| `bedcode-mobile/src/views/SettingsView.vue` | Modify | 内联弹窗改用 `ConfirmDialog` 组件 |

---

## Animation Spec

| 参数 | 值 |
|------|-----|
| 时长 | 280ms |
| 曲线 | `cubic-bezier(0.32, 0.72, 0, 1)` |
| backdrop 曲线 | `ease` |
| backdrop 颜色 | 不变，沿用 `--mobile-overlay` / `--mobile-overlay-heavy` / `--mobile-overlay-light` |

### `center-modal` — 中央缩放弹出

- **打开：** Backdrop `opacity 0→1` (280ms ease) + Content `opacity 0→1` + `scale(0.92)→scale(1)` (280ms cubic-bezier)
- **关闭：** 反向

### `bottom-sheet` — 底部滑入

- **打开：** Backdrop `opacity 0→1` (280ms ease) + Content `translateY(100%)→translateY(0)` (280ms cubic-bezier)
- **关闭：** 反向

---

## Task 1: 全局 Transition CSS

**Files:**
- Modify: `bedcode-mobile/src/styles/mobile.css`

在 `mobile.css` 末尾添加全局 transition 定义。这些 CSS 不使用 scoped，放在全局文件中供所有组件引用。

- [ ] **Step 1: 在 `mobile.css` 末尾追加 transition CSS**

在文件最后（`mobile-loading-fade-leave-to` 块之后）追加：

```css
/* ==================== Modal Transitions ==================== */

/* Center Modal: scale + fade */
.center-modal-enter-active,
.center-modal-leave-active {
  transition: opacity 280ms ease;
}
.center-modal-enter-active .modal-panel,
.center-modal-leave-active .modal-panel {
  transition: transform 280ms cubic-bezier(0.32, 0.72, 0, 1),
              opacity 280ms ease;
}
.center-modal-enter-from,
.center-modal-leave-to {
  opacity: 0;
}
.center-modal-enter-from .modal-panel,
.center-modal-leave-to .modal-panel {
  transform: scale(0.92);
  opacity: 0;
}

/* Bottom Sheet: slide up */
.bottom-sheet-enter-active,
.bottom-sheet-leave-active {
  transition: opacity 280ms ease;
}
.bottom-sheet-enter-active .modal-panel,
.bottom-sheet-leave-active .modal-panel {
  transition: transform 280ms cubic-bezier(0.32, 0.72, 0, 1);
}
.bottom-sheet-enter-from,
.bottom-sheet-leave-to {
  opacity: 0;
}
.bottom-sheet-enter-from .modal-panel,
.bottom-sheet-leave-to .modal-panel {
  transform: translateY(100%);
}
```

- [ ] **Step 2: 验证 CSS 无语法错误**

Run: `cd bedcode-mobile && npx vue-tsc --noEmit 2>&1 | head -5`
Expected: 无 CSS 相关报错（CSS 语法不在 TS 检查范围内，确认无构建错误即可）

- [ ] **Step 3: Commit**

```bash
git add bedcode-mobile/src/styles/mobile.css
git commit -m "feat(mobile): add global center-modal and bottom-sheet transition CSS"
```

---

## Task 2: Modal.vue — 统一 center-modal

**Files:**
- Modify: `bedcode-mobile/src/components/Modal.vue`

当前状态：`<Transition name="modal">`，scoped CSS（0.2s ease, scale 0.95），无 `.modal-panel` class。

- [ ] **Step 1: 替换 Transition name**

将模板中的 `<Transition name="modal">` 改为 `<Transition name="center-modal">`。

- [ ] **Step 2: 给内容面板加 `.modal-panel` class**

当前内容面板 div（第 14 行附近）：
```html
<div class="relative rounded-xl shadow-2xl border bg-[var(--mobile-bg-card)] border-[var(--mobile-border)]">
```

改为：
```html
<div class="relative rounded-xl shadow-2xl border bg-[var(--mobile-bg-card)] border-[var(--mobile-border)] modal-panel">
```

- [ ] **Step 3: 删除 scoped transition CSS**

删除 `<style scoped>` 中的以下代码块（第 114-129 行）：
```css
.modal-enter-active,
.modal-leave-active {
  transition: all 0.2s ease;
}

.modal-enter-from,
.modal-leave-to {
  opacity: 0;
}

.modal-enter-from > div:last-child,
.modal-leave-to > div:last-child {
  transform: scale(0.95);
}
```

如果 `<style scoped>` 内没有其他样式规则，则整个 `<style scoped></style>` 块一并删除。

- [ ] **Step 4: Commit**

```bash
git add bedcode-mobile/src/components/Modal.vue
git commit -m "feat(mobile): Modal.vue use global center-modal transition"
```

---

## Task 3: ConfirmDialog.vue — 统一 center-modal

**Files:**
- Modify: `bedcode-mobile/src/components/ConfirmDialog.vue`

当前状态：`<Transition name="confirm">`，scoped CSS（opacity 0.2s, translateY 20px + opacity 0.2s），内容面板用 `.relative` class 选择器。

- [ ] **Step 1: 替换 Transition name**

将 `<Transition name="confirm">` 改为 `<Transition name="center-modal">`。

- [ ] **Step 2: 给内容面板加 `.modal-panel` class**

当前内容面板 div（第 15 行附近）：
```html
<div class="relative w-full max-w-sm mx-4 mb-[var(--safe-area-bottom,0px)] bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl overflow-hidden shadow-xl">
```

改为：
```html
<div class="relative w-full max-w-sm mx-4 mb-[var(--safe-area-bottom,0px)] bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl overflow-hidden shadow-xl modal-panel">
```

- [ ] **Step 3: 删除 scoped transition CSS**

删除 `<style scoped>` 中的以下代码块（第 132-153 行）：
```css
.confirm-enter-active,
.confirm-leave-active {
  transition: opacity 0.2s ease;
}

.confirm-enter-active .relative,
.confirm-leave-active .relative {
  transition: transform 0.3s cubic-bezier(0.4, 0, 0.2, 1), opacity 0.2s ease;
}

.confirm-enter-from,
.confirm-leave-to {
  opacity: 0;
}

.confirm-enter-from .relative,
.confirm-leave-to .relative {
  transform: translateY(20px);
  opacity: 0;
}
```

如果 `<style scoped>` 内没有其他样式规则，则整个 `<style scoped></style>` 块一并删除。

- [ ] **Step 4: Commit**

```bash
git add bedcode-mobile/src/components/ConfirmDialog.vue
git commit -m "feat(mobile): ConfirmDialog.vue use global center-modal transition"
```

---

## Task 4: BottomSheet.vue — 统一 center-modal

**Files:**
- Modify: `bedcode-mobile/src/components/BottomSheet.vue`

当前状态：`<Transition name="fade">`，scoped CSS（opacity 0.2s, scale 0.95），内容面板用 `.relative` class 选择器。组件名虽为 BottomSheet 但实际是居中输入弹窗，使用 center-modal。

- [ ] **Step 1: 替换 Transition name**

将 `<Transition name="fade">` 改为 `<Transition name="center-modal">`。

- [ ] **Step 2: 给内容面板加 `.modal-panel` class**

当前内容面板 div（第 9 行附近）：
```html
<div class="relative w-full max-w-sm bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl p-6 shadow-xl">
```

改为：
```html
<div class="relative w-full max-w-sm bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl p-6 shadow-xl modal-panel">
```

- [ ] **Step 3: 删除 scoped transition CSS**

删除 `<style scoped>` 中的以下代码块（第 130-151 行）：
```css
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-active .relative,
.fade-leave-active .relative {
  transition: transform 0.2s ease, opacity 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

.fade-enter-from .relative,
.fade-leave-to .relative {
  transform: scale(0.95);
  opacity: 0;
}
```

如果 `<style scoped>` 内没有其他样式规则，则整个 `<style scoped></style>` 块一并删除。

- [ ] **Step 4: Commit**

```bash
git add bedcode-mobile/src/components/BottomSheet.vue
git commit -m "feat(mobile): BottomSheet.vue use global center-modal transition"
```

---

## Task 5: TaskEditDialog.vue — 统一 center-modal

**Files:**
- Modify: `bedcode-mobile/src/components/TaskEditDialog.vue`

当前状态：`<Transition name="modal">`，scoped CSS（all 0.2s ease, scale 0.95），用 `> :last-child` 选择器。有两处 Transition（主弹窗 z-110 和文件浏览器 z-120），都需要改。

- [ ] **Step 1: 替换两处 Transition name**

将第一个 `<Transition name="modal">`（第 3 行附近）改为 `<Transition name="center-modal">`。

将第二个 `<Transition name="modal">`（第 116 行附近）也改为 `<Transition name="center-modal">`。

- [ ] **Step 2: 给两个内容面板加 `.modal-panel` class**

主弹窗内容面板（第 6 行附近）：
```html
<div class="relative w-full max-w-lg bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl p-6 shadow-xl max-h-[85vh] flex flex-col">
```

改为：
```html
<div class="relative w-full max-w-lg bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl p-6 shadow-xl max-h-[85vh] flex flex-col modal-panel">
```

文件浏览器内容面板（第 119 行附近）：
```html
<div class="relative w-full h-full bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl shadow-xl overflow-hidden flex flex-col">
```

改为：
```html
<div class="relative w-full h-full bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl shadow-xl overflow-hidden flex flex-col modal-panel">
```

- [ ] **Step 3: 删除 scoped modal transition CSS**

删除以下代码块（第 263-278 行）：
```css
/* Modal transition - scale + fade */
.modal-enter-active,
.modal-leave-active {
  transition: all 0.2s ease;
}

.modal-enter-from,
.modal-leave-to {
  opacity: 0;
}

.modal-enter-from > :last-child,
.modal-leave-to > :last-child {
  transform: scale(0.95);
}
```

保留 `.dropdown-*` transition CSS（用于目录下拉菜单，不在此次改动范围）。

- [ ] **Step 4: Commit**

```bash
git add bedcode-mobile/src/components/TaskEditDialog.vue
git commit -m "feat(mobile): TaskEditDialog.vue use global center-modal transition"
```

---

## Task 6: FileViewerModal.vue — 统一 center-modal

**Files:**
- Modify: `bedcode-mobile/src/components/FileViewerModal.vue`

当前状态：`<transition name="modal-fade">`，scoped CSS（opacity 0.2s, scale 0.95），内容面板用 `.viewer-modal` class 选择器。

- [ ] **Step 1: 替换 Transition name**

将 `<transition name="modal-fade">`（第 3 行）改为 `<transition name="center-modal">`。

- [ ] **Step 2: 给内容面板加 `.modal-panel` class**

`.viewer-modal` div（第 5 行附近）：
```html
<div class="viewer-modal" :class="{ 'viewer-fullscreen': isFullscreen }" :style="modalStyle">
```

改为：
```html
<div class="viewer-modal modal-panel" :class="{ 'viewer-fullscreen': isFullscreen }" :style="modalStyle">
```

- [ ] **Step 3: 删除 scoped modal-fade transition CSS**

删除 `<style scoped>` 末尾的以下代码块（第 686-705 行）：
```css
/* Modal transition */
.modal-fade-enter-active,
.modal-fade-leave-active {
  transition: opacity 0.2s ease;
}

.modal-fade-enter-from,
.modal-fade-leave-to {
  opacity: 0;
}

.modal-fade-enter-active .viewer-modal,
.modal-fade-leave-active .viewer-modal {
  transition: transform 0.2s ease;
}

.modal-fade-enter-from .viewer-modal,
.modal-fade-leave-to .viewer-modal {
  transform: scale(0.95);
}
```

- [ ] **Step 4: Commit**

```bash
git add bedcode-mobile/src/components/FileViewerModal.vue
git commit -m "feat(mobile): FileViewerModal.vue use global center-modal transition"
```

---

## Task 7: TaskPickerModal.vue — 统一 bottom-sheet

**Files:**
- Modify: `bedcode-mobile/src/components/TaskPickerModal.vue`

当前状态：无 `<Transition>`，使用 `@keyframes modal-in`（0.2s, scale 0.95→1），仅入场动画。需改为 `<Transition name="bottom-sheet">` + `v-if`，删除 `@keyframes`。

- [ ] **Step 1: 添加 `<Transition>` 包裹和 `v-if`**

当前模板结构（第 110-172 行）：
```html
<Teleport to="body">
  <div class="modal-overlay mobile-ui" @click.self="emit('close')">
    <div class="modal-content">
      ...
    </div>
  </div>
  ...
</Teleport>
```

改为：
```html
<Teleport to="body">
  <Transition name="bottom-sheet">
    <div v-if="tasks.length > 0 || true" class="modal-overlay mobile-ui" @click.self="emit('close')">
      ...
    </div>
  </Transition>
  ...
</Teleport>
```

**注意：** TaskPickerModal 没有自己的 `visible` prop，它是通过父组件 v-if 控制整个组件的挂载/卸载。所以这里不能用 v-if 控制动画——需要调整思路。

实际做法：**在父组件中使用 `<Transition>` 包裹 `<TaskPickerModal>`。但更简单的方式是给 TaskPickerModal 加一个 `visible` prop 并在组件内部控制。**

但观察当前用法：TaskPickerModal 由父组件通过 v-if 控制显示，没有 visible prop。为了保持改动最小化，改用另一种方式：**保留 `@keyframes` 入场动画用于入场，添加 `<Transition>` 包裹用于退场动画。**

**更好的方案：** TaskPickerModal 不加 visible prop，直接在组件外层包裹 Transition。但由于父组件用 v-if 控制整个组件，Transition 无法生效（组件卸载后 Transition 没有机会播放 leave 动画）。

**最终方案：** 为 TaskPickerModal 添加 `visible` prop，内部用 `<Transition>` + `v-if` 控制。父组件改用 `:visible` + 事件控制，而非 v-if 直接卸载。

先改组件内部：

将模板改为：
```html
<Teleport to="body">
  <Transition name="bottom-sheet">
    <div v-if="visible" class="modal-overlay mobile-ui" @click.self="emit('close')">
      <div class="modal-content modal-panel">
        ...
      </div>
    </div>
  </Transition>
  ...
</Teleport>
```

在 script 中添加 `visible` prop：
```typescript
const props = defineProps<{
  tasks: PresetTask[]
  visible?: boolean
  sessionId?: string
}>()
```

- [ ] **Step 2: 给 `.modal-content` 加 `.modal-panel` class**

将：
```html
<div class="modal-content">
```

改为：
```html
<div class="modal-content modal-panel">
```

- [ ] **Step 3: 删除 `@keyframes modal-in` 和引用**

删除 scoped CSS 中的（第 213-222 行）：
```css
@keyframes modal-in {
  from {
    opacity: 0;
    transform: scale(0.95);
  }
  to {
    opacity: 1;
    transform: scale(1);
  }
}
```

同时删除 `.modal-content` 中的 `animation: modal-in 0.2s ease;`（第 210 行）。

- [ ] **Step 4: 更新父组件调用方式**

查找所有使用 TaskPickerModal 的父组件，将 `v-if` 改为 `:visible` prop 控制。

在 `ToolboxView.vue` 中（需搜索确认具体位置），将：
```html
<TaskPickerModal v-if="..." :tasks="..." @close="..." @send="..." @execute="..." />
```

改为：
```html
<TaskPickerModal :visible="..." :tasks="..." @close="..." @send="..." @execute="..." />
```

在 `TerminalView.vue` 中（需搜索确认具体位置），做相同改动。

- [ ] **Step 5: Commit**

```bash
git add bedcode-mobile/src/components/TaskPickerModal.vue bedcode-mobile/src/views/ToolboxView.vue bedcode-mobile/src/views/TerminalView.vue
git commit -m "feat(mobile): TaskPickerModal.vue use global bottom-sheet transition"
```

---

## Task 8: ShortcutConfigModal.vue — 统一 bottom-sheet

**Files:**
- Modify: `bedcode-mobile/src/components/ShortcutConfigModal.vue`

当前状态：无 `<Transition>`，使用 `@keyframes slide-up`（0.25s, translateY 100%→0），仅入场动画。有 `visible` prop 但未配合 Transition。

- [ ] **Step 1: 添加 `<Transition>` 包裹，改 `v-if` 为 Transition 控制**

当前模板结构（第 2-229 行）：
```html
<Teleport to="body">
  <div
    v-if="visible"
    class="fixed inset-0 z-[100] flex items-end justify-center mobile-ui"
    @click.self="emit('close')"
  >
    <div class="absolute inset-0 bg-[var(--mobile-overlay-light)]" @click="emit('close')"></div>
    <div class="shortcut-config-modal relative bg-[var(--mobile-bg-card)] ...">
```

改为：
```html
<Teleport to="body">
  <Transition name="bottom-sheet">
    <div
      v-if="visible"
      class="fixed inset-0 z-[100] flex items-end justify-center mobile-ui"
      @click.self="emit('close')"
    >
      <div class="absolute inset-0 bg-[var(--mobile-overlay-light)]" @click="emit('close')"></div>
      <div class="shortcut-config-modal relative bg-[var(--mobile-bg-card)] ... modal-panel">
```

在 `</div>` 结束标签后（即删除确认弹窗之前）关闭 `</Transition>`：

```html
    </div>
  </Transition>

  <!-- 删除确认弹窗 -->
  ...
```

注意：删除确认弹窗（`confirmDeleteCode` 控制的 `delete-confirm-overlay`）和 `ShortcutHelpModal` 在 Transition 外部，不要包进去。

- [ ] **Step 2: 给内容面板加 `.modal-panel` class**

`.shortcut-config-modal` div（第 9 行附近）：
```html
<div class="shortcut-config-modal relative bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-t-2xl w-full max-w-lg max-h-[85vh] flex flex-col shadow-xl">
```

改为：
```html
<div class="shortcut-config-modal relative bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-t-2xl w-full max-w-lg max-h-[85vh] flex flex-col shadow-xl modal-panel">
```

- [ ] **Step 3: 删除 `@keyframes slide-up` 和引用**

删除 scoped CSS 中的（第 450-463 行）：
```css
.shortcut-config-modal {
  animation: slide-up 0.25s cubic-bezier(0.4, 0, 0.2, 1);
}

@keyframes slide-up {
  from {
    transform: translateY(100%);
    opacity: 0;
  }
  to {
    transform: translateY(0);
    opacity: 1;
  }
}
```

仅保留 `.shortcut-config-modal { ... }` 中除 `animation` 以外的样式（当前该选择器只有 animation，所以整块删除）。

- [ ] **Step 4: Commit**

```bash
git add bedcode-mobile/src/components/ShortcutConfigModal.vue
git commit -m "feat(mobile): ShortcutConfigModal.vue use global bottom-sheet transition"
```

---

## Task 9: ShortcutHelpModal.vue — 统一 bottom-sheet

**Files:**
- Modify: `bedcode-mobile/src/components/ShortcutHelpModal.vue`

当前状态：无 `<Transition>`，使用 `@keyframes slide-up`（0.25s, translateY 100%→0），仅入场动画。有 `visible` prop 但未配合 Transition。

- [ ] **Step 1: 添加 `<Transition>` 包裹**

当前模板结构（第 2-29 行）：
```html
<Teleport to="body">
  <div
    v-if="visible"
    class="fixed inset-0 z-[120] flex items-end justify-center mobile-ui"
    @click.self="emit('close')"
  >
    <div class="absolute inset-0 bg-[var(--mobile-overlay-light)]" @click="emit('close')"></div>
    <div class="shortcut-help-modal relative bg-[var(--mobile-bg-card)] ...">
```

改为：
```html
<Teleport to="body">
  <Transition name="bottom-sheet">
    <div
      v-if="visible"
      class="fixed inset-0 z-[120] flex items-end justify-center mobile-ui"
      @click.self="emit('close')"
    >
      <div class="absolute inset-0 bg-[var(--mobile-overlay-light)]" @click="emit('close')"></div>
      <div class="shortcut-help-modal relative bg-[var(--mobile-bg-card)] ... modal-panel">
```

在 `</div>` 结束标签后关闭 `</Transition>`：
```html
    </div>
  </Transition>
</Teleport>
```

- [ ] **Step 2: 给内容面板加 `.modal-panel` class**

`.shortcut-help-modal` div（第 9 行附近）：
```html
<div class="shortcut-help-modal relative bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-t-2xl w-full max-w-lg max-h-[85vh] flex flex-col shadow-xl">
```

改为：
```html
<div class="shortcut-help-modal relative bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-t-2xl w-full max-w-lg max-h-[85vh] flex flex-col shadow-xl modal-panel">
```

- [ ] **Step 3: 删除 `@keyframes slide-up` 和引用**

删除 scoped CSS 中的（第 59-73 行）：
```css
.shortcut-help-modal {
  animation: slide-up 0.25s cubic-bezier(0.4, 0, 0.2, 1);
}

@keyframes slide-up {
  from {
    transform: translateY(100%);
    opacity: 0;
  }
  to {
    transform: translateY(0);
    opacity: 1;
  }
}
```

同样，`.shortcut-help-modal { ... }` 当前只有 animation 属性，整块删除。

- [ ] **Step 4: Commit**

```bash
git add bedcode-mobile/src/components/ShortcutHelpModal.vue
git commit -m "feat(mobile): ShortcutHelpModal.vue use global bottom-sheet transition"
```

---

## Task 10: TerminalConfirmModal.vue — 添加动画

**Files:**
- Modify: `bedcode-mobile/src/components/TerminalConfirmModal.vue`

当前状态：无 `<Teleport>`、无 `<Transition>`、无动画。直接用 `v-if="visible"` 控制显示。

- [ ] **Step 1: 添加 `<Teleport>` + `<Transition>` 包裹**

当前模板（第 2-10 行）：
```html
<div v-if="visible" class="confirm-modal-overlay mobile-ui" @click.self="$emit('cancel')">
  <div class="confirm-modal" :style="safeAreaStyle">
    ...
  </div>
</div>
```

改为：
```html
<Teleport to="body">
  <Transition name="center-modal">
    <div v-if="visible" class="confirm-modal-overlay mobile-ui" @click.self="$emit('cancel')">
      <div class="confirm-modal modal-panel" :style="safeAreaStyle">
        ...
      </div>
    </div>
  </Transition>
</Teleport>
```

- [ ] **Step 2: 给内容面板加 `.modal-panel` class**

`.confirm-modal` div：
```html
<div class="confirm-modal" :style="safeAreaStyle">
```

改为：
```html
<div class="confirm-modal modal-panel" :style="safeAreaStyle">
```

- [ ] **Step 3: Commit**

```bash
git add bedcode-mobile/src/components/TerminalConfirmModal.vue
git commit -m "feat(mobile): TerminalConfirmModal.vue add center-modal transition"
```

---

## Task 11: TerminalSettingsModal.vue — 添加动画

**Files:**
- Modify: `bedcode-mobile/src/components/TerminalSettingsModal.vue`

当前状态：无 `<Teleport>`、无 `<Transition>`、无动画。直接用 `v-if="visible"` 控制显示。

- [ ] **Step 1: 添加 `<Teleport>` + `<Transition>` 包裹**

当前模板（第 2-75 行）：
```html
<div v-if="visible" class="settings-modal-overlay mobile-ui" @click.self="$emit('cancel')">
  <div class="settings-modal" :style="safeAreaStyle">
    ...
  </div>
</div>
```

改为：
```html
<Teleport to="body">
  <Transition name="center-modal">
    <div v-if="visible" class="settings-modal-overlay mobile-ui" @click.self="$emit('cancel')">
      <div class="settings-modal modal-panel" :style="safeAreaStyle">
        ...
      </div>
    </div>
  </Transition>
</Teleport>
```

- [ ] **Step 2: 给内容面板加 `.modal-panel` class**

`.settings-modal` div：
```html
<div class="settings-modal" :style="safeAreaStyle">
```

改为：
```html
<div class="settings-modal modal-panel" :style="safeAreaStyle">
```

- [ ] **Step 3: Commit**

```bash
git add bedcode-mobile/src/components/TerminalSettingsModal.vue
git commit -m "feat(mobile): TerminalSettingsModal.vue add center-modal transition"
```

---

## Task 12: CodeViewerSettingsModal.vue — 添加动画

**Files:**
- Modify: `bedcode-mobile/src/components/CodeViewerSettingsModal.vue`

当前状态：已有 `<Teleport>`，无 `<Transition>`，无动画。直接用 `v-if="visible"` 控制。

- [ ] **Step 1: 添加 `<Transition>` 包裹**

当前模板结构（第 2-102 行）：
```html
<Teleport to="body">
  <div
    v-if="visible"
    class="settings-modal-overlay mobile-ui"
    @click.self="emit('close')"
  >
    <div class="settings-modal" :style="modalStyle">
```

改为：
```html
<Teleport to="body">
  <Transition name="center-modal">
    <div
      v-if="visible"
      class="settings-modal-overlay mobile-ui"
      @click.self="emit('close')"
    >
      <div class="settings-modal modal-panel" :style="modalStyle">
```

在 `</div></Teleport>` 之前关闭 `</Transition>`：
```html
    </div>
  </Transition>
</Teleport>
```

- [ ] **Step 2: 给内容面板加 `.modal-panel` class**

`.settings-modal` div：
```html
<div class="settings-modal" :style="modalStyle">
```

改为：
```html
<div class="settings-modal modal-panel" :style="modalStyle">
```

- [ ] **Step 3: Commit**

```bash
git add bedcode-mobile/src/components/CodeViewerSettingsModal.vue
git commit -m "feat(mobile): CodeViewerSettingsModal.vue add center-modal transition"
```

---

## Task 13: SettingsModal.vue — 添加动画

**Files:**
- Modify: `bedcode-mobile/src/components/SettingsModal.vue`

当前状态：已有 `<Teleport>`，无 `<Transition>`，无动画。直接用 `v-if="visible"` 控制。

- [ ] **Step 1: 添加 `<Transition>` 包裹**

当前模板结构（第 2-136 行）：
```html
<Teleport to="body">
  <div
    v-if="visible"
    class="fixed inset-0 z-[100] flex items-center justify-center p-4 mobile-ui"
    @click.self="emit('close')"
  >
    <div class="absolute inset-0 bg-[var(--mobile-overlay-light)]" @click="emit('close')"></div>
    <div class="relative bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl w-full max-w-sm p-5 shadow-xl">
```

改为：
```html
<Teleport to="body">
  <Transition name="center-modal">
    <div
      v-if="visible"
      class="fixed inset-0 z-[100] flex items-center justify-center p-4 mobile-ui"
      @click.self="emit('close')"
    >
      <div class="absolute inset-0 bg-[var(--mobile-overlay-light)]" @click="emit('close')"></div>
      <div class="relative bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl w-full max-w-sm p-5 shadow-xl modal-panel">
```

在 `</div></Teleport>` 之前关闭 `</Transition>`：
```html
    </div>
  </Transition>
</Teleport>
```

- [ ] **Step 2: 给内容面板加 `.modal-panel` class**

内容面板 div：
```html
<div class="relative bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl w-full max-w-sm p-5 shadow-xl">
```

改为：
```html
<div class="relative bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl w-full max-w-sm p-5 shadow-xl modal-panel">
```

- [ ] **Step 3: Commit**

```bash
git add bedcode-mobile/src/components/SettingsModal.vue
git commit -m "feat(mobile): SettingsModal.vue add center-modal transition"
```

---

## Task 14: ToolboxView 内联弹窗 — 统一动画

**Files:**
- Modify: `bedcode-mobile/src/views/ToolboxView.vue`

当前状态：session picker 使用 `<Transition name="fade">`，confirm dialog 也使用 `<Transition name="fade">`。两者都有 Teleport 和 scoped fade transition，但没有 scoped CSS（使用内联样式）。

需要搜索 ToolboxView.vue 中是否有对应的 scoped `<style>` 中的 `.fade-*` 规则。根据当前代码，ToolboxView.vue 没有 `<style scoped>` 块（样式由 TailwindCSS 内联处理），所以 fade transition 实际上没有 CSS 规则驱动——**意味着当前这两个弹窗也没有完整动画**。

- [ ] **Step 1: session picker 改用 `bottom-sheet` transition + `.modal-panel`**

当前（第 88-118 行）：
```html
<Teleport to="body">
  <Transition name="fade">
    <div v-if="showSessionPicker" class="fixed inset-0 z-50 flex items-center justify-center p-4 mobile-ui">
      <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]" @click="showSessionPicker = false"></div>
      <div class="relative w-full max-w-sm bg-[var(--mobile-bg-card)] ...">
```

改为：
```html
<Teleport to="body">
  <Transition name="bottom-sheet">
    <div v-if="showSessionPicker" class="fixed inset-0 z-50 flex items-center justify-center p-4 mobile-ui">
      <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]" @click="showSessionPicker = false"></div>
      <div class="relative w-full max-w-sm bg-[var(--mobile-bg-card)] ... modal-panel">
```

- [ ] **Step 2: confirm dialog 改用 `center-modal` transition + `.modal-panel`**

当前（第 122-153 行附近）：
```html
<Teleport to="body">
  <Transition name="fade">
    <div v-if="showConfirmDialog" class="fixed inset-0 z-50 flex items-center justify-center p-4 mobile-ui">
      <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]" @click="showConfirmDialog = false"></div>
      <div class="relative w-full max-w-sm bg-[var(--mobile-bg-card)] ...">
```

改为：
```html
<Teleport to="body">
  <Transition name="center-modal">
    <div v-if="showConfirmDialog" class="fixed inset-0 z-50 flex items-center justify-center p-4 mobile-ui">
      <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]" @click="showConfirmDialog = false"></div>
      <div class="relative w-full max-w-sm bg-[var(--mobile-bg-card)] ... modal-panel">
```

- [ ] **Step 3: Commit**

```bash
git add bedcode-mobile/src/views/ToolboxView.vue
git commit -m "feat(mobile): ToolboxView inline dialogs use global transitions"
```

---

## Task 15: SettingsView 内联弹窗 — 统一动画

**Files:**
- Modify: `bedcode-mobile/src/views/SettingsView.vue`

当前状态：两个内联确认弹窗（`showBrowserConfirm` 和 `showConfirm`），无 `<Transition>`、无动画。使用 Teleport + v-if 直接控制。

- [ ] **Step 1: 浏览器确认弹窗加 `<Transition name="center-modal">` + `.modal-panel`**

当前（第 201-211 行）：
```html
<Teleport to="body">
  <div v-if="showBrowserConfirm" class="confirm-modal-overlay mobile-ui" @click.self="cancelOpenBrowser">
    <div class="confirm-modal">
```

改为：
```html
<Teleport to="body">
  <Transition name="center-modal">
    <div v-if="showBrowserConfirm" class="confirm-modal-overlay mobile-ui" @click.self="cancelOpenBrowser">
      <div class="confirm-modal modal-panel">
```

在 `</div></Teleport>` 之前关闭 `</Transition>`：
```html
    </div>
  </Transition>
</Teleport>
```

- [ ] **Step 2: 通用确认弹窗加 `<Transition name="center-modal">` + `.modal-panel`**

当前（第 215-225 行）：
```html
<Teleport to="body">
  <div v-if="showConfirm" class="confirm-modal-overlay mobile-ui" @click.self="cancelConfirm">
    <div class="confirm-modal">
```

改为：
```html
<Teleport to="body">
  <Transition name="center-modal">
    <div v-if="showConfirm" class="confirm-modal-overlay mobile-ui" @click.self="cancelConfirm">
      <div class="confirm-modal modal-panel">
```

在 `</div></Teleport>` 之前关闭 `</Transition>`：
```html
    </div>
  </Transition>
</Teleport>
```

- [ ] **Step 3: Commit**

```bash
git add bedcode-mobile/src/views/SettingsView.vue
git commit -m "feat(mobile): SettingsView inline dialogs add center-modal transition"
```

---

## Task 16: ShortcutConfigModal 删除确认弹窗 — 添加动画

**Files:**
- Modify: `bedcode-mobile/src/components/ShortcutConfigModal.vue`

ShortcutConfigModal 内部有一个删除确认弹窗（`confirmDeleteCode` 控制），当前用 `v-if` 直接显示，无 Transition、无动画。

- [ ] **Step 1: 给删除确认弹窗加 `<Transition name="center-modal">` + `.modal-panel`**

当前（第 216-224 行）：
```html
<div v-if="confirmDeleteCode" class="delete-confirm-overlay" @click.self="confirmDeleteCode = ''">
  <div class="delete-confirm-modal">
```

改为：
```html
<Transition name="center-modal">
  <div v-if="confirmDeleteCode" class="delete-confirm-overlay" @click.self="confirmDeleteCode = ''">
    <div class="delete-confirm-modal modal-panel">
```

在 `</div>` 后关闭 `</Transition>`：
```html
  </div>
</Transition>
```

- [ ] **Step 2: Commit**

```bash
git add bedcode-mobile/src/components/ShortcutConfigModal.vue
git commit -m "feat(mobile): ShortcutConfigModal delete confirm add center-modal transition"
```

---

## Task 17: 集成验证

**Files:**
- All modified files

- [ ] **Step 1: 确认构建无错误**

Run: `cd bedcode-mobile && npx vue-tsc --noEmit 2>&1 | tail -20`
Expected: 无类型错误

- [ ] **Step 2: 确认无残留的旧 transition name 引用**

搜索所有组件中是否还有旧的 transition name：

Run: `cd bedcode-mobile && grep -rn 'name="modal"\|name="fade"\|name="confirm"\|name="modal-fade"' src/components/ src/views/`
Expected: 无匹配结果（所有旧名称已替换为 `center-modal` 或 `bottom-sheet`）

- [ ] **Step 3: 确认无残留的 `@keyframes` 入场动画**

Run: `cd bedcode-mobile && grep -rn '@keyframes modal-in\|@keyframes slide-up' src/components/ src/views/`
Expected: 无匹配结果

- [ ] **Step 4: 确认所有 `.modal-panel` class 已添加**

Run: `cd bedcode-mobile && grep -rn 'modal-panel' src/components/ src/views/`
Expected: 每个弹窗组件的内容面板 div 都有 `modal-panel` class

- [ ] **Step 5: 确认无残留的 scoped transition CSS**

Run: `cd bedcode-mobile && grep -rn '\.modal-enter\|\.fade-enter\|\.confirm-enter\|\.modal-fade-enter' src/components/ src/views/`
Expected: 无匹配结果

- [ ] **Step 6: 手动运行 dev 服务器进行视觉验证**

Run: `cd bedcode-mobile && npm run tauri:android:dev`

逐一验证以下弹窗的打开/关闭动画：
1. Modal.vue — center-modal（缩放弹出）
2. ConfirmDialog.vue — center-modal
3. BottomSheet.vue — center-modal
4. TaskEditDialog.vue — center-modal
5. FileViewerModal.vue — center-modal
6. TaskPickerModal.vue — bottom-sheet（底部滑入）
7. ShortcutConfigModal.vue — bottom-sheet
8. ShortcutHelpModal.vue — bottom-sheet
9. TerminalConfirmModal.vue — center-modal
10. TerminalSettingsModal.vue — center-modal
11. CodeViewerSettingsModal.vue — center-modal
12. SettingsModal.vue — center-modal
13. ToolboxView session picker — bottom-sheet
14. ToolboxView confirm dialog — center-modal
15. SettingsView 确认弹窗 — center-modal

每个弹窗验证：
- 打开动画流畅（280ms）
- 关闭动画流畅（280ms，不闪现）
- Backdrop 淡入淡出正常
- Content 动画方向正确（center-modal: scale, bottom-sheet: translateY）

- [ ] **Step 7: Final commit（如有修复）**

```bash
git add -A
git commit -m "fix(mobile): modal transition integration fixes"
```

---

## 注意事项

1. **关闭动画必须播完**：使用 `v-if` 而非 `v-show` 控制 `<Transition>`，Vue 会等 leave 动画播完再移除 DOM
2. **Teleport 一致性**：所有模态弹窗都应使用 `<Teleport to="body">`，避免被父容器 `overflow: hidden` 裁切
3. **z-index 层级不变**：沿用现有的 z-50 / z-100 / z-110 / z-120 体系
4. **`.modal-panel` class**：内容面板必须加此 class 才能被全局 CSS 选择器匹配到动画规则
5. **性能**：`transform` 和 `opacity` 是 GPU 加速属性，不会触发重排，280ms 在移动端足够流畅
