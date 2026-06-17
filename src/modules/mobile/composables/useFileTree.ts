import { ref } from 'vue'

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

// ==================== Test Data ====================

const TEST_TREE: FileTreeNode[] = [
  {
    name: 'src',
    type: 'folder',
    expanded: true,
    children: [
      { name: 'main.rs', type: 'file' },
      { name: 'app.rs', type: 'file' },
      {
        name: 'commands',
        type: 'folder',
        expanded: false,
        children: [
          { name: 'session.rs', type: 'file' },
          { name: 'config.rs', type: 'file' },
        ],
      },
      {
        name: 'utils',
        type: 'folder',
        expanded: false,
        children: [
          { name: 'helpers.rs', type: 'file' },
        ],
      },
    ],
  },
  { name: 'package.json', type: 'file' },
  { name: 'Cargo.toml', type: 'file' },
  { name: 'README.md', type: 'file' },
]

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
  'app.rs': `use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::start_session,
            commands::stop_session,
            commands::list_sessions,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}`,
  'session.rs': `use crate::shared::system::error::Result;

#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub name: String,
    pub status: String,
}

#[tauri::command]
pub async fn start_session(name: String) -> Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    tracing::info!("Session created: {} ({})", name, id);
    Ok(id)
}`,
  'config.rs': `use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct AppConfig {
    pub theme: String,
    pub font_size: u32,
    pub shell: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            font_size: 14,
            shell: "/bin/bash".to_string(),
        }
    }
}`,
  'helpers.rs': `use std::path::Path;

pub fn ensure_dir(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}

pub fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        &s[..s.char_indices().take(max_len).last().map(|(i, _)| i).unwrap_or(0)]
    }
}`,
  'package.json': `{
  "name": "bedcode",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vue-tsc --noEmit && vite build",
    "tauri:dev": "tauri dev",
    "tauri:build": "tauri build"
  },
  "dependencies": {
    "@tauri-apps/api": "^2.11.0",
    "vue": "^3.4.21",
    "pinia": "^2.1.7"
  }
}`,
  'Cargo.toml': `[package]
name = "bedcode"
version = "0.1.0"
description = "Cross-platform remote terminal"
authors = ["binblink"]
edition = "2021"

[dependencies]
tauri = { version = "2", features = [] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "14", features = ["v4"] }
tracing = "0.1"
tracing-subscriber = "0.3"

[build-dependencies]
tauri-build = { version = "2" }`,
  'README.md': `# BedCode

Cross-platform remote terminal application.

## Features

- Remote terminal control from mobile devices
- WebSocket-based real-time communication
- Multi-session support
- Cross-platform (Desktop + Mobile)

## Development

\`\`\`bash
npm run tauri:dev
\`\`\`

## Build

\`\`\`bash
npm run tauri:build
\`\`\``,
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

// ==================== Composable ====================

export function useFileTree() {
  const settings = ref<SidebarSettings>(loadSettings())
  // 深拷贝测试数据，根据设置决定初始展开状态
  const rawTree = JSON.parse(JSON.stringify(TEST_TREE))
  if (settings.value.defaultExpanded) {
    setAllExpanded(rawTree, true)
  }
  const tree = ref<FileTreeNode[]>(filterTree(rawTree, settings.value.filterPatterns))

  function expandAll() {
    setAllExpanded(tree.value, true)
  }

  function collapseAll() {
    setAllExpanded(tree.value, false)
  }

  function refresh() {
    const fresh = JSON.parse(JSON.stringify(TEST_TREE))
    if (settings.value.defaultExpanded) {
      setAllExpanded(fresh, true)
    }
    tree.value = filterTree(fresh, settings.value.filterPatterns)
  }

  function updateSettings(newSettings: SidebarSettings) {
    settings.value = newSettings
    saveSettings(newSettings)
    refresh()
  }

  return {
    tree,
    expandAll,
    collapseAll,
    refresh,
    settings,
    updateSettings,
  }
}
