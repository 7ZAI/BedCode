/**
 * Auto Task 插件前端共享状态
 *
 * 工具栏按钮（index.ts）与弹窗组件（AutoTaskPanelHost.vue）之间共享可见性状态。
 * 模块级 ref 在插件 bundle 的单个实例内共享。
 */
import { ref } from 'vue'

/** 自动任务面板是否可见 */
export const autoTaskPanelVisible = ref(false)
