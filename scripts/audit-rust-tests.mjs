#!/usr/bin/env node
// Rust 单元测试审计：度量 src 内联 #[cfg(test)] 块的规模，并按「公共 API 可迁移 / 私有依赖」分类。
//
// 用法：
//   node scripts/audit-rust-tests.mjs            # 人读报告（按 crate 汇总 + 档位明细）
//   node scripts/audit-rust-tests.mjs --json     # 机器可读（供后续批处理）
//   node scripts/audit-rust-tests.mjs --crate <子串>   # 只看某个 crate
//   node scripts/audit-rust-tests.mjs --min-test 200   # 只看 test 行数 >= N 的
//
// 分类是**静态保守判定**（宁可多报私有依赖，也不误报可迁移）：
//   public-api  测试只引用 crate 外可见（`pub` 且路径前缀全 `pub`）的项
//   private     测试引用了任意非 `pub` 项（含 `pub(crate)` / `pub(super)` / 裸私有）
//   unknown     解析不出引用（如宏生成项、跨文件 re-export），必须靠编译定夺
//
// 真源结论以**编译验证**为准：搬进 tests/ 后 `cargo test --no-run` 能过才算公共 API 测试。

import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join, relative, dirname, sep } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const SKIP_DIRS = new Set([
  'target', 'node_modules', '.git', 'gen', '.scratch', '.pi', 'build', 'dist',
])

// ==================== 词法处理 ====================

/** 把源码里的字符串字面量 / 字符字面量 / 行注释 / 块注释替换成等长空白，保住行列与括号配平 */
export function stripNonCode(src) {
  let out = ''
  let i = 0
  const n = src.length
  while (i < n) {
    const ch = src[i]
    const rest = src.slice(i)
    if (rest.startsWith('//')) {
      const nl = src.indexOf('\n', i)
      const end = nl === -1 ? n : nl
      out += ' '.repeat(end - i)
      i = end
      continue
    }
    if (rest.startsWith('/*')) {
      const end = src.indexOf('*/', i + 2)
      const stop = end === -1 ? n : end + 2
      // 保留块注释内部的换行，否则后续按行切分会把行号数错
      out += src.slice(i, stop).replace(/[^\n]/g, ' ')
      i = stop
      continue
    }
    if (ch === '"') {
      let j = i + 1
      while (j < n) {
        if (src[j] === '\\') { j += 2; continue }
        if (src[j] === '"') { j += 1; break }
        if (src[j] === '\n') break
        j += 1
      }
      // 必须保留换行：字符串跨行（`\` 续行）时若把 \n 也抹成空格，
      // split('\n') 的行数就会与原文对不上，结构行与原文行错位。
      out += '"' + src.slice(i + 1, Math.max(i + 1, j - 1)).replace(/[^\n]/g, ' ') + '"'
      i = j
      continue
    }
    if (ch === "'") {
      // 字符字面量 vs 生命周期：'a' / '\n' / '\\' 是字面量，'foo 是生命周期
      const m = /^'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]+\}|.)|[^\\'])'/.exec(rest)
      if (m) {
        out += "'" + m[0].slice(1, -1).replace(/[^\n]/g, ' ') + "'"
        i += m[0].length
        continue
      }
      out += ch
      i += 1
      continue
    }
    out += ch
    i += 1
  }
  return out
}

// ==================== 仓库遍历 ====================

function findCrates(root) {
  const crates = []
  const walk = (dir) => {
    let entries
    try { entries = readdirSync(dir, { withFileTypes: true }) } catch { return }
    for (const e of entries) {
      if (!e.isDirectory()) continue
      if (SKIP_DIRS.has(e.name)) continue
      const p = join(dir, e.name)
      try {
        if (statSync(join(p, 'Cargo.toml')).isFile()) crates.push(p)
      } catch { /* 无 Cargo.toml */ }
      walk(p)
    }
  }
  walk(root)
  return crates
}

function crateFiles(crateDir) {
  const out = []
  const walk = (dir) => {
    let entries
    try { entries = readdirSync(dir, { withFileTypes: true }) } catch { return }
    for (const e of entries) {
      const p = join(dir, e.name)
      if (e.isDirectory()) {
        if (SKIP_DIRS.has(e.name)) continue
        walk(p)
      } else if (e.isFile() && e.name.endsWith('.rs')) {
        out.push(p)
      }
    }
  }
  walk(crateDir)
  return out
}

/** 从 Cargo.toml 抠出 lib 名（集成测试里的 `use <libname>::` 要靠它） */
function readLibName(crateDir) {
  let toml = ''
  try { toml = readFileSync(join(crateDir, 'Cargo.toml'), 'utf8') } catch { return null }
  const sec = /^\s*\[lib\]\s*$([\s\S]*?)(?=^\s*\[|\z)/m.exec(toml)
  const body = sec ? sec[1] : toml
  const name = /^\s*name\s*=\s*"([^"]+)"/m.exec(body)
  return name ? name[1] : null
}

// ==================== 可见性解析 ====================

const RE_CFG_TEST = /^\s*#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]/
const RE_ATTR = /^\s*#/
const RE_MOD_DECL = /^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*([;{])/
const RE_USE = /^\s*(?:pub(?:\([^)]*\))?\s+)?use\s+([^;]+);/
const RE_ITEM = new RegExp(
  '^\\s*(?:pub(?:\\(([^)]*)\\))?\\s+)?' + // 可见性
  '(?:default\\s+)?' +
  '(?:const\\s+)?(?:async\\s+)?(?:unsafe\\s+)?' +
  '(?:' +
    'fn|struct|enum|trait|type|static|union|macro_rules' +
  ')\\s+(\\w+)',
)

/** 归一化可见性：pub=0（外部可见）/ pub(crate),pub(super),pub(in ..)=1（crate 内可见）/ 私有=2 */
function visLevel(vis) {
  if (!vis) return 2
  if (vis === 'pub') return 0
  return 1
}

/**
 * 扫描一个文件的非测试部分，产出「模块作用域 items → 可见性」以及子模块声明。
 * 这是分类器的唯一事实源：测试块能引用到的父作用域名字全在这里。
 */
export function parseScope(codeLines) {
  const items = new Map() // name -> { level, kind }
  const childMods = [] // { name, level, braceDepth, pathAttr }
  // 私有成员名（struct 私有字段 + impl 内非 pub fn）——集成测试同样看不见，
  // 只扫模块作用域可见性会把「访问 .db 这类私有字段」的块误判成可迁移。
  const privMembers = new Map() // memberName -> 声明处简述
  let depth = 0
  let pendingAttrs = []
  let pendingPath = null
  let inImpl = false
  let inStruct = false
  for (const line of codeLines) {
    const trimmed = line.trim()
    if (trimmed.startsWith('#[')) {
      const p = /path\s*=\s*"([^"]+)"/.exec(trimmed)
      if (p) pendingPath = p[1]
      pendingAttrs.push(trimmed)
      depth += (line.match(/\{/g) || []).length - (line.match(/\}/g) || []).length
      continue
    }
    // mod 声明
    const modM = RE_MOD_DECL.exec(line)
    if (modM && depth === 0) {
      const [, name, kind] = modM
      const pubM = /^\s*pub\s*(?:\(([^)]*)\))?/.exec(line)
      const level = pubM ? visLevel(pubM[1] ?? 'pub') : 2
      const isTest = pendingAttrs.some((a) => RE_CFG_TEST.test(a))
      if (kind === '{') {
        childMods.push({ name, level, isTest, startDepth: depth, pathAttr: pendingPath })
      } else {
        items.set(name, { level, kind: 'mod' })
      }
      pendingAttrs = []
      pendingPath = null
      depth += (line.match(/\{/g) || []).length - (line.match(/\}/g) || []).length
      continue
    }
    const itemM = RE_ITEM.exec(line)
    if (itemM) {
      const pubM = /^\s*pub\s*(?:\(([^)]*)\))?/.exec(line)
      const level = visLevel(pubM ? pubM[1] ?? 'pub' : null)
      if (depth === 0) items.set(itemM[2], { level, kind: 'item' })
      // impl X { fn priv() } 的非 pub fn
      if (inImpl && itemM[1] === 'fn' && level !== 0 && !privMembers.has(itemM[2])) {
        privMembers.set(itemM[2], 'impl 内非 pub fn')
      }
      if (depth === 1 && inStruct && itemM[1] === 'fn') inStruct = false
    }
    // struct X { 字段 }：depth 1 上的裸标识符 = 字段
    if (inStruct && depth === 1) {
      const f = /^\s*([A-Za-z_]\w*)\s*:/.exec(line)
      const fPub = /^\s*pub\b/.test(line)
      if (f && !fPub && !privMembers.has(f[1])) privMembers.set(f[1], 'struct 私有字段')
    }
    if (/^\s*impl\b/.test(line) && line.includes('{')) inImpl = true
    if (/^\s*(?:pub(?:\([^)]*\))?\s+)?(?:struct|union)\s+\w+[^{;]*\{/.test(line)) inStruct = true
    if (trimmed.startsWith('#[cfg(')) pendingAttrs.push(trimmed)
    if (trimmed !== '' && !trimmed.startsWith('//')) pendingAttrs = []
    const d0 = depth
    depth += (line.match(/\{/g) || []).length - (line.match(/\}/g) || []).length
    if (d0 === 0 && depth === 0 && (trimmed.startsWith('}'))) { inImpl = false; inStruct = false }
  }
  return { items, childMods, privMembers }
}

/** 抽取测试块里「对父作用域的名字引用」：glob 导入 + 显式路径根 + 裸标识符 */
export function collectRefs(testLines, privMembers = new Map()) {
  const globSuper = testLines.some((l) => /^\s*use\s+super\s*::\s*\*\s*;/.test(l))
  const roots = new Set() // 裸路径根 / super::x 的 x
  const cratePaths = new Set() // crate::a::b
  const superNames = new Set()
  const localDefs = new Set() // 测试块内自己定义的东西（搬到外部也要能自洽）
  for (const line of testLines) {
    const useM = RE_USE.exec(line)
    if (useM) {
      const body = useM[1]
      for (const seg of body.split(/[,{]/)) {
        const t = seg.trim().replace(/^\s*(?:pub\s*)?use\s+/, '')
        if (!t || t === '*') continue
        if (t.startsWith('super::')) { superNames.add(t.slice(7).split('::')[0]); continue }
        if (t.startsWith('self::')) { roots.add(t.slice(6).split('::')[0]); continue }
        if (t.startsWith('crate::')) { cratePaths.add(t); continue }
        roots.add(t.split('::')[0])
      }
    }
    // 测试块内自有的定义
    const defM = RE_ITEM.exec(line)
    if (defM) localDefs.add(defM[2])
    const innerModM = RE_MOD_DECL.exec(line)
    if (innerModM) localDefs.add(innerModM[1])
    // super::foo::bar / crate::a::b::C
    for (const m of line.matchAll(/\bsuper::(\w+)/g)) superNames.add(m[1])
    for (const m of line.matchAll(/\bcrate::([\w:]+)/g)) {
      const segs = m[1].split('::').filter(Boolean)
      if (segs.length) cratePaths.add('crate::' + segs.join('::'))
    }
    // 裸调用/类型引用：取所有标识符，交给上层与 items 求交集
    for (const m of line.matchAll(/\b([A-Za-z_][A-Za-z0-9_]*)\b/g)) roots.add(m[1])
    // `.field` / `.method` 访问：命中本文件私有成员名即说明越了 crate 边界
    for (const m of line.matchAll(/\.\s*([A-Za-z_]\w*)/g)) {
      if (privMembers.has(m[1])) roots.add(m[1])
    }
  }
  for (const s of [...superNames]) roots.add(s)
  return { globSuper, roots, superNames, cratePaths, localDefs }
}

// ==================== 测试块抽取 ====================

/**
 * 找出文件里所有 `#[cfg(test)] mod X { ... }`（以及 `#[cfg(test)] #[path=..] mod X;`），
 * 返回每个块的行区间与体。
 */
export function findTestBlocks(src) {
  const code = stripNonCode(src)
  const lines = code.split('\n')
  const rawLines = src.split('\n')
  const blocks = []
  let i = 0
  while (i < lines.length) {
    if (!RE_CFG_TEST.test(lines[i])) { i += 1; continue }
    let j = i
    let pathAttr = null
    while (j < lines.length && (RE_ATTR.test(lines[j]) || lines[j].trim() === '')) {
      const p = /path\s*=\s*"([^"]+)"/.exec(lines[j])
      if (p) pathAttr = p[1]
      j += 1
    }
    const modM = RE_MOD_DECL.exec(lines[j] ?? '')
    if (!modM) { i = j > i ? j : i + 1; continue }
    const [, name, kind] = modM
    if (kind === ';') {
      // #[path] 挂载形态：测试体在另一个文件里
      blocks.push({ name, kind: 'path', startLine: i, endLine: j, body: [], rawBody: [], pathAttr })
      i = j + 1
      continue
    }
    let depth = 0
    let started = false
    let end = lines.length - 1
    for (let k = j; k < lines.length; k += 1) {
      depth += (lines[k].match(/\{/g) || []).length - (lines[k].match(/\}/g) || []).length
      if (lines[k].includes('{')) started = true
      if (started && depth <= 0) { end = k; break }
    }
    // 括号配平可能因未剥离的字符串/宏提前归零——顶层模块的收尾是「整行只有 }」（零缩进），
    // 内部闭括号都有缩进，所以只看顶格闭合行就能修正提前归零。
    while (end + 1 < rawLines.length && rawLines[end] !== '}') end += 1
    blocks.push({
      name,
      kind: 'inline',
      startLine: i,
      endLine: end,
      body: lines.slice(j + 1, end),
      rawBody: rawLines.slice(j + 1, end),
    })
    i = end + 1
  }
  return { blocks, totalLines: lines.length }
}

// ==================== crate 可见性索引（跨文件） ====================

/** 建 crate 级模块树：模块路径 → { file, items }，供 crate::a::b 的解析 */
export function buildModuleIndex(_crateDir, files) {
  const index = new Map() // modulePath -> { file, items }
  // lib.rs / main.rs 视为 crate 根
  const root = files.find((f) => /(^|\/)(lib|main)\.rs$/.test(f)) ?? files[0]
  if (!root) return index
  const read = (file) => {
    const src = readFileSync(file, 'utf8')
    const code = stripNonCode(src)
    const lines = code.split('\n')
    // 去掉测试块后的非测试行
    const { blocks } = findTestBlocks(src)
    const drop = new Set()
    for (const b of blocks) for (let l = b.startLine; l <= b.endLine; l += 1) drop.add(l)
    const prod = lines.filter((_, idx) => !drop.has(idx))
    return { src, lines, prod, blocks, drop, items: parseScope(prod).items }
  }
  const rootInfo = read(root)
  index.set('', { file: root, ...rootInfo })
  const walk = (modPrefix, dir) => {
    let entries
    try { entries = readdirSync(dir, { withFileTypes: true }) } catch { return }
    for (const e of entries) {
      if (e.isDirectory()) {
        if (SKIP_DIRS.has(e.name) || e.name === 'tests') continue
        walk(modPrefix, join(dir, e.name))
        continue
      }
      if (!e.isFile() || !e.name.endsWith('.rs')) continue
      const f = join(dir, e.name)
      if (f === root) continue
      const stem = e.name.replace(/\.rs$/, '')
      const modPath = modPrefix ? `${modPrefix}::${stem}` : stem
      const info = read(f)
      index.set(modPath, { file: f, ...info })
      // 目录模块：foo.rs + foo/ 存在时，foo 的子模块在 foo/ 下
      const sub = join(dir, stem)
      try { if (statSync(sub).isDirectory()) walk(modPath, sub) } catch { /* 无子目录 */ }
    }
  }
  walk('', join(dirname(root), ''))
  return index
}

/** 判定一个 `crate::a::b::C` 路径是否全程 pub 可达 */
function cratePathPublic(index, path) {
  const segs = path.split('::').filter((s) => s && s !== 'crate')
  let cur = ''
  for (let i = 0; i < segs.length; i += 1) {
    const seg = segs[i]
    const node = index.get(cur)
    const next = cur ? `${cur}::${seg}` : seg
    const child = index.get(next)
    if (i === 0) {
      if (child) return { ok: false, reason: `crate::${seg} 非 pub mod` }
      if (node && node.items.has(seg)) {
        const it = node.items.get(seg)
        if (it.level !== 0) return { ok: false, reason: `crate::${seg} 可见性非 pub` }
      } else if (!seg.startsWith('crate')) {
        // 可能来自 #[path] / 宏 / 未收录，退回 unknown
        return { unknown: true, reason: `crate::${seg} 无法解析` }
      }
    } else if (!child) {
      const parent = index.get(cur)
      if (parent && parent.items.has(seg)) {
        const it = parent.items.get(seg)
        if (it.level !== 0 && i === segs.length - 1) {
          return { ok: false, reason: `${path} 末级可见性非 pub` }
        }
        if (it.level !== 0) return { ok: false, reason: `${path} 中间级可见性非 pub` }
      } else {
        return { unknown: true, reason: `${path} 无法解析` }
      }
    }
    cur = next
  }
  return { ok: true }
}
export function auditAll(opts = {}) {
  const { crateFilter = null, minTest = 0 } = opts
  const out = []
  for (const crateDir of findCrates(ROOT)) {
    const crateRel = relative(ROOT, crateDir)
    if (crateFilter && !crateRel.includes(crateFilter)) continue
    const files = crateFiles(crateDir)
    const libName = readLibName(crateDir)
    const index = buildModuleIndex(crateDir, files)
    for (const file of files) {
      if (file.includes(`${sep}tests${sep}`)) continue // 集成测试自身不计入
      let src
      try { src = readFileSync(file, 'utf8') } catch { continue }
      const { blocks, totalLines } = findTestBlocks(src)
      const code = stripNonCode(src)
      const lines = code.split('\n')
      const drop = new Set()
      for (const b of blocks) for (let l = b.startLine; l <= b.endLine; l += 1) drop.add(l)
      const scope = parseScope(lines.filter((_, idx) => !drop.has(idx)))
      const rel = relative(ROOT, file)
      for (const b of blocks) {
        const isMounted = b.kind === 'path'
        let testLines = b.endLine - b.startLine + 1
        let body = b.rawBody ?? b.body
        if (isMounted && b.pathAttr) {
          const target = join(dirname(file), b.pathAttr)
          try {
            const t = stripNonCode(readFileSync(target, 'utf8')).split('\n')
            testLines = t.length
            body = readFileSync(target, 'utf8').split('\n')
          } catch { /* 挂载目标缺失 */ }
        }
        if (testLines < minTest) continue
        // 分类
        let verdict = 'public-api'
        const reasons = []
        if (!isMounted) {
          const { roots, cratePaths, localDefs } = collectRefs(body, scope.privMembers)
          const privNames = []
          const unknownRefs = []
          for (const [name, info] of scope.items) {
            if (localDefs.has(name)) continue
            if (!roots.has(name)) continue
            if (info.level !== 0) privNames.push(`${name}(${visWord(info.level)})`)
          }
          for (const p of cratePaths) {
            const r = cratePathPublic(index, p)
            if (r.ok === false) privNames.push(p)
            else if (r.unknown) unknownRefs.push(`${p}: ${r.reason}`)
          }
          // 显式 `super::x` 直接按 scope 判
          for (const name of roots) {
            const info = scope.items.get(name)
            const pm = scope.privMembers.get(name)
            if (info && info.level !== 0 && !localDefs.has(name)) {
              if (!privNames.some((p) => p.startsWith(name))) privNames.push(`${name}(${visWord(info.level)})`)
            } else if (pm && !localDefs.has(name)) {
              if (!privNames.some((p) => p.startsWith(name))) privNames.push(`.${name}(${pm})`)
            }
          }
          if (privNames.length) {
            verdict = 'private'
            reasons.push(`私有依赖 ${privNames.slice(0, 6).join(', ')}${privNames.length > 6 ? ` …共 ${privNames.length}` : ''}`)
          } else if (unknownRefs.length) {
            verdict = 'unknown'
            reasons.push(`跨文件引用待编译定夺：${unknownRefs.slice(0, 3).join('; ')}`)
          }
        } else {
          verdict = 'private'
          reasons.push(`已挂载于 ${b.pathAttr}（保留 crate 内可见性）`)
        }
        const prodLines = totalLines - (isMounted ? b.endLine - b.startLine + 1 : testLines)
        out.push({
          crate: crateRel,
          lib: libName,
          file: rel,
          module: b.name,
          form: isMounted ? 'path-mounted' : 'inline',
          testLines,
          prodLines,
          verdict,
          reason: reasons.join(' | '),
        })
      }
    }
  }
  return out
}

// 仅在直接以脚本方式运行时产出报告；被 split-rust-tests.mjs 等 import 时只导出函数
const argv = process.argv.slice(2)
const crateFilterIdx = argv.indexOf('--crate')
const crateFilter = crateFilterIdx >= 0 ? argv[crateFilterIdx + 1] : null
const minIdx = argv.indexOf('--min-test')
const minTest = minIdx >= 0 ? Number(argv[minIdx + 1]) : 0

function visWord(level) {
  return level === 1 ? 'pub(crate)/pub(super)' : 'private'
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  const report = auditAll({ crateFilter, minTest })
  const asJson = argv.includes('--json')

if (asJson) {
  console.log(JSON.stringify({ root: ROOT, report }, null, 2))
  process.exit(0)
}

// ==================== 人读报告 ====================

const byCrate = new Map()
for (const r of report) {
  const k = r.crate
  const e = byCrate.get(k) ?? { public: 0, private: 0, unknown: 0, pubLines: 0, privLines: 0, unkLines: 0, prod: 0, files: new Set() }
  e[r.verdict === 'public-api' ? 'public' : r.verdict === 'private' ? 'private' : 'unknown'] += 1
  const lineKey = r.verdict === 'public-api' ? 'pubLines' : r.verdict === 'private' ? 'privLines' : 'unkLines'
  e[lineKey] += r.testLines
  e.prod += r.prodLines
  e.files.add(r.file)
  byCrate.set(k, e)
}

const pad = (s, n) => String(s).padEnd(n)
const padL = (s, n) => String(s).padStart(n)
console.log('\n# Rust 单元测试审计')
console.log(`扫描根：${ROOT}\n`)
console.log(`${pad('crate', 58)}${padL('块', 4)}${padL('生产行', 8)}${padL('测试行', 8)}  分类（块数 / 行数）`)
console.log('-'.repeat(118))
const tot = { public: 0, private: 0, unknown: 0, pubLines: 0, privLines: 0, unkLines: 0, prod: 0 }
for (const [k, e] of [...byCrate].sort((a, b) => (b[1].pubLines + b[1].privLines + b[1].unkLines) - (a[1].pubLines + a[1].privLines + a[1].unkLines))) {
  console.log(
    pad(k, 58) +
    padL(e.public + e.private + e.unknown, 4) +
    padL(e.prod, 8) +
    padL(e.pubLines + e.privLines + e.unkLines, 8) +
    `   公共 ${e.public}/${e.pubLines}  私有 ${e.private}/${e.privLines}  待定 ${e.unknown}/${e.unkLines}`,
  )
  tot.public += e.public; tot.private += e.private; tot.unknown += e.unknown
  tot.pubLines += e.pubLines; tot.privLines += e.privLines; tot.unkLines += e.unkLines; tot.prod += e.prod
}
console.log('-'.repeat(118))
console.log(
  pad('TOTAL', 58) + padL(tot.public + tot.private + tot.unknown, 4) + padL(tot.prod, 8) +
  padL(tot.pubLines + tot.privLines + tot.unkLines, 8) +
  `   公共 ${tot.public}/${tot.pubLines}  私有 ${tot.private}/${tot.privLines}  待定 ${tot.unknown}/${tot.unkLines}`,
)

const tiers = [
  { title: '档位 A：内联测试 ≥ 600 行（拆分收益最大）', min: 600 },
  { title: '档位 B：测试行数 > 生产行数（源文件已被测试淹没）', min: 1, ratio: true },
]
for (const t of tiers) {
  console.log(`\n## ${t.title}`)
  const rows = report.filter((r) => r.testLines >= t.min && (t.ratio ? r.testLines > r.prodLines : true))
    .sort((a, b) => b.testLines - a.testLines)
  for (const r of rows) {
    const ratio = r.prodLines ? Math.round((r.testLines / r.prodLines) * 100) : 9999
    console.log(
      `${padL(r.testLines, 5)}/${padL(r.prodLines, 5)} (${padL(`${ratio}%`, 5)})  ${pad(r.verdict, 10)} ${r.file}`,
    )
  }
}
console.log()}
