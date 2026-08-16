/**
 * useFileTree composable 单元测试
 *
 * mock useHttpApi 模块（httpGetFileTree / httpGetFileTreeChildren / httpGetDiffTree），
 * 覆盖：全量/懒加载两种模式、缓存命中、过滤规则递归过滤、默认展开、
 * 节点转换（nodeType→type、Windows 反斜杠路径归一化、文件夹初始折叠）、
 * 懒加载子节点（文件/已加载/加载中跳过，失败置空防重试）、
 * expandAll/collapseAll/expandAllLazy、refresh 缓存失效与 noCache 参数、
 * diff 模式、updateSettings、sessionId 变化自动刷新。
 * 注意：treeCache 是模块级 Map，各测试用唯一 sessionId 避免跨测试缓存污染。
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { nextTick, ref } from 'vue'

const { httpMocks } = vi.hoisted(() => ({
  httpMocks: {
    httpGetFileTree: vi.fn(),
    httpGetFileTreeChildren: vi.fn(),
    httpGetDiffTree: vi.fn(),
  },
}))

vi.mock('@/composables/useHttpApi', () => ({
  useHttpApi: () => httpMocks,
  setApiBaseUrl: vi.fn(),
}))

import { useFileTree, type FileTreeNode } from '@/composables/useFileTree'

const SETTINGS_KEY = 'bedcode:sidebar-settings'
const DEFAULT_PATTERNS = ['node_modules', 'target', '.git', 'dist', 'build']

/** API 成功响应（code=0） */
const treeOk = (tree: unknown[]) => ({ code: 0, message: 'ok', data: { tree } })
const childrenOk = (children: unknown[]) => ({ code: 0, message: 'ok', data: { children } })

async function flushAsync() {
  await new Promise((r) => setTimeout(r, 0))
  await new Promise((r) => setTimeout(r, 0))
}

/** 收集树中所有文件夹名 */
function folderNames(nodes: FileTreeNode[]): string[] {
  return nodes.flatMap((n) =>
    n.type === 'folder' ? [n.name, ...folderNames(n.children || [])] : [],
  )
}

describe('useFileTree', () => {
  beforeEach(() => {
    localStorage.clear()
    vi.clearAllMocks()
  })

  it('full mode: fetches tree, transforms nodes, normalizes windows paths, folders collapsed', async () => {
    httpMocks.httpGetFileTree.mockResolvedValue(
      treeOk([
        {
          name: 'src',
          nodeType: 'folder',
          path: 'src',
          children: [{ name: 'main.ts', nodeType: 'file', path: 'src\\main.ts' }],
        },
        { name: 'README.md', nodeType: 'file', path: 'README.md' },
      ]),
    )
    const ft = useFileTree(ref('s1'))
    await flushAsync()

    expect(httpMocks.httpGetFileTree).toHaveBeenCalledWith('s1', DEFAULT_PATTERNS)
    expect(ft.loading.value).toBe(false)
    expect(ft.error.value).toBeNull()
    const src = ft.tree.value[0]
    expect(src).toMatchObject({ name: 'src', type: 'folder', expanded: false })
    // Windows 反斜杠路径统一为 /
    expect(src.children![0]).toMatchObject({ name: 'main.ts', type: 'file', path: 'src/main.ts' })
    expect(ft.tree.value[1]).toMatchObject({ name: 'README.md', type: 'file' })
  })

  it('full mode: applies filter patterns recursively and drops ignored folders', async () => {
    httpMocks.httpGetFileTree.mockResolvedValue(
      treeOk([
        {
          name: 'src',
          nodeType: 'folder',
          path: 'src',
          children: [{ name: 'index.ts', nodeType: 'file', path: 'src/index.ts' }],
        },
        {
          name: 'node_modules',
          nodeType: 'folder',
          path: 'node_modules',
          children: [
            { name: 'lodash', nodeType: 'folder', path: 'node_modules/lodash', children: [] },
          ],
        },
        { name: 'package.json', nodeType: 'file', path: 'package.json' },
      ]),
    )
    const ft = useFileTree(ref('s2'))
    await flushAsync()
    expect(ft.tree.value.map((n) => n.name)).toEqual(['src', 'package.json'])
  })

  it('cache: remount with same session id reuses module cache without refetching', async () => {
    httpMocks.httpGetFileTree.mockResolvedValue(treeOk([{ name: 'a.txt', nodeType: 'file', path: 'a.txt' }]))
    const sessionId = ref('s3')
    useFileTree(sessionId)
    await flushAsync()
    expect(httpMocks.httpGetFileTree).toHaveBeenCalledTimes(1)

    // 重新挂载（同 sessionId）：命中模块级 treeCache，不再请求后端
    const remounted = useFileTree(sessionId)
    await flushAsync()
    expect(httpMocks.httpGetFileTree).toHaveBeenCalledTimes(1)
    expect(remounted.tree.value).toHaveLength(1)
  })

  it('defaultExpanded: all folders expanded after fetch', async () => {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify({ defaultExpanded: true }))
    httpMocks.httpGetFileTree.mockResolvedValue(
      treeOk([
        {
          name: 'src',
          nodeType: 'folder',
          path: 'src',
          children: [
            {
              name: 'lib',
              nodeType: 'folder',
              path: 'src/lib',
              children: [{ name: 'a.ts', nodeType: 'file', path: 'src/lib/a.ts' }],
            },
          ],
        },
      ]),
    )
    const ft = useFileTree(ref('s4'))
    await flushAsync()
    expect(ft.tree.value[0].expanded).toBe(true)
    expect(ft.tree.value[0].children![0].expanded).toBe(true)
  })

  it('fetch failure: error set, tree emptied, loading cleared', async () => {
    httpMocks.httpGetFileTree.mockRejectedValue(new Error('network down'))
    const ft = useFileTree(ref('s5'))
    await flushAsync()
    expect(ft.loading.value).toBe(false)
    expect(ft.tree.value).toEqual([])
    expect(ft.error.value).toContain('network down')
  })

  it('non-ok API result treated as failure with backend message', async () => {
    httpMocks.httpGetFileTree.mockResolvedValue({ code: 1, message: 'boom', data: null })
    const ft = useFileTree(ref('s6'))
    await flushAsync()
    expect(ft.tree.value).toEqual([])
    // throw new Error(message) → catch 里 e.toString() 为 'Error: boom'
    expect(ft.error.value).toBe('Error: boom')
  })

  it('lazy mode: root fetch only; loadChildren fetches children on demand', async () => {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify({ lazyLoad: true }))
    httpMocks.httpGetFileTreeChildren.mockResolvedValue(
      childrenOk([
        { name: 'src', nodeType: 'folder', path: 'src' },
        { name: 'README.md', nodeType: 'file', path: 'README.md' },
      ]),
    )
    const ft = useFileTree(ref('s7'))
    await flushAsync()
    // 懒加载模式只取根一层，且不调用全量接口
    expect(httpMocks.httpGetFileTreeChildren).toHaveBeenCalledWith('s7', '', DEFAULT_PATTERNS)
    expect(httpMocks.httpGetFileTree).not.toHaveBeenCalled()

    // 展开文件夹 → 按需加载子节点
    httpMocks.httpGetFileTreeChildren.mockResolvedValueOnce(
      childrenOk([{ name: 'main.ts', nodeType: 'file', path: 'src/main.ts' }]),
    )
    const src = ft.tree.value.find((n) => n.name === 'src')!
    await ft.loadChildren(src)
    expect(src.children).toEqual([expect.objectContaining({ name: 'main.ts', type: 'file' })])
    expect(src.loading).toBe(false)
  })

  it('loadChildren: skips files, already-loaded folders and in-flight folders', async () => {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify({ lazyLoad: true }))
    httpMocks.httpGetFileTreeChildren.mockResolvedValue(
      childrenOk([
        { name: 'a', nodeType: 'folder', path: 'a' },
        { name: 'b', nodeType: 'folder', path: 'b' },
        { name: 'f.txt', nodeType: 'file', path: 'f.txt' },
      ]),
    )
    const ft = useFileTree(ref('s8'))
    await flushAsync()

    // 文件节点不加载
    const fileNode = ft.tree.value.find((n) => n.name === 'f.txt')!
    await ft.loadChildren(fileNode)
    expect(httpMocks.httpGetFileTreeChildren).toHaveBeenCalledTimes(1) // 仅初始 root 一次

    // 正常加载文件夹
    const a = ft.tree.value.find((n) => n.name === 'a')!
    httpMocks.httpGetFileTreeChildren.mockResolvedValueOnce(
      childrenOk([{ name: 'a1', nodeType: 'file', path: 'a/a1' }]),
    )
    await ft.loadChildren(a)
    expect(httpMocks.httpGetFileTreeChildren).toHaveBeenCalledTimes(2)

    // 已加载 → 跳过
    await ft.loadChildren(a)
    expect(httpMocks.httpGetFileTreeChildren).toHaveBeenCalledTimes(2)

    // 加载中 → 跳过（并发防重）
    const b = ft.tree.value.find((n) => n.name === 'b')!
    b.loading = true
    await ft.loadChildren(b)
    expect(httpMocks.httpGetFileTreeChildren).toHaveBeenCalledTimes(2)
  })

  it('loadChildren failure: children set to empty array to avoid retry loops', async () => {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify({ lazyLoad: true }))
    httpMocks.httpGetFileTreeChildren.mockResolvedValue(
      childrenOk([{ name: 'd', nodeType: 'folder', path: 'd' }]),
    )
    const ft = useFileTree(ref('s9'))
    await flushAsync()

    httpMocks.httpGetFileTreeChildren.mockRejectedValueOnce(new Error('boom'))
    const d = ft.tree.value.find((n) => n.name === 'd')!
    await ft.loadChildren(d)
    expect(d.children).toEqual([])
    expect(d.loading).toBe(false)
    expect(ft.error.value).toContain('boom')
  })

  it('expandAll/collapseAll (full mode): toggle expansion state of all folders', async () => {
    httpMocks.httpGetFileTree.mockResolvedValue(
      treeOk([
        {
          name: 'src',
          nodeType: 'folder',
          path: 'src',
          children: [
            {
              name: 'lib',
              nodeType: 'folder',
              path: 'src/lib',
              children: [{ name: 'a.ts', nodeType: 'file', path: 'src/lib/a.ts' }],
            },
          ],
        },
      ]),
    )
    const ft = useFileTree(ref('s10'))
    await flushAsync()
    expect(folderNames(ft.tree.value)).toEqual(['src', 'lib'])

    ft.expandAll()
    expect(ft.tree.value[0].expanded).toBe(true)
    expect(ft.tree.value[0].children![0].expanded).toBe(true)

    ft.collapseAll()
    expect(ft.tree.value[0].expanded).toBe(false)
    expect(ft.tree.value[0].children![0].expanded).toBe(false)
  })

  it('expandAll in lazy mode: recursively loads every folder then expands all', async () => {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify({ lazyLoad: true }))
    httpMocks.httpGetFileTreeChildren.mockResolvedValueOnce(
      childrenOk([
        { name: 'src', nodeType: 'folder', path: 'src' },
        { name: 'docs', nodeType: 'folder', path: 'docs' },
      ]),
    )
    const ft = useFileTree(ref('s11'))
    await flushAsync()

    httpMocks.httpGetFileTreeChildren
      .mockResolvedValueOnce(childrenOk([{ name: 'main.ts', nodeType: 'file', path: 'src/main.ts' }]))
      .mockResolvedValueOnce(childrenOk([{ name: 'deep', nodeType: 'folder', path: 'docs/deep' }]))
      .mockResolvedValueOnce(childrenOk([{ name: 'x.md', nodeType: 'file', path: 'docs/deep/x.md' }]))

    await ft.expandAll()
    // expandAll 不 await 懒加载递归（fire-and-forget），等待链式加载完成
    await flushAsync()
    // root 1 次 + src/docs 并行 2 次 + docs/deep 递归 1 次
    expect(httpMocks.httpGetFileTreeChildren).toHaveBeenCalledTimes(4)
    expect(ft.tree.value[0].expanded).toBe(true)
    expect(ft.tree.value[1].expanded).toBe(true)
    const docs = ft.tree.value[1]
    expect(docs.children![0].expanded).toBe(true)
    expect(docs.children![0].children![0].name).toBe('x.md')
  })

  it('refresh (full mode): deletes cache and refetches', async () => {
    httpMocks.httpGetFileTree.mockResolvedValue(treeOk([{ name: 'a.txt', nodeType: 'file', path: 'a.txt' }]))
    const ft = useFileTree(ref('s12'))
    await flushAsync()
    await ft.refresh()
    expect(httpMocks.httpGetFileTree).toHaveBeenCalledTimes(2)
  })

  it('refresh (lazy mode): refetches root with noCache=true', async () => {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify({ lazyLoad: true }))
    httpMocks.httpGetFileTreeChildren.mockResolvedValue(
      childrenOk([{ name: 'a.txt', nodeType: 'file', path: 'a.txt' }]),
    )
    const ft = useFileTree(ref('s13'))
    await flushAsync()
    await ft.refresh()
    expect(httpMocks.httpGetFileTreeChildren).toHaveBeenCalledTimes(2)
    expect(httpMocks.httpGetFileTreeChildren).toHaveBeenLastCalledWith(
      's13',
      '',
      DEFAULT_PATTERNS,
      true,
    )
  })

  it('diff mode: fetchTree uses httpGetDiffTree full load', async () => {
    httpMocks.httpGetFileTree.mockResolvedValue(treeOk([{ name: 'a.txt', nodeType: 'file', path: 'a.txt' }]))
    httpMocks.httpGetDiffTree.mockResolvedValue(
      treeOk([{ name: 'changed.txt', nodeType: 'file', path: 'changed.txt' }]),
    )
    const ft = useFileTree(ref('s14'))
    await flushAsync()
    ft.toggleDiffMode()
    expect(ft.isDiffMode.value).toBe(true)
    // fetchTree 未从 composable 暴露，用 refresh 触发（非懒加载 → 走 fetchTree）
    await ft.refresh()
    expect(httpMocks.httpGetDiffTree).toHaveBeenCalledWith('s14', DEFAULT_PATTERNS)
    expect(httpMocks.httpGetFileTree).toHaveBeenCalledTimes(1) // 仅初始 watch 那次
    expect(ft.tree.value.map((n) => n.name)).toEqual(['changed.txt'])
  })

  it('updateSettings: applies, persists, invalidates cache and refetches', async () => {
    httpMocks.httpGetFileTree.mockResolvedValue(
      treeOk([{ name: 'src', nodeType: 'folder', path: 'src' }]),
    )
    const ft = useFileTree(ref('s15'))
    await flushAsync()
    ft.updateSettings({
      defaultExpanded: true,
      filterPatterns: ['node_modules'],
      fontSize: 15,
      lazyLoad: false,
    })
    await flushAsync()
    expect(ft.settings.value.defaultExpanded).toBe(true)
    expect(ft.settings.value.fontSize).toBe(15)
    expect(JSON.parse(localStorage.getItem(SETTINGS_KEY)!)).toMatchObject({ fontSize: 15 })
    // 缓存失效 → 重新拉取（初始 watch 1 次 + updateSettings 1 次）
    expect(httpMocks.httpGetFileTree).toHaveBeenCalledTimes(2)
    // 新设置生效：文件夹默认展开
    expect(ft.tree.value[0].expanded).toBe(true)
  })

  it('sessionId change: watch auto-refetches the new session', async () => {
    httpMocks.httpGetFileTree.mockResolvedValue(treeOk([{ name: 'a.txt', nodeType: 'file', path: 'a.txt' }]))
    const sessionId = ref('s16')
    useFileTree(sessionId)
    await flushAsync()
    expect(httpMocks.httpGetFileTree).toHaveBeenLastCalledWith('s16', DEFAULT_PATTERNS)

    sessionId.value = 's17'
    await nextTick()
    await flushAsync()
    expect(httpMocks.httpGetFileTree).toHaveBeenLastCalledWith('s17', DEFAULT_PATTERNS)
  })

  it('empty session id: no fetch at all', () => {
    const ft = useFileTree(ref(''))
    expect(httpMocks.httpGetFileTree).not.toHaveBeenCalled()
    expect(ft.tree.value).toEqual([])
    expect(ft.loading.value).toBe(false)
  })
})
