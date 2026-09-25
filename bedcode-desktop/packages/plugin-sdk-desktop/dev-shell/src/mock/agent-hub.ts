/**
 * Agent Hub 插件领域 mock（dev-shell 专用，纯通用接线）
 *
 * 浏览器中 Rust WASM 后端不可用，dev-shell 的 commands.execute 只执行前端注册的
 * handler。本模块注册 agent-hub 五个领域（探测/安装/Skills/供应商/使用统计）
 * 的命令 handler 骨架，并模拟事件回流（`plugin:agent-hub:*` 全量状态推送），
 * 使插件在 dev-shell 中展示「有数据」的完整形态：
 * - 探测：detect 逐 CLI 延时推进 detecting → 终态（种子为终态目标）
 * - 安装：run 剧本逐行回显输出、测速/检查更新延时出结果、换源改写 npmrc 状态
 * - Skills：扫描动画、编辑保存（hash 重算 → 分发转 stale）、分发/GitHub 安装/本地导入
 * - 供应商：预设 CRUD、反向导入（种子发现条目）、应用（claude 桥接冲突演示）
 * - 使用统计：扫描 syncing → ok、看板聚合、会话分页、日志详情读取
 *
 * 本模块不包含任何具体业务 mock 数据：全部演示种子由插件工程持有（入口导出
 * devMock：detection / install / skills / providers / usage 五个子域），
 * 注入时按 pluginId 经 getDevMock 取种子驱动命令返回值与事件；未导出对应
 * 子域的插件不受影响（各子域命令不注册）。
 */
import { getDevMock } from '../registry'
import type { PluginContext } from '../../../src/types'

// ==================== 种子派生状态（最小本地形状，插件自有扩展字段见插件 devMockTypes） ====================

interface DetectCliInfo {
  installed: boolean | null
  version: string | null
  method: string
  paths: string[]
  dual: boolean
  status: string
  error: string | null
}

interface InstallRunScript {
  command: string
  output: string[]
}

interface SkillEntry {
  dir: string
  path: string
  name: string
  description: string | null
  allowedTools: string | null
  files: Array<{ path: string; hash: string | null }>
  hash: string
  error: string | null
  distribution: Record<'claude' | 'pi', { status: string; missingFiles: string[]; staleFiles: string[] }>
}

interface ProviderPreset {
  id: number
  name: string
  baseUrl: string
  apiStyle: string
  models: string[]
  notes: string | null
  createdAt: number
  updatedAt: number
}

interface UsageSessionRow {
  id: number
  adapter: string
  [key: string]: unknown
}

// ==================== 领域种子状态（最小本地形状） ====================
// 已知字段具名化（mock 内类型安全），未知种子字段经 `[key: string]: unknown`
// 索引透传——形状真源在插件 `types.ts` / `devMockTypes.ts`，dev-shell 不收录插件包

/** 宽松透传基底（替代 Record<string, any>：既有属性有契约，未知字段不逃逸） */
interface Seed {
  [key: string]: unknown
}

/** 探测域（AgentHubState 子集） */
interface DetectionSeed extends Seed {
  envStatus: string
  env: unknown
  authGranted?: boolean
  clis: Record<string, DetectCliInfo>
}

/** npm 镜像域（InstallDomainState.mirror 子集） */
interface MirrorSeed extends Seed {
  speed: { status: string }
  npmrc: {
    backupExists?: boolean
    fileRegistry?: string | null
    appliedAt?: number
    restoredAt?: number
  }
}

/** 运行中安装（ActiveRun 子集） */
interface ActiveRunSeed extends Seed {
  runId: string
  cli: string
  action: 'install' | 'update'
  command: string
  startedAt: number
  cancelRequested: boolean
}

/** 安装域（InstallDomainState 子集 + 剧本扩展） */
interface InstallSeed extends Seed {
  runScripts?: Partial<Record<string, InstallRunScript>>
  mirror: MirrorSeed
  active: ActiveRunSeed | null
  last: Seed | null
  updates: Record<string, { outdated: boolean | null; checkedAt?: number } | null>
}

/** Skills 域（SkillsDomainState 子集 + 编辑器扩展） */
interface SkillsSeed extends Seed {
  skillContents?: Record<string, string>
  skills: SkillEntry[]
  status?: string
  scannedAt?: number | null
  libraryRoot?: string
  localImport?: { path: string; name: string }
  import?: { last: Record<string, unknown> | null }
  github?: { last: Record<string, unknown> | null }
}

/** Claude 桥接视图（ProvidersDomainState 子集） */
interface ClaudeSeed extends Seed {
  bridge: { providerConfigSh?: boolean }
  env: { authTokenMask: string | null; baseUrl: string; model: string | null }
}

/** 供应商域（ProvidersDomainState 子集 + 导入/应用扩展） */
interface ProvidersSeed extends Seed {
  presets: ProviderPreset[]
  claude: ClaudeSeed
  importDiscoveries?: Array<{
    name: string
    baseUrl: string
    apiStyle: string
    models: string[]
    notes: string | null
  }>
  importKeys?: Record<string, string>
  applyFiles?: Record<string, string[]>
  applyBridges?: string[]
  import?: { last: Record<string, unknown> | null }
  apply?: { last: Record<string, unknown> | null }
}

/** 使用统计域状态（UsageDomainState 子集：sources/adapters/home 为自由域） */
interface UsageStateSeed extends Seed {
  status?: string
  syncedAt?: number
  home?: string
  sources?: Array<{ name: string; path: string; builtin?: boolean; [key: string]: unknown }>
  adapters?: Record<string, unknown>
}

/** 使用统计容器（usage devMock 形状） */
interface UsageSeed {
  state: UsageStateSeed
  stats: Record<string, unknown>
  sessions: UsageSessionRow[]
  sessionDetails?: Record<string, Record<string, unknown>>
}

let detection: DetectionSeed | null = null
let install: InstallSeed | null = null
let skills: SkillsSeed | null = null
let providers: ProvidersSeed | null = null
let usage: UsageSeed | null = null

/** 深拷贝（种子 → 工作状态 / 返回值出站，避免外部改动写回种子） */
function clone<T>(v: T): T {
  // 种子为受控 JSON 数据基；structuredClone 失败（函数/类实例等不可克隆值）时
  // 回退引用拷贝（外部只读使用，不写回）
  try {
    return structuredClone(v)
  } catch {
    return v
  }
}

/** FNV-1a 32bit hex（guest skills.rs 同款逐文件 hash 算法） */
function fnv1aHex(text: string): string {
  let h = 0x811c9dc5
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i)
    h = Math.imul(h, 0x01000193)
  }
  return (h >>> 0).toString(16).padStart(8, '0')
}

/** key 掩码（与 guest 掩码形态一致：前 5 位 + *** + 尾 4 位） */
function maskKey(key: string): string {
  if (key.length <= 9) return 'sk-****'
  return `${key.slice(0, 5)}***${key.slice(-4)}`
}

const NPMMIRROR = 'https://registry.npmmirror.com'
const NPMJS = 'https://registry.npmjs.org'

// ==================== 运行中安装模拟（单 run） ====================

let runTimer: ReturnType<typeof setInterval> | null = null
let runState: {
  runId: string
  cli: string
  action: 'install' | 'update'
  command: string
  useMirror: boolean
  startedAt: number
  cancelRequested: boolean
  lines: string[]
  cursor: number
} | null = null
let runSeq = 0

function stopRunTimer(): void {
  if (runTimer !== null) {
    clearInterval(runTimer)
    runTimer = null
  }
}

/** run 模拟：每 tick 回放一行剧本输出，播完置终态并推送（返回待清理句柄）
 * @returns 新 run 的 runId（脚本缺失 = 启动失败时返回 null） */
function startRun(context: PluginContext, cli: string, useMirror: boolean): string | null {
  const script = install?.runScripts?.[cli]
  if (!script) return null
  const updates = (install?.updates ?? {}) as Record<string, { outdated: boolean | null }>
  runState = {
    runId: `mock-run-${++runSeq}`,
    cli,
    action: updates[cli]?.outdated ? 'update' : 'install',
    command: useMirror ? `${script.command} --registry=${NPMMIRROR}` : script.command,
    useMirror,
    startedAt: Date.now(),
    cancelRequested: false,
    lines: [...script.output],
    cursor: 0,
  }
  install!.active = {
    runId: runState.runId,
    cli: runState.cli,
    action: runState.action,
    command: runState.command,
    useMirror,
    startedAt: runState.startedAt,
    cancelRequested: false,
  } as any
  emitInstall(context)

  runTimer = setInterval(() => {
    if (!runState || !install) return
    runState.cursor++
    const done = runState.cursor >= runState.lines.length
    const output = runState.lines.slice(0, runState.cursor).join('\n')
    if (done) {
      install.last = {
        cli: runState.cli,
        action: runState.action,
        command: runState.command,
        ok: true,
        cancelled: false,
        exitCode: 0,
        timedOut: false,
        error: null,
        output,
        finishedAt: Date.now(),
      } as any
      install.active = null
      runState = null
      stopRunTimer()
    } else if (install.active) {
      install.active.cancelRequested = runState.cancelRequested
    }
    emitInstall(context)
  }, 700)
  return runState.runId
}

function cancelRun(context: PluginContext): void {
  if (!runState || !install) return
  stopRunTimer()
  install.last = {
    cli: runState.cli,
    action: runState.action,
    command: runState.command,
    ok: false,
    cancelled: true,
    exitCode: null,
    timedOut: false,
    error: null,
    output: runState.lines.slice(0, runState.cursor).join('\n') + '\n^C cancelled',
    finishedAt: Date.now(),
  } as any
  install.active = null
  runState = null
  emitInstall(context)
}

// ==================== 事件推送（全量状态，与 guest emit 同形） ====================

function emitDetection(context: PluginContext): void {
  if (detection) context.events.emit('plugin:agent-hub:detection', clone(detection))
}
function emitInstall(context: PluginContext): void {
  if (install) {
    const { runScripts: _scripts, ...state } = install
    context.events.emit('plugin:agent-hub:install', clone(state))
  }
}
function emitSkills(context: PluginContext): void {
  if (skills) {
    const { skillContents: _c, localImport: _l, ...state } = skills
    context.events.emit('plugin:agent-hub:skills', clone(state))
  }
}
function emitProviders(context: PluginContext): void {
  if (providers) {
    const {
      importDiscoveries: _d,
      importKeys: _k,
      applyFiles: _f,
      applyBridges: _b,
      ...state
    } = providers
    context.events.emit('plugin:agent-hub:providers', clone(state))
  }
}
function emitUsage(context: PluginContext): void {
  if (usage) context.events.emit('plugin:agent-hub:usage', clone(usage.state))
}

// ==================== 命令 handler ====================

function registerCommands(context: PluginContext): void {
  // ==================== 探测域 ====================
  if (detection) {
    context.commands.register('agent-hub.get-state', () => ({ state: clone(detection) }))
    // 探测动画：env + 全 CLI 置 detecting → 每 500ms 逐 CLI 落终态 → env 终态
    context.commands.register('agent-hub.detect', () => {
      // 早退守卫：模块级 detection 在闭包内 TS 收窄失效（setTimeout 里还有
      // `if (!detection) return` 双保险）；种子缺失时返回错误而非崩溃
      if (!detection) return { ok: false, error: 'detection seed unavailable' }
      const final = clone(detection)
      const detecting = clone(final)
      detecting.envStatus = 'detecting'
      for (const k of Object.keys(detecting.clis)) detecting.clis[k] = { ...detecting.clis[k], status: 'detecting' }
      detection = detecting
      emitDetection(context)
      const cliIds = Object.keys(final.clis)
      cliIds.forEach((id, i) => {
        timers.push(
          setTimeout(() => {
            if (!detection) return
            detection.clis[id] = final.clis[id]
            emitDetection(context)
          }, 500 + i * 500),
        )
      })
      timers.push(
        setTimeout(() => {
          if (!detection) return
          detection.envStatus = final.envStatus
          detection.env = final.env
          emitDetection(context)
        }, 600 + cliIds.length * 500),
      )
      return { ok: true }
    })
    context.commands.register('agent-hub.request-auth', () => {
      detection!.authGranted = true
      emitDetection(context)
      return { ok: true }
    })
  }

  // ==================== 安装域 ====================
  if (install) {
    context.commands.register('agent-hub.get-install-state', () => {
      const { runScripts: _scripts, ...state } = install!
      return { state: clone(state) }
    })
    context.commands.register('agent-hub.get-run-output', () => {
      if (!runState) return { status: 'idle', output: null }
      return {
        status: 'running',
        cli: runState.cli,
        action: runState.action,
        command: runState.command,
        output: runState.lines.slice(0, runState.cursor).join('\n'),
      }
    })
    context.commands.register('agent-hub.describe-install', (args: any) => {
      const script = install?.runScripts?.[args?.cli]
      if (!script) return Promise.reject(new Error(`no install recipe for cli: ${args?.cli}`))
      return {
        command: args?.mirror ? `${script.command} --registry=${NPMMIRROR}` : script.command,
      }
    })
    context.commands.register('agent-hub.speed-test', () => {
      const seedSpeed = clone(install!.mirror.speed)
      seedSpeed.status = 'testing'
      install!.mirror.speed = seedSpeed
      emitInstall(context)
      return new Promise((resolve) => {
        timers.push(
          setTimeout(() => {
            if (!install) return
            install.mirror.speed = { ...clone(install.mirror.speed), status: 'ok' }
            emitInstall(context)
            resolve({ ok: true })
          }, 1400),
        )
      })
    })
    context.commands.register('agent-hub.apply-mirror', (args: any) => {
      install!.mirror.npmrc = {
        ...install!.mirror.npmrc,
        backupExists: true,
        fileRegistry: args?.target === 'npmjs' ? NPMJS : NPMMIRROR,
        appliedAt: Date.now(),
      }
      emitInstall(context)
      return { ok: true }
    })
    context.commands.register('agent-hub.restore-npmrc', () => {
      install!.mirror.npmrc = {
        ...install!.mirror.npmrc,
        backupExists: false,
        fileRegistry: null,
        restoredAt: Date.now(),
      }
      emitInstall(context)
      return { ok: true }
    })
    context.commands.register('agent-hub.check-updates', () => {
      return new Promise((resolve) => {
        timers.push(
          setTimeout(() => {
            if (!install) return
            // checkedAt 刷新为本次检查时间（outdated 判定沿用种子）
            for (const k of Object.keys(install.updates)) {
              if (install.updates[k]) install.updates[k].checkedAt = Date.now()
            }
            emitInstall(context)
            resolve({ ok: true })
          }, 1200),
        )
      })
    })
    context.commands.register('agent-hub.install', (args: any) => {
      const cli: string = args?.cli ?? ''
      if (!install?.runScripts?.[cli]) {
        return Promise.reject(new Error(`no install recipe for cli: ${cli}`))
      }
      if (runState) return Promise.reject(new Error('another run is active'))
      // startRun 同步落 runState 并返回新 runId（无剧本时 null）——避免
      // 早退收窄后 `runState?.runId` 落在 never 上（TS 不跟踪跨函数赋值）
      const runId = startRun(context, cli, !!args?.mirror)
      return { runId }
    })
    context.commands.register('agent-hub.cancel-run', () => {
      cancelRun(context)
      return { ok: true }
    })
  }

  // ==================== Skills 域 ====================
  if (skills) {
    context.commands.register('agent-hub.get-skills-state', () => {
      const { skillContents: _c, localImport: _l, ...state } = skills!
      return { state: clone(state) }
    })
    context.commands.register('agent-hub.scan-skills', () => {
      skills!.status = 'scanning'
      emitSkills(context)
      return new Promise((resolve) => {
        timers.push(
          setTimeout(() => {
            if (!skills) return
            skills.status = 'ready'
            skills.scannedAt = Date.now()
            emitSkills(context)
            resolve({ ok: true })
          }, 900),
        )
      })
    })
    context.commands.register('agent-hub.read-skill', (args: any) => {
      const entry = (skills!.skills as SkillEntry[]).find((s) => s.dir === args?.dir)
      if (!entry) return Promise.reject(new Error(`skill not found: ${args?.dir}`))
      const content = skills!.skillContents?.[args.dir] ?? ''
      return {
        dir: entry.dir,
        name: entry.name,
        description: entry.description,
        allowedTools: entry.allowedTools,
        content,
      }
    })
    // 保存：baseContent 与磁盘现状不一致且未 force → 冲突回磁盘现状；
    // 成功保存重算 SKILL.md hash 并把已分发目标转 stale（模拟副本落后检测）
    context.commands.register('agent-hub.save-skill', (args: any) => {
      const dir: string = args?.dir ?? ''
      const contents = skills!.skillContents ?? (skills!.skillContents = {})
      const current = contents[dir]
      if (typeof current === 'string' && args?.baseContent !== current && !args?.force) {
        return { saved: false, conflict: true, current }
      }
      contents[dir] = args?.content ?? ''
      const entry = (skills!.skills as SkillEntry[]).find((s) => s.dir === dir)
      if (entry) {
        const file = entry.files.find((f) => f.path === 'SKILL.md')
        if (file) file.hash = fnv1aHex(args.content ?? '')
        entry.hash = fnv1aHex(args.content ?? '')
        for (const target of Object.values(entry.distribution)) {
          if (target.status === 'distributed') {
            target.status = 'stale'
            if (!target.staleFiles.includes('SKILL.md')) target.staleFiles.push('SKILL.md')
          }
        }
      }
      return new Promise((resolve) => {
        timers.push(setTimeout(() => { emitSkills(context); resolve({ saved: true }) }, 300))
      })
    })
    context.commands.register('agent-hub.distribute-skill', (args: any) => {
      const dir: string = args?.dir ?? ''
      const targets: string[] = Array.isArray(args?.targets) && args.targets.length > 0
        ? args.targets
        : ['claude', 'pi']
      return new Promise((resolve) => {
        timers.push(
          setTimeout(() => {
            const entry = skills
              ? (skills.skills as SkillEntry[]).find((s) => s.dir === dir)
              : undefined
            if (entry) {
              for (const t of targets) {
                entry.distribution[t as 'claude' | 'pi'] = { status: 'distributed', missingFiles: [], staleFiles: [] }
              }
            }
            emitSkills(context)
            resolve({ ok: true })
          }, 500),
        )
      })
    })
    // GitHub 安装：url 末段解析 skill 名；同名未覆盖 → exists 分支；
    // 成功追加最小条目（SKILL.md 单文件）并更新 github.last
    context.commands.register('agent-hub.install-github-skill', (args: any) => {
      const url: string = args?.url ?? ''
      let name = ''
      try {
        const seg = new URL(url).pathname.split('/').filter(Boolean).pop() ?? ''
        name = seg.replace(/\.git$/, '').replace(/\s+/g, '-').toLowerCase()
      } catch {
        return Promise.reject(new Error(`invalid github url: ${url}`))
      }
      if (!name) return Promise.reject(new Error(`invalid github url: ${url}`))
      const list = skills!.skills as SkillEntry[]
      if (!args?.overwrite && list.some((s) => s.dir === name)) {
        return { installed: false, exists: [name] }
      }
      const content = skills!.skillContents?.[name] ?? `---\nname: ${name}\ndescription: (GitHub 安装演示)\n---\n\n# ${name}\n`
      const entry: SkillEntry = {
        dir: name,
        path: `${skills!.libraryRoot}/${name}`,
        name,
        description: `(GitHub 安装演示) ${name}`,
        allowedTools: null,
        files: [{ path: 'SKILL.md', hash: fnv1aHex(content) }],
        hash: fnv1aHex(content),
        error: null,
        distribution: {
          claude: { status: 'none', missingFiles: [], staleFiles: [] },
          pi: { status: 'none', missingFiles: [], staleFiles: [] },
        },
      }
      const idx = list.findIndex((s) => s.dir === name)
      if (idx >= 0) list[idx] = entry
      else list.push(entry)
      skills!.github = {
        last: { ok: true, installed: [name], skippedFiles: 0, skipped: [], error: null, at: Date.now() },
      }
      emitSkills(context)
      return { installed: true, names: [name] }
    })
    // 本地导入：无 path → 命中种子演示目录；同名未 force → exists 分支；
    // 成功导入追加/替换条目并更新 import.last
    context.commands.register('agent-hub.import-skill', (args: any) => {
      const seed = skills!.localImport
      if (!args?.path) {
        if (!seed) return { picked: false }
        return { picked: true, path: seed.path, name: seed.name }
      }
      const name: string = args.name ?? args.path.split('/').filter(Boolean).pop() ?? ''
      const list = skills!.skills as SkillEntry[]
      if (!args?.force && list.some((s) => s.dir === name)) {
        return { picked: true, path: args.path, name, exists: true }
      }
      const content = skills!.skillContents?.[name] ?? `# ${name}\n`
      const entry: SkillEntry = {
        dir: name,
        path: `${skills!.libraryRoot}/${name}`,
        name,
        description: `(本地导入) ${name}`,
        allowedTools: null,
        files: [{ path: 'SKILL.md', hash: fnv1aHex(content) }],
        hash: fnv1aHex(content),
        error: null,
        distribution: {
          claude: { status: 'none', missingFiles: [], staleFiles: [] },
          pi: { status: 'none', missingFiles: [], staleFiles: [] },
        },
      }
      const idx = list.findIndex((s) => s.dir === name)
      if (idx >= 0) list[idx] = entry
      else list.push(entry)
      skills!.import = {
        last: { ok: true, name, fileCount: entry.files.length, error: null, at: Date.now() },
      }
      emitSkills(context)
      return { picked: true, path: args.path, name }
    })
  }

  // ==================== 供应商域 ====================
  if (providers) {
    context.commands.register('agent-hub.get-providers-state', () => {
      const {
        importDiscoveries: _d,
        importKeys: _k,
        applyFiles: _f,
        applyBridges: _b,
        ...state
      } = providers!
      return { state: clone(state) }
    })
    context.commands.register('agent-hub.save-preset', (args: any) => {
      const presets = providers!.presets as ProviderPreset[]
      const name: string = args?.name ?? ''
      if (presets.some((p) => p.name === name && p.id !== args?.id)) {
        return { saved: false, nameExists: true }
      }
      if (typeof args?.id === 'number') {
        const p = presets.find((x) => x.id === args.id)
        if (!p) return { saved: false }
        Object.assign(p, {
          name: args.name ?? p.name,
          baseUrl: args.baseUrl ?? p.baseUrl,
          apiStyle: args.apiStyle ?? p.apiStyle,
          models: Array.isArray(args.models) ? args.models : p.models,
          updatedAt: Date.now(),
        })
      } else {
        const id = presets.reduce((m, p) => Math.max(m, p.id), 0) + 1
        presets.push({
          id,
          name,
          baseUrl: args?.baseUrl ?? '',
          apiStyle: args?.apiStyle ?? 'openai',
          models: Array.isArray(args?.models) ? args.models : [],
          notes: null,
          createdAt: Date.now(),
          updatedAt: Date.now(),
        })
      }
      emitProviders(context)
      return { saved: true }
    })
    context.commands.register('agent-hub.delete-preset', (args: any) => {
      const presets = providers!.presets as ProviderPreset[]
      const idx = presets.findIndex((p) => p.id === args?.id)
      if (idx >= 0) presets.splice(idx, 1)
      emitProviders(context)
      return { ok: idx >= 0 }
    })
    // 反向导入：种子发现条目按名去重 → created/skipped + key 掩码
    context.commands.register('agent-hub.import-providers', () => {
      const presets = providers!.presets as ProviderPreset[]
      const created: string[] = []
      const skipped: string[] = []
      for (const d of providers!.importDiscoveries ?? []) {
        if (presets.some((p) => p.name === d.name)) {
          skipped.push(d.name)
          continue
        }
        const id = presets.reduce((m, p) => Math.max(m, p.id), 0) + 1
        presets.push({ ...d, id, createdAt: Date.now(), updatedAt: Date.now() })
        created.push(d.name)
      }
      const keys = clone(providers!.importKeys ?? {})
      const result = { created, skipped, keys }
      providers!.import = { last: { ok: true, ...clone(result), error: null, at: Date.now() } }
      emitProviders(context)
      return result
    })
    // 应用预设：claude 桥接冲突（未 force）→ conflict 分支；成功更新 claude 视图
    context.commands.register('agent-hub.apply-provider', (args: any) => {
      const presets = providers!.presets as ProviderPreset[]
      const preset = presets.find((p) => p.id === args?.id)
      if (!preset) return { applied: false, error: 'preset not found' }
      const target: string = args?.target ?? ''
      const bridges: string[] = providers!.applyBridges ?? []
      if (target === 'claude' && providers!.claude.bridge.providerConfigSh && !args?.force) {
        return { applied: false, bridgeConflict: true, bridges: [...bridges] }
      }
      const key = args?.key ?? {}
      if (key.kind === 'inline' && typeof key.value === 'string') {
        providers!.claude.env.authTokenMask = maskKey(key.value)
      } else if (key.kind === 'none') {
        providers!.claude.env.authTokenMask = null
      }
      providers!.claude.env.baseUrl = preset.baseUrl
      providers!.claude.env.model = preset.models[0] ?? null
      const files: string[] = [...(providers!.applyFiles?.[target] ?? [])]
      providers!.apply = {
        last: {
          ok: true,
          preset: preset.name,
          target: target as any,
          files,
          keyMode: key.kind ?? 'none',
          keyLen: typeof key.value === 'string' ? key.value.length : null,
          error: null,
          at: Date.now(),
        },
      }
      emitProviders(context)
      return { applied: true, files }
    })
  }

  // ==================== 使用统计域 ====================
  if (usage) {
    context.commands.register('agent-hub.get-usage-state', () => ({ state: clone(usage!.state) }))
    // 扫描：syncing 事件 → 1.6s 后 ok 事件（前端 syncing → ok 自动重拉看板/列表）
    context.commands.register('agent-hub.scan-usage', () => {
      usage!.state = { ...usage!.state, status: 'syncing' }
      emitUsage(context)
      return new Promise((resolve) => {
        timers.push(
          setTimeout(() => {
            if (!usage) return
            usage.state = { ...usage.state, status: 'ok', syncedAt: Date.now() }
            emitUsage(context)
            resolve({ state: clone(usage.state) })
          }, 1600),
        )
      })
    })
    context.commands.register('agent-hub.get-usage-stats', () => clone(usage!.stats))
    // 多条件查询：adapter / 关键词 / 时间范围（与 guest list_sessions 语义对齐）
    context.commands.register('agent-hub.list-usage-sessions', (args: any) => {
      const offset = Math.max(0, Number(args?.offset) || 0)
      const limit = Math.min(200, Math.max(1, Number(args?.limit) || 15))
      const adapter: string = args?.adapter ?? ''
      const q: string = String(args?.q ?? '').trim().toLowerCase()
      const from = Number(args?.from) || 0
      const to = Number(args?.to) || Infinity
      const all = usage!.sessions.filter((s) => {
        if (adapter !== '' && s.adapter !== adapter) return false
        if (
          q &&
          ![s.title, s.project, s.cli_session_id].some((v) => String(v ?? '').toLowerCase().includes(q))
        )
          return false
        const started = Number(s.started_at) || 0
        if (from && started < from) return false
        if (Number.isFinite(to) && started > to) return false
        return true
      })
      return {
        sessions: clone(all.slice(offset, offset + limit)),
        total: all.length,
        offset,
        limit,
      }
    })
    // 来源清单：内置只读 + 自定义增删（state.sources 持久化，扫描计数合并）
    context.commands.register('agent-hub.list-usage-sources', () => {
      const sources = (usage!.state.sources ?? []).map((s) => {
        const scan = (usage!.state.adapters ?? {})[s.name]
        return scan ? { ...s, scan: clone(scan) } : s
      })
      return { sources: clone(sources) }
    })
    context.commands.register('agent-hub.add-usage-source', (args: any) => {
      const name = String(args?.name ?? '').trim()
      let path = String(args?.path ?? '').trim()
      if (!name || !path) return Promise.reject(new Error('add-source: name and path required'))
      if (path.startsWith('~/')) path = `${usage!.state.home ?? ''}/${path.slice(2)}`
      const srcs = usage!.state.sources ?? []
      if (
        srcs.some((s) => s.name === name || s.path === path) ||
        !/^[a-z][a-z0-9-]{0,31}$/.test(name)
      )
        return Promise.reject(new Error('add-source: name or path already registered'))
      srcs.push({ name, path, builtin: false })
      ;(usage!.state.adapters ?? {})[name] = {
        files: 0,
        parsed: 0,
        skipped: 0,
        sessions: 0,
        error: null,
      }
      usage!.state = { ...usage!.state }
      emitUsage(context)
      return { state: clone(usage!.state) }
    })
    context.commands.register('agent-hub.remove-usage-source', (args: any) => {
      const name = String(args?.name ?? '')
      const srcs = usage!.state.sources ?? []
      const target = srcs.find((s) => s.name === name)
      if (!target) return Promise.reject(new Error(`remove-source: ${name} not found`))
      if (target.builtin) return Promise.reject(new Error('remove-source: builtin sources cannot be removed'))
      usage!.state.sources = srcs.filter((s) => s.name !== name)
      delete (usage!.state.adapters ?? {})[name]
      usage!.state = { ...usage!.state }
      emitUsage(context)
      return { state: clone(usage!.state) }
    })
    context.commands.register('agent-hub.read-usage-session', (args: any) => {
      const id = Number(args?.id)
      const preset = usage!.sessionDetails?.[String(id)] ?? usage!.sessionDetails?.[id]
      if (preset) return clone(preset)
      const row = usage!.sessions.find((s) => s.id === id)
      if (!row) return Promise.reject(new Error(`session not found: ${id}`))
      return {
        session: clone(row),
        events: [],
        eventsTruncated: false,
        raw: [],
        rawTruncated: false,
        skippedLines: 0,
      }
    })
  }
}

/** devMock 种子容器（插件 AgentHubDevMock 的本地镜像，见 wasm-apps/agent-hub/src/devMockTypes.ts） */
interface AgentHubSeed {
  detection?: DetectionSeed
  install?: InstallSeed
  skills?: SkillsSeed
  providers?: ProvidersSeed
  usage?: UsageSeed
}

// ==================== 注入入口 ====================

/** 待清理定时器（探测动画/测速/扫描等模拟延时） */
let timers: number[] = []

/**
 * 注册 agent-hub 领域命令 mock（loader 在 activate 前调用）
 *
 * 仅注册种子中存在的子域命令；返回 Disposable 供插件 deactivate 清理定时器
 */
export function registerAgentHubMock(context: PluginContext, pluginId: string): {
  dispose(): void
} {
  // SAFETY: dev-shell 刻意不收录插件包类型（SDK 不依赖插件），种子形状按
  // 插件 devMockTypes.ts 的领域接口本地镜像后 cast——运行期种子字段是这些
  // 接口的超集（额外字段经索引签名透传），cast 只收窄不丢字段
  const seed = getDevMock(pluginId) as unknown as AgentHubSeed | undefined
  if (!seed) return { dispose() {} }

  detection = seed.detection ? clone(seed.detection) : null
  install = seed.install ? clone(seed.install) : null
  skills = seed.skills ? clone(seed.skills) : null
  providers = seed.providers ? clone(seed.providers) : null
  usage = seed.usage ? clone(seed.usage) : null

  timers = []
  registerCommands(context)

  return {
    dispose() {
      stopRunTimer()
      runState = null
      while (timers.length) clearTimeout(timers.pop()!)
      detection = install = skills = providers = null
      usage = null
    },
  }
}
