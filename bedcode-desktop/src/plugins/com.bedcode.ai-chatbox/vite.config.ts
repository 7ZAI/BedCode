import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { resolve } from 'path'

const pluginId = 'com.bedcode.ai-chatbox'

export default defineConfig({
  plugins: [vue()],
  build: {
    lib: {
      entry: resolve(__dirname, 'index.ts'),
      formats: ['es'],
      fileName: () => 'index.js',
    },
    outDir: resolve(__dirname, '../../../src-tauri/resources/plugins/desktop', pluginId),
    emptyOutDir: false,
  },
})
