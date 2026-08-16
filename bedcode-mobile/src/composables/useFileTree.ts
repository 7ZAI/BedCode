import { ref, watch, type Ref } from 'vue'
import { useHttpApi } from './useHttpApi'

// ==================== Types ====================

export interface FileTreeNode {
  name: string
  type: 'file' | 'folder'
  path?: string // 相对于工作目录的路径
  children?: FileTreeNode[]
  expanded?: boolean // folder only
  loading?: boolean // 文件夹子节点加载中标记（懒加载模式）
}

// ==================== Settings ====================

export interface SidebarSettings {
  defaultExpanded: boolean
  filterPatterns: string[]
  fontSize: number // 文件树字体大小 (px)，范围 10-20
  lazyLoad: boolean // 懒加载模式，默认关闭
}

const SETTINGS_KEY = 'bedcode:sidebar-settings'

/** 文件树字体大小范围 */
export const FONT_SIZE_MIN = 10
export const FONT_SIZE_MAX = 20
export const FONT_SIZE_DEFAULT = 13

const DEFAULT_SETTINGS: SidebarSettings = {
  defaultExpanded: false,
  filterPatterns: ['node_modules', 'target', '.git', 'dist', 'build'],
  fontSize: FONT_SIZE_DEFAULT,
  lazyLoad: false,
}

function loadSettings(): SidebarSettings {
  try {
    const raw = localStorage.getItem(SETTINGS_KEY)
    if (raw) return { ...DEFAULT_SETTINGS, ...JSON.parse(raw) }
  } catch { /* ignore corrupt data */ }
  return { ...DEFAULT_SETTINGS }
}

function saveSettings(settings: SidebarSettings): void {
  localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings))
}

// ==================== Utility Functions ====================

/** 递归设置所有文件夹的展开状态 */
function setAllExpanded(nodes: FileTreeNode[], expanded: boolean): void {
  for (const node of nodes) {
    if (node.type === 'folder') {
      node.expanded = expanded
      if (node.children) {
        setAllExpanded(node.children, expanded)
      }
    }
  }
}

/** 递归过滤匹配过滤规则的文件夹节点 */
function filterTree(nodes: FileTreeNode[], patterns: string[]): FileTreeNode[] {
  return nodes
    .filter(node => {
      if (node.type === 'folder' && patterns.some(p => node.name.toLowerCase() === p.toLowerCase().trim())) {
        return false
      }
      return true
    })
    .map(node => {
      if (node.type === 'folder' && node.children) {
        return { ...node, children: filterTree(node.children, patterns) }
      }
      return node
    })
}

/** 将 API 响应的 nodeType 转换为前端 type，并递归处理 children */
function transformApiNode(node: any): FileTreeNode {
  return {
    name: node.name,
    type: node.nodeType === 'folder' ? 'folder' : 'file',
    // 统一路径分隔符为 /，避免 Windows 后端返回 \ 导致混合分隔符
    path: node.path ? node.path.replace(/\\/g, '/') : undefined,
    children: node.children ? node.children.map(transformApiNode) : undefined,
    expanded: node.nodeType === 'folder' ? false : undefined,
  }
}

// ==================== Cache ====================

interface CacheEntry {
  tree: FileTreeNode[]
  timestamp: number
}

const treeCache = new Map<string, CacheEntry>()

// ==================== Composable ====================

export function useFileTree(sessionId: Ref<string>, baseUrl?: Ref<string>) {
  const settings = ref<SidebarSettings>(loadSettings())
  const tree = ref<FileTreeNode[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)
  const isDiffMode = ref(false)

  async function fetchTree() {
    const id = sessionId.value
    if (!id) return

    // 非懒加载 + 非 diff 模式：有缓存则使用缓存
    if (!settings.value.lazyLoad && !isDiffMode.value) {
      const cached = treeCache.get(id)
      if (cached) {
        const filtered = filterTree(cached.tree, settings.value.filterPatterns)
        if (settings.value.defaultExpanded) {
          setAllExpanded(filtered, true)
        }
        tree.value = filtered
        return
      }
    }

    loading.value = true
    error.value = null

    try {
      const { httpGetFileTree, httpGetDiffTree, httpGetFileTreeChildren } = useHttpApi()

      if (isDiffMode.value) {
        // diff 模式始终全量加载
        const result = await httpGetDiffTree(id, settings.value.filterPatterns)
        if (result.code !== 0 || !result.data) {
          throw new Error(result.message || 'mobile.file.fetchDiffTreeFailed')
        }
        const transformed = result.data.tree.map(transformApiNode)
        if (settings.value.defaultExpanded) {
          setAllExpanded(transformed, true)
        }
        tree.value = transformed
      } else if (settings.value.lazyLoad) {
        // 懒加载模式：只获取根目录一层
        const result = await httpGetFileTreeChildren(id, '', settings.value.filterPatterns)
        if (result.code !== 0 || !result.data) {
          throw new Error(result.message || 'mobile.file.fetchTreeFailed')
        }
        const transformed = result.data.children.map(transformApiNode)
        tree.value = transformed
      } else {
        // 全量加载模式
        const result = await httpGetFileTree(id, settings.value.filterPatterns)
        if (result.code !== 0 || !result.data) {
          throw new Error(result.message || 'mobile.file.fetchTreeFailed')
        }
        const transformed = result.data.tree.map(transformApiNode)

        // 写入缓存
        treeCache.set(id, { tree: transformed, timestamp: Date.now() })

        // 应用过滤和展开设置
        const filtered = filterTree(transformed, settings.value.filterPatterns)
        if (settings.value.defaultExpanded) {
          setAllExpanded(filtered, true)
        }
        tree.value = filtered
      }
    } catch (e: any) {
      error.value = e?.toString() || (isDiffMode.value ? 'mobile.file.fetchDiffTreeFailed' : 'mobile.file.fetchTreeFailed')
      tree.value = []
    } finally {
      loading.value = false
    }
  }

  /** 懒加载：展开文件夹时加载其子节点 */
  async function loadChildren(node: FileTreeNode) {
    if (node.type !== 'folder' || node.loading || node.children !== undefined) return

    node.loading = true

    try {
      const { httpGetFileTreeChildren } = useHttpApi()
      const result = await httpGetFileTreeChildren(
        sessionId.value,
        node.path || '',
        settings.value.filterPatterns,
      )
      if (result.code !== 0 || !result.data) {
        throw new Error(result.message || 'mobile.file.loadChildrenFailed')
      }
      const transformed = result.data.children.map(transformApiNode)
      node.children = transformed
    } catch (e: any) {
      // 加载失败时设置 children 为空数组，避免无限重试
      node.children = []
      error.value = e?.toString() || 'mobile.file.loadChildrenFailed'
    } finally {
      node.loading = false
    }
  }

  async function refresh() {
    const id = sessionId.value
    if (id) {
      treeCache.delete(id)
    }
    if (settings.value.lazyLoad && !isDiffMode.value) {
      // 懒加载模式：用 noCache 绕过 HTTP 缓存重新获取根目录
      loading.value = true
      error.value = null
      try {
        const { httpGetFileTreeChildren } = useHttpApi()
        const result = await httpGetFileTreeChildren(id, '', settings.value.filterPatterns, true)
        if (result.code !== 0 || !result.data) {
          throw new Error(result.message || 'mobile.file.fetchTreeFailed')
        }
        const transformed = result.data.children.map(transformApiNode)
        tree.value = transformed
      } catch (e: any) {
        error.value = e?.toString() || 'mobile.file.fetchTreeFailed'
        tree.value = []
      } finally {
        loading.value = false
      }
    } else {
      await fetchTree()
    }
  }

  function expandAll() {
    if (settings.value.lazyLoad) {
      // 懒加载模式：递归加载所有层级
      expandAllLazy(tree.value)
    } else {
      setAllExpanded(tree.value, true)
    }
  }

  function collapseAll() {
    setAllExpanded(tree.value, false)
  }

  /** 懒加载模式下递归展开所有层级 */
  async function expandAllLazy(nodes: FileTreeNode[]) {
    const foldersToLoad = nodes.filter(
      n => n.type === 'folder' && n.children === undefined && !n.loading
    )
    if (foldersToLoad.length === 0) {
      // 所有文件夹已加载，直接展开
      setAllExpanded(nodes, true)
      return
    }

    // 并行加载所有未加载的文件夹
    await Promise.all(foldersToLoad.map(folder => loadChildren(folder)))

    // 递归处理新加载的子节点
    for (const folder of foldersToLoad) {
      if (folder.children && folder.children.length > 0) {
        await expandAllLazy(folder.children)
      }
    }

    // 全部加载完成后统一展开
    setAllExpanded(nodes, true)
  }

  function updateSettings(newSettings: SidebarSettings) {
    settings.value = newSettings
    saveSettings(newSettings)
    // 设置变更后清除缓存重新获取（过滤规则或懒加载模式可能变了）
    const id = sessionId.value
    if (id) {
      treeCache.delete(id)
    }
    fetchTree()
  }

  /** 切换 diff 模式（仅切换标志，不触发 fetchTree，由调用方决定刷新策略） */
  function toggleDiffMode() {
    isDiffMode.value = !isDiffMode.value
  }

  // 监听 sessionId 变化，自动获取文件树
  watch(sessionId, (newId) => {
    if (newId) {
      fetchTree()
    }
  }, { immediate: true })

  return {
    tree,
    loading,
    error,
    isDiffMode,
    expandAll,
    collapseAll,
    refresh,
    toggleDiffMode,
    loadChildren,
    settings,
    updateSettings,
  }
}
