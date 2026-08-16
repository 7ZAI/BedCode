import { defineConfig, type Plugin } from 'vite'
import vue from '@vitejs/plugin-vue'
import { resolve } from 'path'
import { bedcodePlugin } from '@bedcode/plugin-sdk-mobile/vite'

/**
 * 库模式构建下 Vite 不会把提取出的 CSS 注入 JS：产物中的 style.css 无人引用，
 * 而宿主动态 import 的只有 index.js（manifest.main），因此插件所有 scoped 样式
 * （md-body markdown 排版 / Shiki 高亮 / thinking-block 思考块）在运行时完全不生效。
 *
 * 此插件在 generateBundle 阶段把 CSS 内联进入口 chunk，运行时注入宿主 document.head
 * （插件视图渲染在宿主 DOM 中）。data-plugin-css 标记保证插件重载时先移除旧样式再注入新样式。
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
          'var o=document.querySelector("style[data-plugin-css=\\"ai-chatbox\\"]");' +
          'if(o)o.remove();' +
          'var s=document.createElement("style");' +
          's.setAttribute("data-plugin-css","ai-chatbox");' +
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
  },
})
