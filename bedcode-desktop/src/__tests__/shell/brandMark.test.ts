/**
 * 桌面品牌标志几何锁（源码扫描型）
 * -----------------------------------------------------------------------------
 * 为什么需要：品牌标志有多处渲染点，且分属两套 UI——旧布局 `src/components/`
 * （TitleBar / SplashLoading）与新壳 `src/shell/`（ShellToolbar / ShellBootScreen）。
 * 品牌重生（icons/icon.svg 换几何）时只改了其中一套，另一套留在旧 W 标志上，
 * 于是「运行后看到的标志不是应用图标那个」（用户实测反馈）。
 *
 * 本锁把两件事钉住：
 *   L1 每个品牌位都画当前标志几何（与应用图标 icon.svg 逐字同源）
 *   L2 全 `src/**` 不再出现已退役的旧 W 标志笔画（防回接：新加品牌位时抄了旧代码）
 */
import { describe, it, expect } from 'vitest'
import { readFileSync, readdirSync } from 'node:fs'
import { join, relative } from 'node:path'

const SRC_DIR = join(process.cwd(), 'src')

/** 当前标志：与应用图标 `src-tauri/icons/icon.svg` 同一条路径（`>` 提示符 + 光标横线） */
const MARK_PATHS = [
  'M 24 18 L 59 50 L 24 82 L 32 74 L 51 50 L 32 26 Z',
  'M 51 60 L 84 62 L 53 65 Z',
]

/** 已退役的旧 W 标志：四笔画（图标字面量来自旧 icon.svg，改一处漏一处都据此抓出来） */
const RETIRED_GEOMETRY = ['23.2', '36.8', '39.3', '48.7', '51.3', '60.7', '63.2', '76.8']

/** 品牌位的四个渲染点（两套 UI 各自一处标题位 + 一处启动位） */
const BRAND_SOURCES = [
  'src/components/TitleBar.vue',
  'src/components/SplashLoading.vue',
  'src/shell/components/ShellToolbar.vue',
  'src/shell/components/ShellBootScreen.vue',
]

/** 收集 src 下的 .vue 源码（相对路径，POSIX 分隔符） */
function collectVueFiles(dir: string): string[] {
  const out: string[] = []
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name)
    if (entry.isDirectory()) {
      out.push(...collectVueFiles(full))
    } else if (entry.name.endsWith('.vue')) {
      out.push(relative(process.cwd(), full).split('\\').join('/'))
    }
  }
  return out
}

describe('品牌标志几何锁', () => {
  it('L1 每个品牌位都画当前标志几何', () => {
    expect(BRAND_SOURCES.length, '品牌位清单不得为空（否则本锁空转）').toBeGreaterThanOrEqual(4)
    for (const file of BRAND_SOURCES) {
      const source = readFileSync(file, 'utf-8')
      for (const path of MARK_PATHS) {
        expect(source, `${file} 缺少当前标志路径 ${path}`).toContain(path)
      }
    }
  })

  it('L2 全 src 不再出现旧 W 标志笔画', () => {
    const files = collectVueFiles(SRC_DIR)
    // 防「扫描空转 = 全绿」
    expect(files.length, 'src 下 .vue 文件数基线').toBeGreaterThanOrEqual(50)

    const offenders = files.filter((file) =>
      RETIRED_GEOMETRY.some((token) => readFileSync(file, 'utf-8').includes(token)),
    )
    expect(offenders, '旧 W 标志（四笔画 + 琥珀菱形）已退役，出现即为品牌位漏改').toEqual([])
  })
})
