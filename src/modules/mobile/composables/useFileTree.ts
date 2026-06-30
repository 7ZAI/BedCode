import { ref, watch, type Ref } from 'vue'
import { useHttpApi } from './useHttpApi'

// ==================== Types ====================

export interface FileTreeNode {
  name: string
  type: 'file' | 'folder'
  path?: string // 相对于工作目录的路径
  children?: FileTreeNode[]
  expanded?: boolean // folder only
}

// ==================== Settings ====================

export interface SidebarSettings {
  defaultExpanded: boolean
  filterPatterns: string[]
  fontSize: number // 文件树字体大小 (px)，范围 10-20
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

export function useFileTree(sessionId: Ref<string>) {
  const settings = ref<SidebarSettings>(loadSettings())
  const tree = ref<FileTreeNode[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)
  const isDiffMode = ref(false)

  async function fetchTree() {
    const id = sessionId.value
    if (!id) return

    // 有缓存则使用缓存（非 diff 模式）
    if (!isDiffMode.value) {
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
      const { httpGetFileTree, httpGetDiffTree } = useHttpApi()

      if (isDiffMode.value) {
        const result = await httpGetDiffTree(id, settings.value.filterPatterns)
        if (result.code !== 0 || !result.data) {
          throw new Error(result.message || 'mobile.file.fetchDiffTreeFailed')
        }
        const transformed = result.data.tree.map(transformApiNode)
        if (settings.value.defaultExpanded) {
          setAllExpanded(transformed, true)
        }
        tree.value = transformed
      } else {
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

  async function refresh() {
    const id = sessionId.value
    if (id) {
      treeCache.delete(id)
    }
    await fetchTree()
  }

  function expandAll() {
    setAllExpanded(tree.value, true)
  }

  function collapseAll() {
    setAllExpanded(tree.value, false)
  }

  function updateSettings(newSettings: SidebarSettings) {
    settings.value = newSettings
    saveSettings(newSettings)
    // 设置变更后清除缓存重新获取（过滤规则可能变了）
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
    settings,
    updateSettings,
  }
}
