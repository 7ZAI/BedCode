# 移动端弹窗动画统一 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 bedcode-mobile 所有弹窗/模态框添加统一的打开/关闭动画，消除闪现问题

**Architecture:** 在全局 `mobile.css` 中定义两套 Vue transition CSS（`center-modal` 和 `bottom-sheet`），所有弹窗组件统一引用并删除自带的 scoped transition CSS。使用 `.modal-panel` class 标记内容面板元素。

**Tech Stack:** Vue 3 `<Transition>`, CSS transitions, TailwindCSS

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `bedcode-mobile/src/styles/mobile.css` | Modify | 添加全局 `center-modal` 和 `bottom-sheet` transition CSS |
| `bedcode-mobile/src/components/Modal.vue` | Modify | 替换 transition name，加 `.modal-panel`，删 scoped CSS |
| `bedcode-mobile/src/components/ConfirmDialog.vue` | Modify | 替换 transition name，加 `.modal-panel`，删 scoped CSS |
| `bedcode-mobile/src/components/BottomSheet.vue` | Modify | 替换 transition name，加 `.modal-panel`，删 scoped CSS |
| `bedcode-mobile/src/components/TaskEditDialog.vue` | Modify | 替换 transition name，加 `.modal-panel`，删 scoped CSS |
| `bedcode-mobile/src/components/FileViewerModal.vue` | Modify | 替换 transition name，加 `.modal-panel`，删 scoped CSS |
| `bedcode-mobile/src/components/TaskPickerModal.vue` | Modify | 加 `<Transition>`，加 `.modal-panel`，删 `@keyframes`，重构为标准 backdrop + panel 结构 |
| `bedcode-mobile/src/components/ShortcutConfigModal.vue` | Modify | 加 `<Transition>`，加 `.modal-panel`，删 `@keyframes`，重构为标准结构 |
| `bedcode-mobile/src/components/ShortcutHelpModal.vue` | Modify | 加 `<Transition>`，加 `.modal-panel`，删 `@keyframes`，重构为标准结构 |
| `bedcode-mobile/src/components/TerminalConfirmModal.vue` | Modify | 加 `<Teleport>` + `<Transition>`，加 `.modal-panel` |
| `bedcode-mobile/src/components/TerminalSettingsModal.vue` | Modify | 加 `<Teleport>` + `<Transition>`，加 `.modal-panel` |
| `bedcode-mobile/src/components/CodeViewerSettingsModal.vue` | Modify | 加 `<Transition>`，加 `.modal-panel` |
| `bedcode-mobile/src/components/SettingsModal.vue` | Modify | 加 `<Transition>`，加 `.modal-panel` |
| `bedcode-mobile/src/views/ToolboxView.vue` | Modify | 替换 inline dialog 的 transition name，加 `.modal-panel` |
| `bedcode-mobile/src/views/SettingsView.vue` | Modify | 替换 inline dialog 为 `<Transition>`，加 `.modal-panel` |

---

### Task 1: 添加全局 transition CSS

**Files:**
- Modify: `bedcode-mobile/src/styles/mobile.css` (末尾追加)

- [ ] **Step 1: 在 mobile.css 末尾追加全局 transition 定义**

在文件末尾（`mobile-loading-fade` 部分之后）追加：

```css
/* ============================================
   Modal Transition Classes
   全局弹窗动画 — 所有模态弹窗统一使用
   ============================================ */

/* Center Modal: scale + fade — 用于确认框、设置弹窗、编辑表单 */
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

/* Bottom Sheet: slide up — 用于大面板、配置、帮助文档 */
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

- [ ] **Step 2: Commit**

```bash
git add bedcode-mobile/src/styles/mobile.css
git commit -m "feat(mobile): add global center-modal and bottom-sheet transition CSS"
```

---

### Task 2: 迁移已有 Transition 的组件（center-modal 组）

这 5 个组件已有 `<Transition>`，只需替换 name 并删除 scoped transition CSS。

**Files:**
- Modify: `bedcode-mobile/src/components/Modal.vue`
- Modify: `bedcode-mobile/src/components/ConfirmDialog.vue`
- Modify: `bedcode-mobile/src/components/BottomSheet.vue`
- Modify: `bedcode-mobile/src/components/TaskEditDialog.vue`
- Modify: `bedcode-mobile/src/components/FileViewerModal.vue`

- [ ] **Step 1: 修改 Modal.vue**

1. 将 `<Transition name="modal">` 改为 `<Transition name="center-modal">`
2. 在 content panel div 上加 `modal-panel` class：将 `class="relative rounded-xl shadow-2xl border..."` 改为 `class="relative modal-panel rounded-xl shadow-2xl border..."`
3. 删除整个 `<style scoped>` 块中的 transition CSS（`.modal-enter-active` 到 `.modal-leave-to > div:last-child { transform: scale(0.95); }`）

具体删除的 CSS：
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

如果 `<style scoped>` 块删除后为空，则删除整个 `<style scoped>` 标签。

- [ ] **Step 2: 修改 ConfirmDialog.vue**

1. 将 `<Transition name="confirm">` 改为 `<Transition name="center-modal">`
2. 在 content panel div 上加 `modal-panel` class：将 `class="relative w-full max-w-sm mx-4..."` 改为 `class="relative modal-panel w-full max-w-sm mx-4..."`
3. 删除整个 `<style scoped>` 中的 transition CSS：

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

如果 `<style scoped>` 块删除后为空，则删除整个 `<style scoped>` 标签。

- [ ] **Step 3: 修改 BottomSheet.vue**

1. 将 `<Transition name="fade">` 改为 `<Transition name="center-modal">`
2. 在 content panel div 上加 `modal-panel` class：将 `class="relative w-full max-w-sm bg-..."` 改为 `class="relative modal-panel w-full max-w-sm bg-..."`
3. 删除 `<style scoped>` 中的 transition CSS：

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

如果 `<style scoped>` 块删除后为空，则删除整个 `<style scoped>` 标签。

- [ ] **Step 4: 修改 TaskEditDialog.vue**

1. 将两处 `<Transition name="modal">` 改为 `<Transition name="center-modal">`
   - 第一处：主弹窗（z-[110]）
   - 第二处：FileExplorer 弹窗（z-[120]）
2. 在两个 content panel div 上加 `modal-panel` class：
   - 主弹窗：`class="relative w-full max-w-lg bg-..."` → `class="relative modal-panel w-full max-w-lg bg-..."`
   - FileExplorer 弹窗：`class="relative w-full h-full bg-..."` → `class="relative modal-panel w-full h-full bg-..."`
3. 删除 `<style scoped>` 中的 transition CSS：

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

/* Dropdown transition */
.dropdown-enter-active,
.dropdown-leave-active {
  transition: all 0.2s ease;
}

.dropdown-enter-from,
.dropdown-leave-to {
  opacity: 0;
  transform: translateY(-8px);
}
```

**注意：** `dropdown` transition 在目录选择下拉菜单中使用，保留不动，只删除 `modal` transition 部分。

- [ ] **Step 5: 修改 FileViewerModal.vue**

1. 将 `<transition name="modal-fade">` 改为 `<Transition name="center-modal">`
2. 在 content panel div 上加 `modal-panel` class：将 `class="viewer-modal"` 改为 `class="viewer-modal modal-panel"`
3. 删除 `<style scoped>` 末尾的 transition CSS：

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

- [ ] **Step 6: Commit**

```bash
git add bedcode-mobile/src/components/Modal.vue bedcode-mobile/src/components/ConfirmDialog.vue bedcode-mobile/src/components/BottomSheet.vue bedcode-mobile/src/components/TaskEditDialog.vue bedcode-mobile/src/components/FileViewerModal.vue
git commit -m "refactor(mobile): migrate existing modals to global center-modal transition"
```

---

### Task 3: 迁移 bottom-sheet 组组件

这 3 个组件目前只有入场动画（`@keyframes`），需要改为 `<Transition>` + 全局 CSS。

**Files:**
- Modify: `bedcode-mobile/src/components/TaskPickerModal.vue`
- Modify: `bedcode-mobile/src/components/ShortcutConfigModal.vue`
- Modify: `bedcode-mobile/src/components/ShortcutHelpModal.vue`

- [ ] **Step 1: 修改 TaskPickerModal.vue**

1. 重构模板结构为标准 backdrop + panel 形式。当前 `.modal-overlay` 既是容器又是 backdrop，需要拆分。

将模板改为：
```html
<template>
  <Teleport to="body">
    <Transition name="bottom-sheet">
      <div v-if="..." class="modal-overlay mobile-ui" @click.self="emit('close')">
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-[var(--mobile-overlay)]" @click="emit('close')"></div>
        <!-- Content Panel -->
        <div class="modal-content modal-panel">
          ...（保持不变）
        </div>
      </div>
    </Transition>

    <!-- 新增/编辑任务弹窗（使用共享组件） -->
    <TaskEditDialog ... />
  </Teleport>
</template>
```

关键变更：
- 在 `.modal-overlay` 外包一层 `<Transition name="bottom-sheet">`
- 原来的 `.modal-overlay` 的 `background: var(--mobile-overlay)` 样式删除，改用子元素 backdrop
- `.modal-content` 加上 `modal-panel` class
- `@click.self="emit('close')"` 保留在 overlay 上
- 删除 `@keyframes modal-in` 和 `.modal-content { animation: modal-in 0.2s ease; }`

CSS 变更：
- 从 `.modal-overlay` 样式中删除 `background: var(--mobile-overlay);`
- 删除 `@keyframes modal-in { from { opacity: 0; transform: scale(0.95); } to { opacity: 1; transform: scale(1); } }`
- 删除 `.modal-content { animation: modal-in 0.2s ease; }`（注意 `.modal-content` 的其他样式保留）
- `.modal-overlay` 添加 `position: relative;`（保持 inset 布局）

2. 由于 `.modal-overlay` 已有 `position: fixed; inset: 0;`，backdrop 子元素用 `absolute inset-0` 覆盖即可。

- [ ] **Step 2: 修改 ShortcutConfigModal.vue**

1. 在外层 `<Teleport>` 内的容器 div 外包 `<Transition name="bottom-sheet">`

当前结构：
```html
<Teleport to="body">
  <div v-if="visible" class="fixed inset-0 z-[100] flex items-end justify-center mobile-ui" ...>
    <div class="absolute inset-0 bg-[var(--mobile-overlay-light)]" @click="emit('close')"></div>
    <div class="shortcut-config-modal relative ...">
```

改为：
```html
<Teleport to="body">
  <Transition name="bottom-sheet">
    <div v-if="visible" class="fixed inset-0 z-[100] flex items-end justify-center mobile-ui" ...>
      <div class="absolute inset-0 bg-[var(--mobile-overlay-light)]" @click="emit('close')"></div>
      <div class="shortcut-config-modal relative modal-panel ...">
```

2. 在 `.shortcut-config-modal` div 上加 `modal-panel` class
3. 删除 `<style scoped>` 中的 `@keyframes slide-up` 和 `.shortcut-config-modal { animation: slide-up 0.25s ...; }`

删除：
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

4. 添加闭合标签 `</Transition>`

注意：此组件内部还有 `confirmDeleteCode` 内联弹窗和 `ShortcutHelpModal`，这些在 `<Transition>` 包裹的容器 div 之外（或在容器内部的嵌套层级），不影响外层 transition。

- [ ] **Step 3: 修改 ShortcutHelpModal.vue**

当前结构：
```html
<Teleport to="body">
  <div v-if="visible" class="fixed inset-0 z-[120] flex items-end justify-center mobile-ui" ...>
    <div class="absolute inset-0 bg-[var(--mobile-overlay-light)]" @click="emit('close')"></div>
    <div class="shortcut-help-modal relative ...">
```

改为：
```html
<Teleport to="body">
  <Transition name="bottom-sheet">
    <div v-if="visible" class="fixed inset-0 z-[120] flex items-end justify-center mobile-ui" ...>
      <div class="absolute inset-0 bg-[var(--mobile-overlay-light)]" @click="emit('close')"></div>
      <div class="shortcut-help-modal relative modal-panel ...">
```

1. 在容器 div 外包 `<Transition name="bottom-sheet">`
2. 在 `.shortcut-help-modal` div 上加 `modal-panel` class
3. 删除 `<style scoped>` 中的 `@keyframes slide-up` 和 `.shortcut-help-modal { animation: slide-up 0.25s ...; }`

删除：
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

4. 添加闭合标签 `</Transition>`

- [ ] **Step 4: Commit**

```bash
git add bedcode-mobile/src/components/TaskPickerModal.vue bedcode-mobile/src/components/ShortcutConfigModal.vue bedcode-mobile/src/components/ShortcutHelpModal.vue
git commit -m "refactor(mobile): migrate bottom-sheet modals to global transition"
```

---

### Task 4: 为无动画组件添加 Transition（center-modal 组）

这 4 个组件完全没有动画，需要加 `<Teleport>` + `<Transition name="center-modal">` + `.modal-panel`。

**Files:**
- Modify: `bedcode-mobile/src/components/TerminalConfirmModal.vue`
- Modify: `bedcode-mobile/src/components/TerminalSettingsModal.vue`
- Modify: `bedcode-mobile/src/components/CodeViewerSettingsModal.vue`
- Modify: `bedcode-mobile/src/components/SettingsModal.vue`

- [ ] **Step 1: 修改 TerminalConfirmModal.vue**

当前结构：
```html
<div v-if="visible" class="confirm-modal-overlay mobile-ui" @click.self="$emit('cancel')">
  <div class="confirm-modal" :style="safeAreaStyle">
```

改为：
```html
<Teleport to="body">
  <Transition name="center-modal">
    <div v-if="visible" class="confirm-modal-overlay mobile-ui" @click.self="$emit('cancel')">
      <div class="confirm-modal modal-panel" :style="safeAreaStyle">
```

并在末尾 `</div>` 后加上 `</Transition></Teleport>`。

变更：
1. 外层包 `<Teleport to="body">` + `<Transition name="center-modal">`
2. `.confirm-modal` div 加 `modal-panel` class
3. 删除无用的 `@click.self`（改为 backdrop overlay 方式）

进一步优化：添加 backdrop 子元素替换 `@click.self`：
```html
<Teleport to="body">
  <Transition name="center-modal">
    <div v-if="visible" class="confirm-modal-overlay mobile-ui">
      <div class="absolute inset-0" @click="$emit('cancel')"></div>
      <div class="confirm-modal modal-panel" :style="safeAreaStyle">
```

- [ ] **Step 2: 修改 TerminalSettingsModal.vue**

当前结构：
```html
<div v-if="visible" class="settings-modal-overlay mobile-ui" @click.self="$emit('cancel')">
  <div class="settings-modal" :style="safeAreaStyle">
```

改为：
```html
<Teleport to="body">
  <Transition name="center-modal">
    <div v-if="visible" class="settings-modal-overlay mobile-ui">
      <div class="absolute inset-0" @click="$emit('cancel')"></div>
      <div class="settings-modal modal-panel" :style="safeAreaStyle">
```

并在末尾 `</div>` 后加上 `</Transition></Teleport>`。

1. 外层包 `<Teleport to="body">` + `<Transition name="center-modal">`
2. 删除 `@click.self`，改用子元素 backdrop
3. `.settings-modal` div 加 `modal-panel` class

- [ ] **Step 3: 修改 CodeViewerSettingsModal.vue**

已有 `<Teleport>`，只需加 `<Transition>` 和 `.modal-panel`。

当前结构：
```html
<Teleport to="body">
  <div v-if="visible" class="settings-modal-overlay mobile-ui" @click.self="emit('close')">
    <div class="settings-modal" :style="modalStyle">
```

改为：
```html
<Teleport to="body">
  <Transition name="center-modal">
    <div v-if="visible" class="settings-modal-overlay mobile-ui">
      <div class="absolute inset-0" @click="emit('close')"></div>
      <div class="settings-modal modal-panel" :style="modalStyle">
```

并在末尾 `</div>` 后加上 `</Transition>`。

1. 在 `<Teleport>` 内加 `<Transition name="center-modal">`
2. 删除 `@click.self`，改用子元素 backdrop
3. `.settings-modal` div 加 `modal-panel` class

- [ ] **Step 4: 修改 SettingsModal.vue**

当前结构：
```html
<Teleport to="body">
  <div v-if="visible" class="fixed inset-0 z-[100] flex items-center justify-center p-4 mobile-ui" @click.self="emit('close')">
    <div class="absolute inset-0 bg-[var(--mobile-overlay-light)]" @click="emit('close')"></div>
    <div class="relative bg-[var(--mobile-bg-card)] border ...">
```

已有 backdrop 子元素，只需加 `<Transition>` 和 `.modal-panel`。

改为：
```html
<Teleport to="body">
  <Transition name="center-modal">
    <div v-if="visible" class="fixed inset-0 z-[100] flex items-center justify-center p-4 mobile-ui" @click.self="emit('close')">
      <div class="absolute inset-0 bg-[var(--mobile-overlay-light)]" @click="emit('close')"></div>
      <div class="relative modal-panel bg-[var(--mobile-bg-card)] border ...">
```

1. 在 `<Teleport>` 内加 `<Transition name="center-modal">`
2. content panel div 加 `modal-panel` class
3. 在末尾 `</div>` 后加上 `</Transition>`

- [ ] **Step 5: Commit**

```bash
git add bedcode-mobile/src/components/TerminalConfirmModal.vue bedcode-mobile/src/components/TerminalSettingsModal.vue bedcode-mobile/src/components/CodeViewerSettingsModal.vue bedcode-mobile/src/components/SettingsModal.vue
git commit -m "feat(mobile): add center-modal transition to components without animation"
```

---

### Task 5: 迁移视图内联弹窗

ToolboxView 和 SettingsView 中有内联弹窗，需要迁移到全局 transition。

**Files:**
- Modify: `bedcode-mobile/src/views/ToolboxView.vue`
- Modify: `bedcode-mobile/src/views/SettingsView.vue`

- [ ] **Step 1: 修改 ToolboxView.vue**

1. Session Picker Dialog：将 `<Transition name="fade">` 改为 `<Transition name="center-modal">`，在 content panel div 上加 `modal-panel` class

当前：
```html
<Transition name="fade">
  <div v-if="showSessionPicker" class="fixed inset-0 z-50 flex items-center justify-center p-4 mobile-ui">
    <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]" @click="showSessionPicker = false"></div>
    <div class="relative w-full max-w-sm bg-[var(--mobile-bg-card)] ...">
```

改为：
```html
<Transition name="center-modal">
  <div v-if="showSessionPicker" class="fixed inset-0 z-50 flex items-center justify-center p-4 mobile-ui">
    <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]" @click="showSessionPicker = false"></div>
    <div class="relative modal-panel w-full max-w-sm bg-[var(--mobile-bg-card)] ...">
```

2. Confirm Execute Dialog：同理，将 `<Transition name="fade">` 改为 `<Transition name="center-modal">`，content panel 加 `modal-panel`

当前：
```html
<Transition name="fade">
  <div v-if="showConfirmDialog" class="fixed inset-0 z-50 flex items-center justify-center p-4 mobile-ui">
    <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]" @click="showConfirmDialog = false"></div>
    <div class="relative w-full max-w-sm bg-[var(--mobile-bg-card)] ...">
```

改为：
```html
<Transition name="center-modal">
  <div v-if="showConfirmDialog" class="fixed inset-0 z-50 flex items-center justify-center p-4 mobile-ui">
    <div class="absolute inset-0 bg-[var(--mobile-overlay-heavy)]" @click="showConfirmDialog = false"></div>
    <div class="relative modal-panel w-full max-w-sm bg-[var(--mobile-bg-card)] ...">
```

- [ ] **Step 2: 修改 SettingsView.vue**

1. Browser Confirm Modal 和 Confirm Dialog 都没有 `<Transition>`，需要加上。

当前 Browser Confirm：
```html
<Teleport to="body">
  <div v-if="showBrowserConfirm" class="confirm-modal-overlay mobile-ui" @click.self="cancelOpenBrowser">
    <div class="confirm-modal">
```

改为：
```html
<Teleport to="body">
  <Transition name="center-modal">
    <div v-if="showBrowserConfirm" class="confirm-modal-overlay mobile-ui">
      <div class="absolute inset-0" @click="cancelOpenBrowser"></div>
      <div class="confirm-modal modal-panel">
```

末尾加 `</Transition>`。

当前 Confirm Dialog：
```html
<Teleport to="body">
  <div v-if="showConfirm" class="confirm-modal-overlay mobile-ui" @click.self="cancelConfirm">
    <div class="confirm-modal">
```

改为：
```html
<Teleport to="body">
  <Transition name="center-modal">
    <div v-if="showConfirm" class="confirm-modal-overlay mobile-ui">
      <div class="absolute inset-0" @click="cancelConfirm"></div>
      <div class="confirm-modal modal-panel">
```

末尾加 `</Transition>`。

- [ ] **Step 3: Commit**

```bash
git add bedcode-mobile/src/views/ToolboxView.vue bedcode-mobile/src/views/SettingsView.vue
git commit -m "refactor(mobile): migrate inline view dialogs to global transition"
```

---

### Task 6: 视觉验证

- [ ] **Step 1: 启动开发服务器**

```bash
cd bedcode-mobile && npm run tauri:android:dev
```

或者如果只想在浏览器验证：
```bash
cd bedcode-mobile && npm run dev
```

- [ ] **Step 2: 逐个验证弹窗动画**

逐个触发以下弹窗，验证打开和关闭动画都流畅播放：

**center-modal 组（缩放弹出）：**
1. 终端设置弹窗 — 在终端页点击设置图标
2. 终端确认弹窗 — 点击清屏确认
3. 代码查看器设置 — 打开文件后点击设置
4. InputAssistant 设置 — 长按悬浮球
5. 确认对话框 — 停止会话确认
6. 底部输入弹窗 — 手动连接输入 IP
7. 任务编辑弹窗 — 新建/编辑任务
8. 文件查看器 — 点击文件查看
9. 工具箱确认执行弹窗
10. 设置页确认弹窗

**bottom-sheet 组（底部滑入）：**
1. 任务选择器 — 点击任务按钮
2. 快捷键配置 — 悬浮球配置
3. 快捷键帮助 — 配置页帮助按钮

**检查要点：**
- 打开动画流畅，有 scale/translateY 过渡
- 关闭动画流畅，不是闪现消失
- backdrop 淡入淡出
- 时长约 280ms，不快不慢
- 动画期间不影响交互（无抖动）

- [ ] **Step 3: 如有问题，修复并提交**

常见问题及修复：
- 如果某个弹窗动画不生效：检查 `.modal-panel` class 是否正确添加
- 如果 backdrop 不跟随淡入淡出：检查 backdrop 是否是容器 div 的子元素而非背景色
- 如果 content panel 初始位置不对：检查是否有 scoped CSS 覆盖了 transform

```bash
git add -A
git commit -m "fix(mobile): adjust modal transition issues from visual verification"
```
