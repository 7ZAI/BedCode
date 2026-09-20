import { defineConfig, type Plugin } from 'vite'
import vue from '@vitejs/plugin-vue'
import { resolve } from 'path'
import { bedcodePlugin } from '@binblink/bedcode-plugin-sdk-desktop/vite'

/**
 * Terminal Session Center 插件前端构建配置
 *
 * `bedcodePlugin()` 把 vue / vue-i18n / pinia / vue-sonner 外部化并在产物里改读
 * `window.__BEDCODE_SHARED__`（D2 前端收口：插件不得自带第二份运行时）。
 *
 * `inlinePluginCss()`（票 13 搬入视图组件时必需，与 auto-task 同款）：库模式下 Vite
 * 不会把提取出的 CSS 注入 JS，宿主动态 import 的只有 index.js —— 不内联则本插件
 * 所有 scoped 样式（Tab 切换过渡 `.tab-fade-*`、弹窗过渡 `.modal-*`）静默失效。
 */
function inlinePluginCss(): Plugin {
  return {
    name: 'inline-plugin-css',
    apply: 'build',
    // vite:css-post 在 post 阶段才把提取出的 CSS asset 写入 bundle，
    // 本插件必须在它之后运行才能拿到 CSS 内容
    enforce: 'post',
    generateBundle(_options, bundle) {
      const entry = Object.values(bundle).find((f) => f.type === 'chunk' && f.isEntry)
      if (!entry || entry.type !== 'chunk') return

      for (const fileName of Object.keys(bundle)) {
        if (!fileName.endsWith('.css')) continue
        const css = bundle[fileName]
        if (css.type !== 'asset') continue
        const text =
          typeof css.source === 'string' ? css.source : Buffer.from(css.source).toString('utf-8')
        const injection =
          ';!function(){' +
          'var o=document.querySelector("style[data-plugin-css=\\"session\\"]");' +
          'if(o)o.remove();' +
          'var s=document.createElement("style");' +
          's.setAttribute("data-plugin-css","session");' +
          's.textContent=' +
          JSON.stringify(text) +
          ';' +
          'document.head.appendChild(s);}();'
        entry.code = injection + '\n' + entry.code
        delete bundle[fileName]
      }
    },
  }
}

export default defineConfig({
  plugins: [vue(), bedcodePlugin(), inlinePluginCss()],
  define: {
    'process.env.NODE_ENV': JSON.stringify('production'),
  },
  build: {
    lib: {
      entry: resolve(__dirname, 'src/index.ts'),
      formats: ['es'],
      fileName: () => 'index.js',
    },
    outDir: resolve(__dirname, 'dist'),
    emptyOutDir: true,
    minify: 'terser',
    // 插件产物契约 = 单文件 index.js（manifest.main，宿主按目录整包分发）：
    // 表单里的动态 import（@tauri-apps/plugin-dialog 目录选择器）会触发代码分割
    // 产出额外 chunk，而构建/watch 脚本只同步 index.js 到宿主资源目录，分包会导致
    // 打包后入口 import 缺失文件加载失败——强制内联保持单产物（file-transfer 先例）
    rollupOptions: {
      output: { inlineDynamicImports: true },
    },
  },
})
