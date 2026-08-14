import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'

/**
 * 计划任务插件测试配置（与 ai-chatbox 同模式：插件内独立跑 vitest run，
 * 宿主根 vitest 只覆盖 src/ 与 SDK，不包含 plugins/）
 */
export default defineConfig({
  plugins: [vue()],
  test: {
    environment: 'happy-dom',
    globals: true,
    include: ['src/__tests__/**/*.test.ts'],
    exclude: ['node_modules', 'dist'],
  },
})
