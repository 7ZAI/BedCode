/**
 * useRemoteFs 编排测试（ticket 09 场景矩阵补齐）
 *
 * 承接被删宿主 usePeerRemoteFiles 编排测试的场景矩阵（dirId 两级契约版）：
 * 根清单加载与失败错误态（i18n key）、进入共享根、面包屑导航（cd/up/goTo/
 * goRoot）、勾选派生只计文件（selectedTotalSize/allSelected/selectedFiles）、
 * reset 全清。mock 最小 PluginContext，只测编排逻辑不测渲染。
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
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
    i18n: {
      t(key: string) {
        return key
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

/** 根清单 / 目录清单双形态应答器 */
function respondRootsAndEntries(env: ReturnType<typeof makeContext>): void {
  env.onCommand('file-transfer.list-remote', (args: any) =>
    args?.dirId === '' ? { roots: [ROOT] } : { entries: FILE_ENTRIES },
  )
}

describe('useRemoteFs orchestration', () => {
  let env: ReturnType<typeof makeContext>

  beforeEach(() => {
    vi.clearAllMocks()
    env = makeContext()
  })

  it('loadRoots renders shared roots as enterable entries and stays at the chooser level', async () => {
    env.onCommand('file-transfer.list-remote', () => ({
      roots: [ROOT, { id: 'root-2', name: '文档' }],
    }))
    const fs = useRemoteFs(env.context)

    await fs.loadRoots()

    expect(env.lastCall('file-transfer.list-remote')).toMatchObject({
      args: { path: '', dirId: '' },
    })
    expect(fs.currentRoot.value).toBeNull()
    expect(fs.entries.value.map((e) => e.name)).toEqual(['下载', '文档'])
    expect(fs.error.value).toBeNull()
    expect(fs.loading.value).toBe(false)
  })

  it('roots fetch failure surfaces the i18n error key with an empty listing', async () => {
    env.onCommand('file-transfer.list-remote', () => Promise.reject(new Error('offline')))
    const fs = useRemoteFs(env.context)

    await fs.loadRoots()

    expect(fs.error.value).toBe('transfer.table.dirUnavailable')
    expect(fs.entries.value).toEqual([])
    expect(fs.loading.value).toBe(false)
  })

  it('zero roots keeps the chooser with no error; dir listing notice passes through', async () => {
    const fs = useRemoteFs(env.context)

    env.onCommand('file-transfer.list-remote', () => ({ roots: [] }))
    await fs.loadRoots()
    expect(fs.currentRoot.value).toBeNull()
    expect(fs.entries.value).toEqual([])
    expect(fs.error.value).toBeNull()

    env.onCommand('file-transfer.list-remote', (args: any) =>
      args?.dirId === '' ? { roots: [ROOT] } : { entries: FILE_ENTRIES, notice: 'saf-filtered' },
    )
    await fs.enterRoot(ROOT)
    expect(fs.notice.value).toBe('saf-filtered')
  })

  it('enterRoot loads the root listing addressed by dirId and seeds crumbs', async () => {
    respondRootsAndEntries(env)
    const fs = useRemoteFs(env.context)

    await fs.enterRoot(ROOT)

    expect(env.lastCall('file-transfer.list-remote')).toMatchObject({
      args: { path: '', dirId: 'root-1' },
    })
    expect(fs.crumbs.value).toEqual(['下载'])
  })

  it('cd nests rel paths inside the root; at the chooser level it resolves the real dirId by name', async () => {
    respondRootsAndEntries(env)
    const fs = useRemoteFs(env.context)

    // 根清单层点击 = 进入该共享根：按展示名从根清单解析真实 dirId（展示名 ≠ id，
    // 直接拿名字当 id 会寻址不到目录表 → 「目录不可用」）
    await fs.loadRoots()
    await fs.cd('下载')
    expect(fs.currentRoot.value).toEqual({ id: 'root-1', name: '下载' })

    await fs.cd('docs')
    expect(env.lastCall('file-transfer.list-remote')!.args).toEqual({ path: 'docs', dirId: 'root-1' })
    expect(fs.currentPath.value).toBe('docs')
    expect(fs.crumbs.value).toEqual(['下载', 'docs'])

    await fs.cd('sub')
    expect(env.lastCall('file-transfer.list-remote')!.args).toEqual({ path: 'docs/sub', dirId: 'root-1' })
  })

  it('cd at the chooser level without a prior roots fetch falls back to same-name id', async () => {
    respondRootsAndEntries(env)
    const fs = useRemoteFs(env.context)

    // 防御兑底：未拉取过根清单时无法回查映射，同名兜底不阻断进入
    await fs.cd('下载')
    expect(fs.currentRoot.value).toEqual({ id: '下载', name: '下载' })
  })

  it('up walks back; from the first level it returns to the roots chooser', async () => {
    respondRootsAndEntries(env)
    const fs = useRemoteFs(env.context)
    await fs.enterRoot(ROOT)
    await fs.cd('docs')

    await fs.up()
    expect(env.lastCall('file-transfer.list-remote')!.args).toEqual({ path: '', dirId: '' })
    expect(fs.currentRoot.value).toBeNull()
    expect(fs.entries.value.map((e) => e.name)).toEqual(['下载'])
  })

  it('goTo slices the path segments; negative index falls back to the chooser', async () => {
    respondRootsAndEntries(env)
    const fs = useRemoteFs(env.context)
    await fs.enterRoot(ROOT)
    await fs.cd('docs')
    await fs.cd('sub')

    await fs.goTo(1)
    expect(env.lastCall('file-transfer.list-remote')!.args).toEqual({ path: 'docs', dirId: 'root-1' })

    await fs.goTo(-1)
    expect(fs.currentRoot.value).toBeNull()
  })

  it('selection derives count/total over files only; toggleAll covers every entry name', async () => {
    respondRootsAndEntries(env)
    const fs = useRemoteFs(env.context)
    await fs.enterRoot(ROOT)

    // 勾选部分文件（目录不计入大小）
    fs.toggle('a.pdf')
    fs.toggle('pics')
    fs.toggle('b.txt')
    expect(fs.selectedCount.value).toBe(3)
    expect(fs.selectedTotalSize.value).toBe(150)
    expect(fs.selectedFiles.value).toEqual(['a.pdf', 'b.txt'])

    fs.toggle('pics') // 取消目录勾选后未全选
    expect(fs.allSelected.value).toBe(false)
    fs.toggleAll() // 全选（含目录名，allSelected 才为真）
    expect(fs.allSelected.value).toBe(true)

    fs.toggle('a.pdf')
    expect(fs.allSelected.value).toBe(false)
    fs.clearSelection()
    expect(fs.selectedCount.value).toBe(0)
  })

  it('toggleAll is a no-op at the chooser level (no root entered)', async () => {
    env.onCommand('file-transfer.list-remote', () => ({ roots: [ROOT] }))
    const fs = useRemoteFs(env.context)
    await fs.loadRoots()

    fs.toggleAll()
    expect(fs.selected.value.size).toBe(0)
  })

  it('reset clears browsing state entirely (peer offline)', async () => {
    respondRootsAndEntries(env)
    const fs = useRemoteFs(env.context)
    await fs.enterRoot(ROOT)
    fs.toggle('a.pdf')
    fs.notice.value = 'filtered'

    fs.reset()

    expect(fs.currentRoot.value).toBeNull()
    expect(fs.currentPath.value).toBe('')
    expect(fs.entries.value).toEqual([])
    expect(fs.selected.value.size).toBe(0)
    expect(fs.notice.value).toBeNull()
    expect(fs.error.value).toBeNull()
  })
})
