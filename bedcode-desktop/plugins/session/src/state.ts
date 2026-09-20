/**
 * Terminal Session Center 插件前端共享状态
 *
 * 终端工具栏按钮（index.ts 注册）与任务队列弹窗（TaskQueueModal.vue）之间共享可见性。
 * 模块级 ref 在插件 bundle 的单个实例内共享（每个 webview 独立加载 bundle）。
 */
import { ref } from 'vue'

/** 任务队列弹窗是否可见 */
export const taskModalVisible = ref(false)
