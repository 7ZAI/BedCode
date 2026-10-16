import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'
import { resolve } from 'path'

export default defineConfig({
  // @ts-expect-error -- vite 5/8 双版本并存（node_modules/.pnpm），plugin-vue 的
  // Plugin<Api> 与 vitest 解析到的 vite PluginOption 类型不兼容——工作区既有噪音，
  // 仅行级压制，不改任何插件行为
  plugins: [vue()],
  test: {
    environment: 'happy-dom',
    globals: true,
    include: [
      'src/__tests__/**/*.test.ts',
      'packages/plugin-sdk-desktop/__tests__/**/*.test.ts',
      // 插件侧无独立 vitest 依赖（离线），复用宿主 vitest 运行（vitest 按工作区
      // hoisting 解析插件依赖）：agent-hub diff 纯函数
      'wasm-apps/agent-hub/src/__tests__/**/*.test.ts',
      // 会话中心插件：工程契约测试从第一天起进门禁（票 03）；票 17 起旧 auto-task
      // 的任务域视图与测试面一并迁入本插件（旧插件工程无测试面，缺口在迁移时补上）
      'wasm-apps/terminal-session/src/__tests__/**/*.test.ts',
      // 文件传输插件：taskReason 防泄漏锁（ADR 0030 收口，2026-09-28）——纯函数
      // 测试由根 vitest 运行（该插件无独立 vitest 依赖，与 agent-hub 同模式）
      'wasm-apps/file-transfer/src/__tests__/**/*.test.ts',
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
