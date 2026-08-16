import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { resolve } from 'path'

const host = process.env.TAURI_DEV_HOST

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src'),
    },
  },
  server: {
    host: host ? '0.0.0.0' : false,
    port: 1420,
    strictPort: true,
    watch: {
      // 排除巨型构建目录，避免 chokidar 扫描/监听数万文件霸占事件循环导致请求挂起
      ignored: [
        '**/src-tauri/target/**',
        '**/src-tauri/gen/**',
        '**/rust/target/**',
        '**/dist/**',
        '**/node_modules/**',
        '**/.git/**',
      ],
    },
    hmr: host
      ? {
          protocol: 'ws',
          host: host,
          port: 1421,
        }
      : undefined,
    fs: {
      allow: [
        resolve(__dirname, 'src'),
        // 插件 SDK 源码（@binblink/plugin-sdk-desktop 经 file: symlink 解析为真实路径，
        // 共享 UI 组件如 Select.vue 在 packages/ 下，dev server 需显式放行）
        resolve(__dirname, 'packages'),
        resolve(__dirname, 'index.html'),
        resolve(__dirname, 'public'),
        resolve(__dirname, 'node_modules'),
        resolve(__dirname, 'package.json'),
        resolve(__dirname, 'vite.config.ts'),
        resolve(__dirname, 'tailwind.config.js'),
        resolve(__dirname, 'postcss.config.js'),
        resolve(__dirname, 'tsconfig.json'),
      ],
    },
  },
  optimizeDeps: {
    entries: ['./index.html'],
    include: [
      'vue',
      'vue-router',
      'pinia',
      '@tauri-apps/api/core',
      '@tauri-apps/api/event',
      '@tauri-apps/api/window',
      '@tauri-apps/plugin-dialog',
      '@tauri-apps/plugin-os',
      '@tauri-apps/plugin-shell',
      '@tauri-apps/plugin-notification',
      '@xterm/xterm',
      '@xterm/addon-fit',
      '@xterm/addon-web-links',
      'ansi_up',
      'qrcode',
      'uuid',
    ],
    holdUntilCrawlEnd: false,
  },
  clearScreen: false,
})
