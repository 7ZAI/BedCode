/**
 * useRemoteFs 编排测试（ticket 09 场景矩阵补齐）
 *
 * 承接被删宿主 usePeerRemoteFiles 编排测试的场景矩阵（dirId 两级契约版）：
 * 根清单加载与失败错误态、进入共享根、面包屑栈导航（cd/up/goTo/goRoot）、
 * 勾选只作用于文件不碰目录、目录加载失败不污染状态且可恢复、reset 全清。
 * mock 最小 PluginContext，只测编排逻辑不测渲染。
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import { useRemoteFs } from '../../../../plugins/file-transfer/src/composables/useRemoteFs'

type EventHandler = (payload: any) => void

function makeContext() {
  const calls: Array<{ id: string; args: any }> = []
  const handlers = new Map<string, EventHandler>()
  const responders = new Map<string, (args: any) => unknown>()

  const context = {
    commands: {
      async execute(id: string, args?: any) {
        calls.push({ id, args })
        const respond = responders.get(id)
        if (!respond) throw new Error(`no responder for ${id}`)
        return respond(args)
      },
      register: vi.fn(),
    },
    events: {
      on(_event: string, _handler: EventHandler) {
        return { dispose: () => {} }
      },
    },
  } as unknown as PluginContext

  function lastCall(id: string): { id: string; args: any } | undefined {
    for (let i = calls.length - 1; i >= 0; i--) if (calls[i]!.id === id) return calls[i]
    return undefined
  }

  function onCommand(id: string, respond: (args: any) => unknown): void {
    responders.set(id, respond)
  }

  return { context, calls, lastCall, onCommand }
}

const ROOT = { id: 'root-1', name: '下载' }
const FILE_ENTRIES = [
  { name: 'a.pdf', size: 100, mtime: 1, isDir: false },
  { name: 'b.txt', size: 50, mtime: 2, isDir: false },
  { name: 'pics', size: 0, mtime: 3, isDir: true },
]

describe('useRemoteFs orchestration', () => {
  let env: ReturnType<typeof makeContext>

  beforeEach(() => {
    vi.clearAllMocks()
    env = makeContext()
  })

  it('loadRoots renders shared roots as enterable dir entries at the chooser level', async () => {
    env.onCommand('file-transfer.list-remote', () => ({
      roots: [ROOT, { id: 'root-2', name: '文档' }],
    }))
    const fs = useRemoteFs(env.context, () => 'node-a')

    await fs.loadRoots()

    expect(env.lastCall('file-transfer.list-remote')).toMatchObject({
      args: { path: '', dirId: '' },
    })
    expect(fs.currentRoot.value).toBeNull()
    expect(fs.entries.value.map((e) => e.name)).toEqual(['下载', '文档'])
    expect(fs.entries.value.every((e) => e.isDir)).toBe(true)
    expect(fs.errorKey.value).toBe('')
    expect(fs.loading.value).toBe(false)
  })

  it('roots fetch failure surfaces the error key with an empty listing', async () => {
    env.onCommand('file-transfer.list-remote', () => Promise.reject(new Error('offline')))
    const fs = useRemoteFs(env.context, () => 'node-a')

    await fs.loadRoots()

    expect(fs.errorKey.value).toBe('transfer.error.dirUnavailable')
    expect(fs.entries.value).toEqual([])
    expect(fs.loading.value).toBe(false)
  })

  it('zero roots keeps the chooser with no error; dir listing notice passes through', async () => {
    const fs = useRemoteFs(env.context, () => 'node-a')

    env.onCommand('file-transfer.list-remote', () => ({ roots: [] }))
    await fs.loadRoots()
    expect(fs.currentRoot.value).toBeNull()
    expect(fs.entries.value).toEqual([])
    expect(fs.errorKey.value).toBe('')

    env.onCommand('file-transfer.list-remote', (args: any) =>
      args?.dirId === '' ? { roots: [ROOT] } : { entries: FILE_ENTRIES, notice: 'saf-filtered' },
    )
    await fs.enterRoot(ROOT)
    expect(fs.notice.value).toBe('saf-filtered')
  })

  it('enterRoot loads the root directory listing addressed by dirId', async () => {
    env.onCommand('file-transfer.list-remote', (_args: any) =>
      _args?.dirId === '' ? { roots: [ROOT] } : { entries: FILE_ENTRIES },
    )
    const fs = useRemoteFs(env.context, () => 'node-a')

    await fs.enterRoot(ROOT)

    expect(env.lastCall('file-transfer.list-remote')).toMatchObject({
      args: { path: '', dirId: 'root-1' },
    })
    expect(fs.currentRoot.value).toEqual(ROOT)
    expect(fs.breadcrumb.value.map((c) => c.name)).toEqual([
      'transfer.breadcrumb.home',
      '下载',
    ])
    expect(fs.errorKey.value).toBe('')
  })

  it('cd at the chooser level resolves the real dirId by name from the last roots fetch', async () => {
    env.onCommand('file-transfer.list-remote', () => ({
      roots: [ROOT, { id: 'root-2', name: '文档' }],
    }))
    const fs = useRemoteFs(env.context, () => 'node-a')

    // 根清单层点击 = 进入该共享根：展示名 ≠ id，直接拿名字当 id 会寻址不到
    // 对端目录表（表现为「目录不可用」）；按展示名从最近一次根清单回查真实 id
    await fs.loadRoots()
    await fs.cd('文档')
    expect(fs.currentRoot.value).toEqual({ id: 'root-2', name: '文档' })
    expect(env.lastCall('file-transfer.list-remote')!.args).toEqual({ path: '', dirId: 'root-2' })

    await fs.cd('docs')
    expect(env.lastCall('file-transfer.list-remote')!.args).toEqual({ path: 'docs', dirId: 'root-2' })
  })

  it('cd at the chooser level without a prior roots fetch falls back to same-name id', async () => {
    const fs = useRemoteFs(env.context, () => 'node-a')

    // 防御兜底：未拉取过根清单时无法回查映射，同名兜底不阻断进入
    await fs.cd('下载')
    expect(fs.currentRoot.value).toEqual({ id: '下载', name: '下载' })
  })

  it('cd builds nested rel paths and extends the breadcrumb stack', async () => {
    env.onCommand('file-transfer.list-remote', (args: any) =>
      args?.dirId === '' ? { roots: [ROOT] } : { entries: [] },
    )
    const fs = useRemoteFs(env.context, () => 'node-a')
    await fs.enterRoot(ROOT)

    await fs.cd('docs')
    expect(env.lastCall('file-transfer.list-remote')!.args).toEqual({ path: 'docs', dirId: 'root-1' })

    await fs.cd('sub')
    expect(env.lastCall('file-transfer.list-remote')!.args).toEqual({ path: 'docs/sub', dirId: 'root-1' })
    expect(fs.breadcrumb.value.map((c) => c.path)).toEqual(['', '', 'docs', 'docs/sub'])
  })

  it('up walks back and returns to the roots chooser from the first level', async () => {
    env.onCommand('file-transfer.list-remote', (args: any) =>
      args?.dirId === '' ? { roots: [ROOT] } : { entries: [] },
    )
    const fs = useRemoteFs(env.context, () => 'node-a')
    await fs.enterRoot(ROOT)
    await fs.cd('docs')

    await fs.up()
    expect(env.lastCall('file-transfer.list-remote')!.args).toEqual({ path: '', dirId: 'root-1' })

    // 根内首层再返回 → 回共享根清单
    await fs.up()
    expect(fs.currentRoot.value).toBeNull()
    expect(fs.entries.value.map((e) => e.name)).toEqual(['下载'])
  })

  it('goTo navigates via breadcrumb index; index 0 returns to the chooser', async () => {
    env.onCommand('file-transfer.list-remote', (args: any) =>
      args?.dirId === '' ? { roots: [ROOT] } : { entries: [] },
    )
    const fs = useRemoteFs(env.context, () => 'node-a')
    await fs.enterRoot(ROOT)
    await fs.cd('docs')
    await fs.cd('sub')

    // 面包屑：home / 下载 / docs / sub → goTo(2) 应落在 docs
    await fs.goTo(2)
    expect(env.lastCall('file-transfer.list-remote')!.args).toEqual({ path: 'docs', dirId: 'root-1' })
    expect(fs.breadcrumb.value.map((c) => c.name)).toEqual(['transfer.breadcrumb.home', '下载', 'docs'])

    await fs.goTo(0)
    expect(fs.currentRoot.value).toBeNull()
  })

  it('toggleAll flips file membership only and never selects dirs', async () => {
    env.onCommand('file-transfer.list-remote', (args: any) =>
      args?.dirId === '' ? { roots: [ROOT] } : { entries: FILE_ENTRIES },
    )
    const fs = useRemoteFs(env.context, () => 'node-a')
    await fs.enterRoot(ROOT)

    fs.toggleAll()
    expect([...fs.selectedNames.value].sort()).toEqual(['a.pdf', 'b.txt'])
    expect(fs.selectedEntries.value.every((e) => !e.isDir)).toBe(true)
    expect(fs.hasSelection.value).toBe(true)

    fs.toggleAll()
    expect(fs.hasSelection.value).toBe(false)

    fs.toggle('a.pdf')
    expect(fs.selectedNames.value).toEqual(['a.pdf'])
    fs.clearSelection()
    expect(fs.selectedNames.value).toEqual([])
  })

  it('browse failure sets the error key without corrupting state and recovers on reload', async () => {
    let fail = true
    env.onCommand('file-transfer.list-remote', (args: any) =>
      args?.dirId === ''
        ? { roots: [ROOT] }
        : args?.path === 'docs'
          ? fail
            ? Promise.reject(new Error('denied'))
            : { entries: FILE_ENTRIES }
          : { entries: [] },
    )
    const fs = useRemoteFs(env.context, () => 'node-a')
    await fs.enterRoot(ROOT) // 先进入共享根，确保失败发生在目录层而非根清单层

    await fs.cd('docs')

    expect(fs.errorKey.value).toBe('transfer.error.dirUnavailable')
    expect(fs.entries.value).toEqual([])
    expect(fs.loading.value).toBe(false)
    expect(fs.currentRoot.value).toEqual(ROOT) // 浏览位置未被失败破坏

    fail = false
    await fs.cd('docs')
    expect(fs.errorKey.value).toBe('')
    expect(fs.entries.value).toHaveLength(3)
  })

  it('reset clears browsing state entirely (peer offline)', async () => {
    env.onCommand('file-transfer.list-remote', (args: any) =>
      args?.dirId === '' ? { roots: [ROOT] } : { entries: FILE_ENTRIES },
    )
    const fs = useRemoteFs(env.context, () => 'node-a')
    await fs.enterRoot(ROOT)
    fs.toggle('a.pdf')

    fs.reset()

    expect(fs.currentRoot.value).toBeNull()
    expect(fs.entries.value).toEqual([])
    expect(fs.selectedNames.value).toEqual([])
    expect(fs.notice.value).toBeNull()
    expect(fs.errorKey.value).toBe('')
  })
})
