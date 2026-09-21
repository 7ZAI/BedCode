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
      // 插件侧无独立 vitest 依赖（离线），复用宿主 vitest 运行（vitest 按工作区
      // hoisting 解析插件依赖）：agent-hub diff 纯函数
      'plugins/agent-hub/src/__tests__/**/*.test.ts',
      // 会话中心插件：工程契约测试从第一天起进门禁（票 03）；票 17 起旧 auto-task
      // 的任务域视图与测试面一并迁入本插件（旧插件工程无测试面，缺口在迁移时补上）
      'plugins/session/src/__tests__/**/*.test.ts',
    ],
    exclude: ['node_modules', 'dist'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      exclude: ['node_modules/', 'src/__tests__/'],
    },
  },
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src'),
    },
  },
})
