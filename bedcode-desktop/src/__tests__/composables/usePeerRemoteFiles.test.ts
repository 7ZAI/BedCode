/**
 * usePeerRemoteFiles 测试（issue 11）
 *
 * 覆盖编排口径（不测渲染）：共享根清单拉取与单根自动进入、目录下钻面包屑、
 * 选择口径（文件点选/全选仅作用文件/切目录清理失效选中）、目录递归枚举
 * （含超限）、拉取入队参数拼装、权限过滤 notice 位与错误兜底。
 * invoke 经 mock 按命令名分流驱动。
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import {
  usePeerRemoteFiles,
  _resetPeerRemoteFilesForTest,
  joinRel,
  enumerateDirFiles,
  sumSelectedBytes,
  toggleAllFiles,
  formatBytes,
  type RemoteBrowseResult,
  type SharedRoot,
} from '@/composables/usePeerRemoteFiles'

// Mock Tauri invoke（本 composable 不订阅事件）
const mockInvoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: any[]) => mockInvoke(...args),
}))

const NODE_A = 'aa'.repeat(32)
const ROOT: SharedRoot = { id: 'root-1', name: 'docs' }

function listing(entries: RemoteBrowseResult['entries'], filtered = false): RemoteBrowseResult {
  return { entries, filtered }
}

describe('pure helpers', () => {
  it('joinRel concatenates with / and keeps root empty', () => {
    expect(joinRel('', 'a')).toBe('a')
    expect(joinRel('a', 'b')).toBe('a/b')
    expect(joinRel('a/b', 'c.txt')).toBe('a/b/c.txt')
  })

  it('sumSelectedBytes counts files only', () => {
    const entries = [
      { name: 'd', isDir: true, size: 0 },
      { name: 'a.txt', isDir: false, size: 10 },
      { name: 'b.txt', isDir: false, size: 5 },
    ]
    expect(sumSelectedBytes(entries, ['d', 'a.txt', 'b.txt'])).toBe(15)
    expect(sumSelectedBytes(entries, [])).toBe(0)
  })

  it('toggleAllFiles flips file membership only and never touches dirs', () => {
    const entries = [
      { name: 'd', isDir: true, size: 0 },
      { name: 'a.txt', isDir: false, size: 1 },
      { name: 'b.txt', isDir: false, size: 2 },
    ]
    const first = toggleAllFiles(entries, [])
    expect(first.sort()).toEqual(['a.txt', 'b.txt'])
    // 已含部分文件时补齐为全选；再翻转为移除全部文件但保留既有目录选中
    const partial = toggleAllFiles(entries, ['a.txt', 'keep-dir'])
    expect(partial.includes('b.txt')).toBe(true)
    expect(partial.includes('keep-dir')).toBe(true)
    const cleared = toggleAllFiles(entries, first)
    expect(cleared).toEqual([])
  })

  it('formatBytes renders human units with dash for zero', () => {
    expect(formatBytes(0)).toBe('—')
    expect(formatBytes(512)).toBe('512 B')
    expect(formatBytes(2048)).toBe('2.0 KB')
    expect(formatBytes(3 * 1024 * 1024)).toBe('3.0 MB')
  })
})

describe('enumerateDirFiles', () => {
  it('flattens nested directories breadth-first into rel paths', async () => {
    const tree: Record<string, RemoteBrowseResult> = {
      '': listing([
        { name: 'sub', isDir: true, size: 0 },
        { name: 'top.txt', isDir: false, size: 1 },
      ]),
      sub: listing([{ name: 'deep.bin', isDir: false, size: 2 }]),
    }
    const files = await enumerateDirFiles((rel) => Promise.resolve(tree[rel]!), '', {
      maxDepth: 8,
      maxFiles: 64,
    })
    expect(files).toEqual([
      { relPath: 'top.txt', size: 1 },
      { relPath: 'sub/deep.bin', size: 2 },
    ])
  })

  it('throws too-many-files beyond cap', async () => {
    const wide = Array.from({ length: 4 }, (_, i) => ({ name: `f${i}.txt`, isDir: false, size: i }))
    await expect(
      enumerateDirFiles(() => Promise.resolve(listing(wide)), '', {
        maxDepth: 8,
        maxFiles: 2,
      }),
    ).rejects.toThrow('too-many-files')
  })
})

describe('usePeerRemoteFiles flow', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    _resetPeerRemoteFilesForTest()
  })

  it('open() pulls roots; single root auto-enters and loads its listing', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'list_peer_shared_roots') return Promise.resolve([ROOT])
      if (cmd === 'browse_peer_directory')
        return Promise.resolve(
          listing([
            { name: 'inner', isDir: true, size: 0 },
            { name: 'readme.md', isDir: false, size: 7 },
          ]),
        )
      return Promise.reject(new Error(`unexpected cmd ${cmd}`))
    })
    const fs = usePeerRemoteFiles()
    await fs.open(NODE_A, 'PeerA')

    expect(mockInvoke).toHaveBeenCalledWith('list_peer_shared_roots', { nodeId: NODE_A })
    expect(fs.activeRoot.value).toEqual(ROOT)
    // 单根自动进入：browse 以根 id 寻址，rel 为空串
    expect(mockInvoke).toHaveBeenCalledWith('browse_peer_directory', {
      nodeId: NODE_A,
      dirId: ROOT.id,
      relPath: '',
    })
    expect(fs.entries.value).toHaveLength(2)
  })

  it('open() with multiple roots stays on chooser; selectRoot navigates', async () => {
    const second: SharedRoot = { id: 'root-2', name: 'downloads' }
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'list_peer_shared_roots') return Promise.resolve([ROOT, second])
      if (cmd === 'browse_peer_directory') return Promise.resolve(listing([]))
      return Promise.reject(new Error(`unexpected cmd ${cmd}`))
    })
    const fs = usePeerRemoteFiles()
    await fs.open(NODE_A, 'PeerA')
    expect(fs.activeRoot.value).toBeNull()
    expect(fs.roots.value).toHaveLength(2)

    fs.selectRoot(second)
    await vi.waitFor(() => expect(fs.loading.value).toBe(false))
    expect(fs.activeRoot.value).toEqual(second)
  })

  it('zero roots keeps chooser with no error and never browses', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'list_peer_shared_roots') return Promise.resolve([])
      return Promise.reject(new Error(`unexpected cmd ${cmd}`))
    })
    const fs = usePeerRemoteFiles()
    await fs.open(NODE_A, 'PeerA')
    expect(fs.activeRoot.value).toBeNull()
    expect(fs.errorKey.value).toBe('')
    expect(mockInvoke).not.toHaveBeenCalledWith('browse_peer_directory', expect.anything())
  })

  it('enterDir/navigateTo drive breadcrumb stack with rel paths', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'list_peer_shared_roots') return Promise.resolve([ROOT])
      if (cmd === 'browse_peer_directory') return Promise.resolve(listing([]))
      return Promise.reject(new Error(`unexpected cmd ${cmd}`))
    })
    const fs = usePeerRemoteFiles()
    await fs.open(NODE_A, 'PeerA')

    await fs.enterDir({ name: 'sub', isDir: true, size: 0 })
    expect(fs.currentPath.value).toBe('sub')
    await fs.enterDir({ name: 'deep', isDir: true, size: 0 })
    expect(fs.currentPath.value).toBe('sub/deep')
    await fs.navigateTo(0)
    expect(fs.currentPath.value).toBe('')
  })

  it('pullSelection queues selected files and enumerates chosen dirs', async () => {
    mockInvoke.mockImplementation((cmd: string, args?: any) => {
      if (cmd === 'list_peer_shared_roots') return Promise.resolve([ROOT])
      if (cmd !== 'browse_peer_directory' && cmd !== 'pull_peer_files')
        return Promise.reject(new Error(`unexpected cmd ${cmd}`))
      if (cmd === 'browse_peer_directory') {
        if (args?.relPath === '') {
          return Promise.resolve(
            listing([
              { name: 'loose.txt', isDir: false, size: 3 },
              { name: 'pack', isDir: true, size: 0 },
            ]),
          )
        }
        // 目录递归枚举路径
        return Promise.resolve(listing([{ name: 'nested.bin', isDir: false, size: 9 }]))
      }
      return Promise.resolve(2)
    })
    const fs = usePeerRemoteFiles()
    await fs.open(NODE_A, 'PeerA')

    fs.toggleSelect('loose.txt')
    fs.toggleSelect('pack')
    expect(fs.hasSelection.value).toBe(true)

    const queued = await fs.pullSelection()
    expect(queued).toBe(2)
    expect(mockInvoke).toHaveBeenCalledWith('pull_peer_files', {
      nodeId: NODE_A,
      dirId: ROOT.id,
      files: [
        { relPath: 'loose.txt', size: 3 },
        { relPath: 'pack/nested.bin', size: 9 },
      ],
    })
    // 拉取完成后选择保留（用户可继续操作），pulling 复位
    expect(fs.pulling.value).toBe(false)
  })

  it('filtered notice only shows on empty filtered listings and clears elsewhere', async () => {
    let reply = listing([], true)
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'list_peer_shared_roots') return Promise.resolve([ROOT])
      if (cmd === 'browse_peer_directory') return Promise.resolve(reply)
      return Promise.reject(new Error(`unexpected cmd ${cmd}`))
    })
    const fs = usePeerRemoteFiles()
    await fs.open(NODE_A, 'PeerA')
    expect(fs.filteredNotice.value).toBe(true)

    reply = listing([{ name: 'x.txt', isDir: false, size: 1 }], true)
    await fs.refresh()
    expect(fs.filteredNotice.value).toBe(false)
  })

  it('browse failure falls back to error key without corrupting state', async () => {
    let fail = false
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'list_peer_shared_roots') return Promise.resolve([ROOT])
      if (cmd === 'browse_peer_directory')
        return fail ? Promise.reject(new Error('dial failed')) : Promise.resolve(listing([]))
      return Promise.reject(new Error(`unexpected cmd ${cmd}`))
    })
    const fs = usePeerRemoteFiles()
    await fs.open(NODE_A, 'PeerA')
    expect(fs.errorKey.value).toBe('')

    fail = true
    await fs.refresh()
    expect(fs.errorKey.value).toBe('peers.files.error.dirUnavailable')
    expect(fs.entries.value).toEqual([])
  })

  it('roots fetch failure surfaces rootsUnavailable error key', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'list_peer_shared_roots') return Promise.reject(new Error('offline'))
      return Promise.reject(new Error(`unexpected cmd ${cmd}`))
    })
    const fs = usePeerRemoteFiles()
    await fs.open(NODE_A, 'PeerA')
    expect(fs.errorKey.value).toBe('peers.files.error.rootsUnavailable')
    expect(fs.loading.value).toBe(false)
  })
})
