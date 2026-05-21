import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { resolve } from 'path'

const host = process.env.TAURI_DEV_HOST
const isMobile = !!/android|ios/.exec(process.env.TAURI_ENV_PLATFORM || '')

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src'),
    },
  },
  server: {
    host: isMobile || host ? '0.0.0.0' : false,
    port: 1420,
    strictPort: true,
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
    // 预打包所有依赖，避免首次请求时扫描 discovery
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
      'html5-qrcode',
      'uuid',
    ],
    // 不要等爬取完才开始预打包，并行加速
    holdUntilCrawlEnd: false,
  },
  clearScreen: false,
})