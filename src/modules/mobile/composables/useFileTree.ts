import { ref, watch, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

// ==================== Types ====================

export interface FileTreeNode {
  name: string
  type: 'file' | 'folder'
  children?: FileTreeNode[]
  expanded?: boolean // folder only
}

// ==================== Settings ====================

export interface SidebarSettings {
  defaultExpanded: boolean
  filterPatterns: string[]
}

const SETTINGS_KEY = 'bedcode:sidebar-settings'

const DEFAULT_SETTINGS: SidebarSettings = {
  defaultExpanded: false,
  filterPatterns: ['node_modules', 'target', '.git', 'dist', 'build'],
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

// ==================== Test File Contents (for FileViewerModal) ====================

export const TEST_FILE_CONTENTS: Record<string, string> = {
  'main.rs': `use std::io;

fn main() {
    println!("Hello, BedCode!");

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .expect("Failed to read line");

    println!("You entered: {}", input.trim());
}

struct Session {
    id: String,
    name: String,
    active: bool,
}

impl Session {
    fn new(id: &str, name: &str) -> Self {
        Session {
            id: id.to_string(),
            name: name.to_string(),
            active: true,
        }
    }
}`,
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

  async function fetchTree() {
    const id = sessionId.value
    if (!id) return

    // 有缓存则使用缓存
    const cached = treeCache.get(id)
    if (cached) {
      const filtered = filterTree(cached.tree, settings.value.filterPatterns)
      if (settings.value.defaultExpanded) {
        setAllExpanded(filtered, true)
      }
      tree.value = filtered
      return
    }

    loading.value = true
    error.value = null

    try {
      const rawNodes = await invoke<unknown[]>('http_get_file_tree', {
        sessionId: id,
        excludeDirs: settings.value.filterPatterns,
      })

      const transformed = rawNodes.map(transformApiNode)

      // 写入缓存
      treeCache.set(id, { tree: transformed, timestamp: Date.now() })

      // 应用过滤和展开设置
      const filtered = filterTree(transformed, settings.value.filterPatterns)
      if (settings.value.defaultExpanded) {
        setAllExpanded(filtered, true)
      }
      tree.value = filtered
    } catch (e: any) {
      error.value = e?.toString() || '获取文件树失败'
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
    expandAll,
    collapseAll,
    refresh,
    settings,
    updateSettings,
  }
}
