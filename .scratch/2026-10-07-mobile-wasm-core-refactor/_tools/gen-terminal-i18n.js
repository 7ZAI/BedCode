/**
 * Ticket 15 — generate plugin terminal i18n messages from host locales.
 * Scans plugin terminal sources for t('...') key literals, resolves each key to its
 * host locale path, and emits a TS file with nested zh-CN / en message trees.
 *
 * Usage: node gen-terminal-i18n.js [--check]
 */
const fs = require('fs')
const path = require('path')

const ROOT = '/home/binblink/project/tauriProject/BedCode/bedcode-mobile'
const TERMINAL = path.join(ROOT, 'plugins/terminal-session/src/terminal')
const OUT = path.join(TERMINAL, 'i18n.ts')
const CHECK = process.argv.includes('--check')

/** load a locale module (pure object literal with `export default`) */
function loadLocale(rel) {
  const src = fs.readFileSync(path.join(ROOT, rel), 'utf8').replace(/export default/, 'module.exports =')
  const mod = { exports: {} }
  // eslint-disable-next-line no-new-func
  new Function('module', 'exports', src)(mod, mod.exports)
  return mod.exports
}

/** locale 文件顶层为单根键（如 { mobile: {...} }）——取出根值 */
function loadRoot(rel) {
  const o = loadLocale(rel)
  return o[Object.keys(o)[0]]
}

const zh = {
  mobile: loadRoot('src/locales/zh-CN/mobile.ts'),
  desktop: loadRoot('src/locales/zh-CN/desktop.ts'),
  settings: loadRoot('src/locales/zh-CN/settings.ts'),
  common: loadRoot('src/locales/zh-CN/common.ts'),
}
const en = {
  mobile: loadRoot('src/locales/en/mobile.ts'),
  desktop: loadRoot('src/locales/en/desktop.ts'),
  settings: loadRoot('src/locales/en/settings.ts'),
  common: loadRoot('src/locales/en/common.ts'),
}

/** plugin key -> host locale path segments */
function hostPath(key) {
  if (key === 'terminal.titleDesktop') return ['desktop', 'terminal', 'title']
  const seg = key.split('.')
  const head = seg[0]
  if (head === 'terminal' || head === 'terminalHelp' || head === 'input' || head === 'shortcutConfig' ||
      head === 'shortcutHelp' || head === 'toolbox' || head === 'presetTask' || head === 'taskPicker') {
    return ['mobile', ...seg]
  }
  if (key === 'connection.connectFailed') return ['mobile', 'connection', 'connectFailed']
  if (key === 'session.mockName') return ['mobile', 'session', 'mockName']
  if (head === 'theme') return ['settings', 'appearance', ...seg.slice(1)]
  if (head === 'common') return seg
  return null
}

function pick(tree, seg) {
  let cur = tree
  for (const s of seg) {
    if (cur == null || typeof cur !== 'object') return undefined
    cur = cur[s]
  }
  return cur
}

function walk(dir, out = []) {
  for (const name of fs.readdirSync(dir)) {
    const p = path.join(dir, name)
    const st = fs.statSync(p)
    if (st.isDirectory()) walk(p, out)
    else if (/\.(ts|vue)$/.test(name)) out.push(p)
  }
  return out
}

// 1) collect key literals
const keys = new Set()
const KEY_RE = /['"]((?:terminal|terminalHelp|input|shortcutConfig|shortcutHelp|toolbox|presetTask|taskPicker|connection|session|theme|common)\.[A-Za-z0-9_.]+)['"]/g
for (const file of walk(TERMINAL)) {
  const src = fs.readFileSync(file, 'utf8')
  let m
  while ((m = KEY_RE.exec(src))) keys.add(m[1])
}
// 2) dynamic onboarding keys (k(prefix) + onboardingSteps suffixes)
const steps = fs.readFileSync(path.join(TERMINAL, 'config/onboardingSteps.ts'), 'utf8')
for (const m of steps.matchAll(/(?:titleKey|descKey|tryHintKey):\s*'([^']+)'/g)) {
  keys.add(`terminal.${m[1]}`)
}
// theme labels (resolve via themes.ts label fields)
const themes = fs.readFileSync(path.join(TERMINAL, 'config/themes.ts'), 'utf8')
for (const m of themes.matchAll(/label:\s*'([^']+)'/g)) {
  if (m[1].includes('.')) keys.add(m[1])
}

// 3) resolve + build trees
const missing = []
const treeZh = {}
const treeEn = {}
for (const key of [...keys].sort()) {
  const seg = hostPath(key)
  const z = seg && pick(zh, seg)
  const e = seg && pick(en, seg)
  if (typeof z !== 'string' || typeof e !== 'string') {
    missing.push(`${key}  →  ${seg ? seg.join('.') : '(unmapped)'}`)
    continue
  }
  assign(treeZh, key, z)
  assign(treeEn, key, e)
}

function assign(tree, key, value) {
  const seg = key.split('.')
  let cur = tree
  for (let i = 0; i < seg.length - 1; i++) {
    cur[seg[i]] = cur[seg[i]] || {}
    cur = cur[seg[i]]
  }
  cur[seg[seg.length - 1]] = value
}

console.log(`keys: ${keys.size}, resolved: ${keys.size - missing.length}, missing: ${missing.length}`)
if (missing.length) console.log('MISSING:\n  ' + missing.join('\n  '))
if (CHECK) process.exit(0)

const header = `/**
 * 终端域文案（票 15：随终端 UI 域自宿主 locales 迁入）
 *
 * 键为域内相对键（如 'terminal.title'）；activate 时经 context.i18n.registerMessages
 * 注册，取文案一律经 host.t(...)（自动加插件 id 前缀）。
 * 键映射：terminal.* ← mobile.terminal.* · theme.* ← settings.appearance.* ·
 * common.* ← common.* · connection/session/toolbox/presetTask/taskPicker ← mobile.* 同名子树。
 */

/** zh-CN 文案树 */
export const messagesZhCN = ${JSON.stringify(treeZh, null, 2)}

/** en 文案树 */
export const messagesEn = ${JSON.stringify(treeEn, null, 2)}
`
fs.writeFileSync(OUT, header)
console.log(`written: ${OUT}`)
