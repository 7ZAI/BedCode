/**
 * @bedcode/plugin-sdk-mobile Vite 插件
 *
 * 职责：
 * 1. 共享模块（vue/vue-i18n/pinia）外置为宿主全局（window.__BEDCODE_SHARED__）
 * 2. lib 模式下将全部 CSS（含 SFC `<style scoped>`）内联进入口 chunk ——
 *    宿主加载插件只 import() 入口 JS，不加载 dist/style.css；不内联则
 *    插件 scoped 样式全部丢失（真机上表现为组件样式错乱，如下拉刷新指示器
 *    箭头失去宽高约束渲染成巨幅图形）
 */
import type { Plugin, UserConfig } from 'vite'
import MagicString from 'magic-string'

const SHARED_MODULES: Record<string, string> = {
  'vue': 'window.__BEDCODE_SHARED__["vue"]',
  'vue-i18n': 'window.__BEDCODE_SHARED__["vue-i18n"]',
  'pinia': 'window.__BEDCODE_SHARED__["pinia"]',
}

export function bedcodePlugin(): Plugin[] {
  return [sharedModulesPlugin(), inlinePluginCss()]
}

/** 共享模块外置：插件产物引用宿主全局，避免重复实例化 Vue/Pinia */
function sharedModulesPlugin(): Plugin {
  const externalModules = Object.keys(SHARED_MODULES)

  return {
    name: 'bedcode-shared-modules',
    enforce: 'pre',

    config(config: UserConfig) {
      const existingExternal = config.build?.rollupOptions?.external
      const externalArray = Array.isArray(existingExternal)
        ? existingExternal
        : typeof existingExternal === 'string'
          ? [existingExternal]
          : []

      return {
        build: {
          rollupOptions: {
            external: [...externalArray, ...externalModules],
          },
        },
      }
    },

    renderChunk(code, chunk) {
      let modified = false
      const s = new MagicString(code)

      for (const [modName, globalExpr] of Object.entries(SHARED_MODULES)) {
        const escapedName = modName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

        const defaultRe = new RegExp(
          `import\\s+(\\w+)\\s+from\\s+['"]${escapedName}['"]`, 'gm'
        )
        let match: RegExpExecArray | null
        while ((match = defaultRe.exec(code)) !== null) {
          const varName = match[1]
          s.overwrite(match.index, match.index + match[0].length, `const ${varName} = ${globalExpr}`)
          modified = true
        }

        const namedRe = new RegExp(
          `import\\s*\\{([^}]+)\\}\\s*from\\s+['"]${escapedName}['"]`, 'gm'
        )
        while ((match = namedRe.exec(code)) !== null) {
          const imports = match[1]
          s.overwrite(match.index, match.index + match[0].length, `const { ${imports} } = ${globalExpr}`)
          modified = true
        }

        const namespaceRe = new RegExp(
          `import\\s+\\*\\s+as\\s+(\\w+)\\s+from\\s+['"]${escapedName}['"]`, 'gm'
        )
        while ((match = namespaceRe.exec(code)) !== null) {
          const varName = match[1]
          s.overwrite(match.index, match.index + match[0].length, `const ${varName} = ${globalExpr}`)
          modified = true
        }
      }

      if (!modified) return null

      return {
        code: s.toString(),
        map: s.generateMap({ hires: true }),
      }
    },
  }
}

/**
 * CSS 内联注入（与桌面端 ai-chatbox 插件 inlinePluginCss 同模式）：
 *
 * vite 库模式构建下提取出的 CSS asset 无人引用（宿主动态 import 的只有
 * manifest.main 对应的 index.js），而 vite:css-post 在 generateBundle 阶段才
 * 把 CSS 写入 bundle —— 普通（非 post）插件先于它运行拿不到 CSS 内容，
 * 故本插件必须 enforce: 'post' 保证在 css-post 之后执行。
 */
function inlinePluginCss(): Plugin {
  return {
    name: 'bedcode-inline-plugin-css',
    apply: 'build',
    enforce: 'post',
    generateBundle(_options, bundle) {
      const entry = Object.values(bundle).find((f) => f.type === 'chunk' && f.isEntry)
      if (!entry || entry.type !== 'chunk') return

      const cssParts: string[] = []
      for (const fileName of Object.keys(bundle)) {
        if (!fileName.endsWith('.css')) continue
        const css = bundle[fileName]
        if (css.type !== 'asset') continue
        cssParts.push(
          typeof css.source === 'string' ? css.source : new TextDecoder().decode(css.source),
        )
        delete bundle[fileName]
      }
      if (cssParts.length === 0) return

      const injected =
        `;(function(){var s=document.createElement('style');` +
        `s.setAttribute('data-bedcode-plugin-css','');` +
        `s.textContent=${JSON.stringify(cssParts.join('\n'))};` +
        `document.head.appendChild(s)})();`
      entry.code = injected + '\n' + entry.code
    },
  }
}
