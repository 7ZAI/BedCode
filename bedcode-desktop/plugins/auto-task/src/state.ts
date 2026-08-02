/**
 * Auto Task 插件前端共享状态
 *
 * 工具栏按钮（index.ts）与弹窗组件（AutoTaskModal.vue）之间共享可见性状态。
 * 模块级 ref 在插件 bundle 的单个实例内共享（每个 webview 独立加载）。
 */
import { ref } from 'vue'

/** 自动任务弹窗是否可见 */
export const autoTaskModalVisible = ref(false)
