import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'
import { resolve } from 'path'

export default defineConfig({
  plugins: [vue()],
  test: {
    environment: 'happy-dom',
    globals: true,
    include: [
      'src/__tests__/**/*.test.ts',
      'packages/plugin-sdk-desktop/__tests__/**/*.test.ts',
      // 计划任务插件（scheduler）只读面板测试：插件侧无独立 vitest 依赖（离线），
      // 复用宿主 vitest 运行（vitest 按工作区 hoisting 解析插件依赖）
      'plugins/scheduler/src/__tests__/**/*.test.ts',
    ],
    exclude: ['node_modules', 'dist'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      exclude: ['node_modules/', 'src/__tests__/']
    }
  },
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src'),
    },
  },
})