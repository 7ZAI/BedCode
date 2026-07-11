import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { resolve } from 'path'
import { bedcodePlugin } from '@bedcode/plugin-sdk-desktop/vite'

const pluginId = 'com.bedcode.ai-chatbox'

export default defineConfig({
  plugins: [vue(), bedcodePlugin()],
  define: {
    'process.env.NODE_ENV': JSON.stringify('production'),
  },
  build: {
    lib: {
      entry: resolve(__dirname, 'index.ts'),
      formats: ['es'],
      fileName: () => 'index.js',
    },
    outDir: resolve(__dirname, '../../../src-tauri/resources/plugins/desktop', pluginId),
    emptyOutDir: false,
    minify: 'terser',
  },
})
