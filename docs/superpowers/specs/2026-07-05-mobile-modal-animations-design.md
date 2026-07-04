# 移动端弹窗动画统一设计

## 概述

为 bedcode-mobile 所有弹窗/模态框添加统一的打开/关闭动画，消除当前"闪现"问题。定义两套标准 transition CSS，所有组件统一引用。

## 问题分析

当前 12 个弹窗组件的动画状态：

- **5 个完全没有动画**：TerminalConfirmModal、TerminalSettingsModal、CodeViewerSettingsModal、SettingsModal、ToolboxView inline dialogs
- **3 个只有入场动画**：TaskPickerModal（@keyframes modal-in）、ShortcutConfigModal（@keyframes slide-up）、ShortcutHelpModal（@keyframes slide-up）
- **4 个有完整但不一致的动画**：Modal.vue、ConfirmDialog.vue、BottomSheet.vue、TaskEditDialog.vue、FileViewerModal.vue — 各自定义不同时长、不同曲线、不同命名

## 动画规范

### 参数

| 参数 | 值 |
|------|-----|
| 时长 | 280ms |
| 曲线 | `cubic-bezier(0.32, 0.72, 0, 1)` |
| backdrop 曲线 | `ease` (简单淡入淡出) |
| backdrop 颜色 | 不变，沿用 `--mobile-overlay` / `--mobile-overlay-heavy` / `--mobile-overlay-light` |

### 类型 1: `center-modal` — 中央缩放弹出

适用：确认框、设置弹窗、编辑表单等小/中型弹窗

**打开：**
- Backdrop: `opacity 0 → 1` (280ms ease)
- Content: `opacity 0 → 1` + `scale(0.92) → scale(1)` (280ms cubic-bezier)

**关闭：**
- Backdrop: `opacity 1 → 0` (280ms ease)
- Content: `opacity 1 → 0` + `scale(1) → scale(0.92)` (280ms cubic-bezier)

### 类型 2: `bottom-sheet` — 底部滑入

适用：大面板、快捷键配置、帮助文档、任务选择等底部弹出面板

**打开：**
- Backdrop: `opacity 0 → 1` (280ms ease)
- Content: `translateY(100%) → translateY(0)` (280ms cubic-bezier)

**关闭：**
- Backdrop: `opacity 1 → 0` (280ms ease)
- Content: `translateY(0) → translateY(100%)` (280ms cubic-bezier)

## CSS 实现

在 `bedcode-mobile/src/styles/mobile.css` 中添加全局 transition 定义：

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

**设计说明：**
- 使用 `.modal-panel` class 定位内容面板，比 `> :last-child` 更可靠
- 所有弹窗的内容面板元素加 `class="modal-panel"`，由全局 CSS 统一驱动动画
- backdrop 和 content 在同一个 `<Transition>` 父级下自动协调
- CSS 不使用 `scoped`，放在全局 `mobile.css` 中供所有组件引用

## 组件改动清单

### 模板结构约定

所有弹窗采用统一的 DOM 结构：

```html
<Teleport to="body">
  <Transition name="center-modal">
    <div v-if="visible" class="fixed inset-0 z-XX flex ...">
      <!-- Backdrop -->
      <div class="absolute inset-0 bg-[var(--mobile-overlay)]" @click="close"></div>
      <!-- Content Panel (加 .modal-panel class) -->
      <div class="relative modal-panel ...">
        ...
      </div>
    </div>
  </Transition>
</Teleport>
```

### 逐组件改动

| 组件 | 动画类型 | 改动 |
|------|----------|------|
| Modal.vue | center-modal | 替换 `<Transition name="modal">` → `center-modal`，删除 scoped transition CSS |
| ConfirmDialog.vue | center-modal | 替换 `name="confirm"` → `center-modal`，删除 scoped transition CSS |
| BottomSheet.vue | center-modal | 替换 `name="fade"` → `center-modal`，删除 scoped transition CSS |
| TaskEditDialog.vue | center-modal | 替换 `name="modal"` → `center-modal`，删除 scoped transition CSS |
| FileViewerModal.vue | center-modal | 替换 `name="modal-fade"` → `center-modal`，删除 scoped transition CSS |
| TaskPickerModal.vue | bottom-sheet | 加 `<Transition name="bottom-sheet">`，删除 `@keyframes modal-in` |
| ShortcutConfigModal.vue | bottom-sheet | 加 `<Transition name="bottom-sheet">`，删除 `@keyframes slide-up` |
| ShortcutHelpModal.vue | bottom-sheet | 加 `<Transition name="bottom-sheet">`，删除 `@keyframes slide-up` |
| TerminalConfirmModal.vue | center-modal | 加 `<Teleport>` + `<Transition name="center-modal">` |
| TerminalSettingsModal.vue | center-modal | 加 `<Teleport>` + `<Transition name="center-modal">` |
| CodeViewerSettingsModal.vue | center-modal | 加 `<Transition name="center-modal">`（已有 Teleport） |
| SettingsModal.vue | center-modal | 加 `<Transition name="center-modal">` |

### ToolboxView 内联弹窗

ToolboxView 中的 `showSessionPicker` 和 `showConfirmDialog` 是内联弹窗（未使用组件），需要：
- session picker: 改用 `<Transition name="bottom-sheet">` 包裹
- confirm dialog: 改用 `ConfirmDialog` 组件 + center-modal

### SettingsView 内联弹窗

SettingsView 中的 `showBrowserConfirm` 和 `showConfirm` 同样是内联弹窗：
- 改用 `ConfirmDialog` 组件 + center-modal

### 不改动

- FileSidebar 内的 dropdown transition — 保持 `name="dropdown"`
- Toast 组件 — 有自己的动画逻辑
- FileSidebar 的 backdrop — 非 modal 场景

## 注意事项

1. **关闭动画必须播完**：使用 `v-if` 而非 `v-show` 控制 `<Transition>`，Vue 会等 leave 动画播完再移除 DOM
2. **Teleport 一致性**：所有模态弹窗都应使用 `<Teleport to="body">`，避免被父容器 `overflow: hidden` 裁切
3. **z-index 层级不变**：沿用现有的 z-50 / z-100 / z-110 / z-120 体系
4. **`.modal-panel` class**：内容面板必须加此 class 才能被全局 CSS 选择器匹配到动画规则
5. **性能**：`transform` 和 `opacity` 是 GPU 加速属性，不会触发重排，280ms 在移动端足够流畅
