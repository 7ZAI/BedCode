/**
 * manifest-gen — 桌面端 plugin.json contributes/permissions 自动填充
 *
 * 单一事实来源是插件源码：前端 context.ui.register* 调用推导 views 扩展点，
 * 前端 context API 使用 + Rust host 调用推导权限，Rust invoke_command 匹配臂推导 commands，
 * on_terminal_input/output 实现推导 terminal handlers。
 *
 * 与移动端 manifest-gen 的区别（桌面端扩展点/权限体系不同）：
 * - 扩展点：sidebar/toolbox/statusbar（views）+ fileHandlers（无 navTab/settings/toolbarItems）
 * - 权限：ui:sidebar / ui:toolbox / ui:statusbar / ui:pageToolbar / ui:input / ui:fileHandler
 *   / terminal:observe / session:write / broadcast 等桌面端权限
 *
 * 合并策略（保守，避免误删导致运行时拒绝）：
 * - permissions：派生结果与手工声明取并集
 * - contributes.views/fileHandlers：扫描到注册调用时以扫描结果为准
 *   （按 id 从旧条目继承无法静态求值的字段，如 i18n.t() 动态 title）；未扫描到时保留原值
 * - contributes.commands：以 Rust invoke_command 匹配臂为准，title 从旧条目继承
 * - terminal.inputHandlers/outputParsers：从 Rust 实现检测填充
 * - configuration/lifecycle/icon 等手工字段永不覆盖
 */

import { existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

// ==================== 权限映射表 ====================

/** 前端 UI 注册调用 → 权限（与宿主 permission.ts PERMISSION_API_MAP 一一对应） */
const REGISTER_PERMISSIONS = {
  registerSidebarPanel: 'ui:sidebar',
  registerToolboxPage: 'ui:toolbox',
  registerStatusBarItem: 'ui:statusbar',
  registerTitleBarItem: 'ui:statusbar',
  registerPageToolbarItem: 'ui:pageToolbar',
  registerInputExtension: 'ui:input',
  registerTerminalToolbarItem: 'ui:input',
  registerFileHandler: 'ui:fileHandler',
}

/** 前端 context API 使用 → 权限（正则按合并后的前端源码匹配） */
const FRONTEND_PERMISSION_RULES = [
  { re: /\.storage\s*\.\s*(get|set|delete|flush)\b/, perm: 'storage' },
  { re: /\.terminal\s*\.\s*sendInput\b/, perm: 'terminal:input' },
  { re: /\.terminal\s*\.\s*onInput\b/, perm: 'terminal:input' },
  { re: /\.terminal\s*\.\s*onOutput\b/, perm: 'terminal:output' },
  { re: /\.terminal\s*\.\s*onInputSubmitted\b/, perm: 'terminal:observe' },
  { re: /\.session\s*\.\s*(list|get|onStatusChange)\b/, perm: 'session:read' },
  { re: /\.session\s*\.\s*(create|stop)\b/, perm: 'session:write' },
  { re: /\.http\s*\.\s*registerEndpoint\b/, perm: 'network:http' },
  { re: /\.fileService\s*\.\s*(mount|unmount|updateRoots|getPeer|pickDirectory|pickFiles)\b/, perm: 'fileservice' },
  { re: /\.events\s*\.\s*(on|emit)\b/, perm: 'broadcast' },
]

/** Rust host API 调用 → 权限 */
const RUST_PERMISSION_RULES = [
  { re: /\b(storage_get|storage_set|storage_delete|db_execute|db_query)\b/, perm: 'storage' },
  { re: /\bterminal_send\b/, perm: 'terminal:input' },
  { re: /\b(session_list|session_get)\b/, perm: 'session:read' },
  { re: /\bhttp_fetch\b/, perm: 'network:http' },
  { re: /\b(fs_read|fs_copy)\b/, perm: 'fs:read' },
  { re: /\bfs_write\b/, perm: 'fs:write' },
  { re: /\bplugin_db_\w+\b/, perm: 'storage' },
]

// ==================== 文件收集 ====================

const FRONTEND_EXTS = new Set(['.ts', '.tsx', '.vue', '.js'])

/** 递归收集指定扩展名的文件 */
function collectFiles(dir, exts, out = []) {
  if (!existsSync(dir)) return out
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === 'node_modules' || entry.name === 'dist' || entry.name.startsWith('.')) continue
    const full = join(dir, entry.name)
    if (entry.isDirectory()) {
      collectFiles(full, exts, out)
    } else if (entry.isFile() && exts.some((e) => entry.name.endsWith(e))) {
      out.push(full)
    }
  }
  return out
}

// ==================== 对象字面量解析 ====================

/** 从源码指定下标起提取配对的 {...} 对象字面量文本 */
function extractObjectLiteral(source, fromIndex) {
  const start = source.indexOf('{', fromIndex)
  if (start === -1) return null
  let depth = 0
  let inStr = null
  for (let i = start; i < source.length; i++) {
    const ch = source[i]
    if (inStr) {
      if (ch === '\\') i++
      else if (ch === inStr) inStr = null
      continue
    }
    if (ch === "'" || ch === '"' || ch === '`') inStr = ch
    else if (ch === '{') depth++
    else if (ch === '}') {
      depth--
      if (depth === 0) return source.slice(start, i + 1)
    }
  }
  return null
}

/** 按顶层逗号切分对象字面量内部（忽略嵌套与字符串内的逗号） */
function splitTopLevel(body) {
  const parts = []
  let depth = 0
  let inStr = null
  let cur = ''
  for (let i = 0; i < body.length; i++) {
    const ch = body[i]
    if (inStr) {
      cur += ch
      if (ch === '\\') cur += body[++i] ?? ''
      else if (ch === inStr) inStr = null
      continue
    }
    if (ch === "'" || ch === '"' || ch === '`') { inStr = ch; cur += ch; continue }
    if (ch === '{' || ch === '[' || ch === '(') depth++
    else if (ch === '}' || ch === ']' || ch === ')') depth--
    if (ch === ',' && depth === 0) {
      parts.push(cur)
      cur = ''
    } else {
      cur += ch
    }
  }
  if (cur.trim()) parts.push(cur)
  return parts
}

/** 分类字面量值：字符串/数字/布尔/标识符，动态表达式返回 null */
function classifyValue(raw) {
  const text = raw.trim().replace(/,$/, '').trim()
  const strMatch = text.match(/^(['"])((?:\\.|(?!\1).)*)\1$/s)
  if (strMatch) return strMatch[2]
  if (/^-?\d+(\.\d+)?$/.test(text)) return Number(text)
  if (text === 'true') return true
  if (text === 'false') return false
  if (/^[A-Za-z_$][\w$]*$/.test(text)) return { __ident: text }
  return null
}

/** 解析简单对象字面量为键值映射（嵌套对象/动态值 → null） */
function parseObjectLiteral(literal) {
  const body = literal.slice(literal.indexOf('{') + 1, literal.lastIndexOf('}'))
  const result = {}
  for (const part of splitTopLevel(body)) {
    const m = part.match(/^\s*(?:['"](\w+)['"]|(\w+))\s*:\s*([\s\S]+)$/)
    if (!m) continue
    const key = m[1] || m[2]
    result[key] = classifyValue(m[3])
  }
  return result
}

/** 提取源码中所有 `context.ui.callName({...})` 注册调用的对象参数 */
function findRegisterCalls(source, callName) {
  const results = []
  const re = new RegExp(`\\.${callName}\\s*\\(`, 'g')
  let m
  while ((m = re.exec(source)) !== null) {
    const literal = extractObjectLiteral(source, m.index + m[0].length - 1)
    if (literal) results.push(parseObjectLiteral(literal))
  }
  return results
}

// ==================== 字段合并 ====================

/** 旧条目按 id 建索引 */
function indexById(entries) {
  const map = new Map()
  for (const e of entries || []) {
    if (e && e.id) map.set(e.id, e)
  }
  return map
}

/** 合并扫描条目与旧条目：扫描值优先，null（动态表达式）回退旧值，再回退默认值 */
function mergeEntry(scanned, old, defaults = {}) {
  const merged = { ...defaults }
  if (old) Object.assign(merged, old)
  for (const [key, value] of Object.entries(scanned)) {
    if (value === null || value === undefined) continue
    if (value && typeof value === 'object' && value.__ident) {
      merged[key] = value.__ident
      continue
    }
    merged[key] = value
  }
  return merged
}

// ==================== Rust 扫描 ====================

/** 提取 invoke_command 匹配臂中的 command id 列表（过滤 _ 前缀内置分支） */
function extractRustCommands(rustSource) {
  const fnIndex = rustSource.search(/\bfn\s+invoke_command\b/)
  if (fnIndex === -1) return []
  const body = extractObjectLiteral(rustSource, fnIndex)
  if (!body) return []
  const ids = []
  const re = /"([\w][\w.-]*)"\s*=>/g
  let m
  while ((m = re.exec(body)) !== null) {
    // 下划线前缀为内置控制分支（如 _http_endpoint），不是插件命令
    if (!m[1].startsWith('_')) ids.push(m[1])
  }
  return [...new Set(ids)]
}

/** 检测 rust 源码是否实现了终端处理导出 */
function extractTerminalHandlers(rustSource) {
  const handlers = []
  if (/\bfn\s+on_terminal_input\b/.test(rustSource)) handlers.push('on_terminal_input')
  if (/\bfn\s+on_terminal_output\b/.test(rustSource)) handlers.push('on_terminal_output')
  return handlers
}

// ==================== 主入口 ====================

/**
 * 扫描插件源码并自动填充 plugin.json 的 contributes/permissions
 * @param {string} cwd 插件工程根目录
 * @param {{ check?: boolean }} options check 模式只报告不写入
 * @returns {{ changed: boolean, report: string[] }}
 */
export function generateManifest(cwd, { check = false } = {}) {
  const manifestPath = join(cwd, 'plugin.json')
  if (!existsSync(manifestPath)) {
    throw new Error(`plugin.json 不存在: ${manifestPath}`)
  }
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf-8'))
  const report = []
  const permissions = new Set(manifest.permissions || [])
  const contributes = { ...(manifest.contributes || {}) }

  // ---------- 前端扫描 ----------
  const frontendSource = collectFiles(join(cwd, 'src'), [...FRONTEND_EXTS])
    .map((f) => readFileSync(f, 'utf-8'))
    .join('\n')

  // views：sidebar / toolbox / statusbar 三种类型
  const sidebarPanels = findRegisterCalls(frontendSource, 'registerSidebarPanel')
  const toolboxPages = findRegisterCalls(frontendSource, 'registerToolboxPage')
  const statusBarItems = findRegisterCalls(frontendSource, 'registerStatusBarItem')

  const scannedViews = [
    ...sidebarPanels.map((s) => mergeEntry(s, undefined, { type: 'sidebar' })),
    ...toolboxPages.map((s) => mergeEntry(s, undefined, { type: 'toolbox' })),
    ...statusBarItems.map((s) => mergeEntry(s, undefined, { type: 'statusbar' })),
  ]

  if (scannedViews.length > 0) {
    const old = indexById(contributes.views)
    contributes.views = scannedViews.map((s) => mergeEntry(s, old.get(s.id)))
    report.push(`contributes.views ← ${contributes.views.map((v) => v.id).join(', ')}`)
  }

  // fileHandlers
  const fileHandlers = findRegisterCalls(frontendSource, 'registerFileHandler')
  if (fileHandlers.length > 0) {
    const old = indexById(contributes.fileHandlers)
    contributes.fileHandlers = fileHandlers.map((s) => mergeEntry(s, old.get(s.id)))
    report.push(`contributes.fileHandlers ← ${contributes.fileHandlers.map((v) => v.id).join(', ')}`)
  }

  // 注册调用 → 权限
  for (const [callName, perm] of Object.entries(REGISTER_PERMISSIONS)) {
    if (new RegExp(`\\.${callName}\\s*\\(`).test(frontendSource)) {
      if (!permissions.has(perm)) report.push(`permissions + ${perm}（${callName} 调用）`)
      permissions.add(perm)
    }
  }

  // 前端 context API 使用 → 权限
  for (const rule of FRONTEND_PERMISSION_RULES) {
    if (rule.re.test(frontendSource)) {
      if (!permissions.has(rule.perm)) report.push(`permissions + ${rule.perm}（前端 API 使用）`)
      permissions.add(rule.perm)
    }
  }

  // ---------- Rust 扫描 ----------
  const rustSource = collectFiles(join(cwd, 'rust/src'), ['.rs'])
    .map((f) => readFileSync(f, 'utf-8'))
    .join('\n')

  if (rustSource) {
    for (const rule of RUST_PERMISSION_RULES) {
      if (rule.re.test(rustSource)) {
        if (!permissions.has(rule.perm)) report.push(`permissions + ${rule.perm}（Rust host 调用）`)
        permissions.add(rule.perm)
      }
    }

    const commandIds = extractRustCommands(rustSource)
    if (commandIds.length > 0) {
      const old = indexById(contributes.commands)
      contributes.commands = commandIds.map((id) =>
        mergeEntry({ id }, old.get(id), { id, title: id })
      )
      report.push(`contributes.commands ← ${commandIds.length} 个`)
    }

    const handlers = extractTerminalHandlers(rustSource)
    if (handlers.length > 0) {
      const terminal = { ...(contributes.terminal || {}) }
      const inputHandlers = handlers.filter((h) => h === 'on_terminal_input')
      const outputParsers = handlers.filter((h) => h === 'on_terminal_output')
      if (inputHandlers.length > 0) terminal.inputHandlers = inputHandlers
      if (outputParsers.length > 0) terminal.outputParsers = outputParsers
      contributes.terminal = terminal
      report.push(`contributes.terminal handlers ← ${handlers.join(', ')}`)
    }
  }

  // ---------- 生成结果 ----------
  const generated = {
    ...manifest,
    permissions: [...permissions].sort(),
    contributes,
  }
  const changed = JSON.stringify(generated, null, 2) !== JSON.stringify(manifest, null, 2)

  if (changed && !check) {
    writeFileSync(manifestPath, JSON.stringify(generated, null, 2) + '\n', 'utf-8')
  }

  return { changed, report }
}
