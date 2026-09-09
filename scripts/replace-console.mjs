#!/usr/bin/env node
/**
 * 业务代码 console.* → logger.* 机械化替换（双端）
 *
 * 规则：
 * - 导入 `import { logger } from '@/utils/frontendLogger'`（或相对路径）
 * - console.log/debug/info/warn/error( → logger.log/debug/info/warn/error(
 * - 跳过：测试文件、devConsoleRelay.ts、frontendLogger.ts 自身、已 import logger 的文件
 *
 * 用法：node scripts/replace-console.mjs <desktop|mobile>
 */
import { readFileSync, writeFileSync, readdirSync, statSync } from 'node:fs'
import { join, resolve } from 'node:path'

const target = process.argv[2]
if (target !== 'desktop' && target !== 'mobile') {
  console.error('用法: node scripts/replace-console.mjs <desktop|mobile>')
  process.exit(1)
}

const root = resolve(import.meta.dirname, '..')
const srcDir = join(root, target === 'desktop' ? 'bedcode-desktop/src' : 'bedcode-mobile/src')

function collect(dir, out = []) {
  for (const name of readdirSync(dir)) {
    if (name === 'node_modules' || name === 'dist') continue
    const p = join(dir, name)
    const st = statSync(p)
    if (st.isDirectory()) collect(p, out)
    else if (/\.(ts|vue)$/.test(name) && !/\.test\.|\.spec\./.test(name)) out.push(p)
  }
  return out
}

const LEVELS = ['log', 'debug', 'info', 'warn', 'error']
const re = new RegExp(`\\bconsole\\.(${LEVELS.join('|')})\\(`, 'g')
const importRe = /import\s*\{[^}]*logger[^}]*\}\s*from\s*['"]@\/utils\/frontendLogger['"]/

let changedFiles = 0
let replacedCalls = 0

for (const file of collect(srcDir)) {
  if (file.endsWith('devConsoleRelay.ts') || file.endsWith('frontendLogger.ts')) continue
  let src = readFileSync(file, 'utf8')
  if (importRe.test(src)) continue // 已接入 logger，跳过（幂等）

  const callCount = (src.match(re) || []).length
  if (callCount === 0) continue

  const importLine = `import { logger } from '@/utils/frontendLogger'`

  if (file.endsWith('.ts')) {
    const lines = src.split('\n')
    const firstImport = lines.findIndex((l) => /^\s*import\b/.test(l))
    const insertAt = firstImport !== -1 ? firstImport + 1 : 0
    lines.splice(insertAt, 0, importLine)
    src = lines.join('\n')
  } else {
    const scriptMatch = src.match(/<script[^>]*>([\s\S]*?)<\/script>/)
    if (!scriptMatch) {
      console.warn(`[跳过] ${file}: 未找到 <script> 块`)
      continue
    }
    const block = scriptMatch[1]
    const blockLines = block.split('\n')
    const firstImport = blockLines.findIndex((l) => /^\s*import\b/.test(l))
    if (firstImport !== -1) blockLines.splice(firstImport + 1, 0, importLine)
    else {
      const firstCode = blockLines.findIndex((l) => /^\S/.test(l))
      blockLines.splice(firstCode === -1 ? 0 : firstCode, 0, importLine)
    }
    src = src.replace(scriptMatch[1], blockLines.join('\n'))
  }

  src = src.replace(re, (m, lv) => `logger.${lv}(`)
  writeFileSync(file, src)
  changedFiles++
  replacedCalls += callCount
}

console.log(`完成: ${changedFiles} 文件, ${replacedCalls} 处 console.* → logger.* (${target})`)
