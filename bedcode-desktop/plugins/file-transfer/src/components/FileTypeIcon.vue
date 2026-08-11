<script setup lang="ts">
/**
 * FileTypeIcon — 按文件扩展名匹配类型图标（模仿常规文件系统）
 *
 * 常用类型（音乐/视频/图片/PDF/文档/表格/演示/压缩包/代码/安装包/光盘镜像）
 * 各配专属线性图标 + 展示色（宿主功能色 token）；未知扩展名回退通用文件图标。
 * 目录固定文件夹图标。图标沿用 feather/heroicons 24 线性描边风格（stroke-width 1.5），
 * 与桌面端 .ft-ico 纯色渲染一致。
 */
import { computed } from 'vue'

const props = defineProps<{ name: string; isDir: boolean }>()

interface TypeSpec {
  /** 图标类型键（模板分支匹配） */
  kind: string
  /** 图标展示色（宿主功能色 token） */
  color: string
}

/** 类型分组：[kind, color, 扩展名列表]；保持顺序无关，扩展名查表唯一 */
const TYPE_GROUPS: Array<[string, string, string[]]> = [
  ['audio', 'var(--color-warning)', ['mp3', 'wav', 'flac', 'm4a', 'aac', 'ogg', 'wma', 'ape', 'aiff']],
  ['video', 'var(--color-success)', ['mp4', 'mkv', 'avi', 'mov', 'wmv', 'flv', 'webm', 'm4v', 'mpg', 'mpeg', 'ts', 'rmvb', '3gp']],
  ['image', 'var(--color-primary)', ['jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp', 'svg', 'ico', 'heic', 'heif', 'tif', 'tiff', 'raw']],
  ['pdf', 'var(--color-danger)', ['pdf']],
  ['doc', 'var(--text-secondary)', ['doc', 'docx', 'txt', 'md', 'rtf', 'odt']],
  ['sheet', 'var(--text-secondary)', ['xls', 'xlsx', 'csv', 'ods']],
  ['slide', 'var(--text-secondary)', ['ppt', 'pptx', 'odp']],
  ['archive', 'var(--color-warning)', ['zip', 'rar', '7z', 'tar', 'gz', 'bz2', 'xz']],
  ['code', 'var(--text-secondary)', ['js', 'ts', 'jsx', 'tsx', 'json', 'html', 'css', 'py', 'rs', 'go', 'java', 'c', 'cpp', 'h', 'vue', 'sh', 'bash', 'yml', 'yaml', 'toml', 'sql', 'php', 'rb', 'kt', 'swift']],
  ['app', 'var(--text-secondary)', ['exe', 'apk', 'msi', 'dmg', 'deb', 'rpm', 'appimage']],
  ['disc', 'var(--text-secondary)', ['iso', 'img']],
]

/** 扩展名 → 类型查表（小写） */
const EXT_MAP = new Map<string, TypeSpec>()
for (const [kind, color, exts] of TYPE_GROUPS) {
  for (const ext of exts) EXT_MAP.set(ext, { kind, color })
}

/** 当前条目的类型（目录优先；未知扩展名回退通用文件） */
const spec = computed<TypeSpec>(() => {
  if (props.isDir) return { kind: 'folder', color: 'var(--color-primary)' }
  const ext = props.name.split('.').pop()?.toLowerCase() ?? ''
  return EXT_MAP.get(ext) ?? { kind: 'file', color: 'var(--text-tertiary)' }
})
</script>

<template>
  <span class="ft-ico" :style="{ color: spec.color }">
    <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <!-- 目录 -->
      <path
        v-if="spec.kind === 'folder'"
        stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"
        d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"
      />
      <!-- 通用文件（未知扩展名回退） -->
      <path
        v-else-if="spec.kind === 'file'"
        stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"
        d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8zM14 2v6h6"
      />
      <!-- 音乐：音符 + 两个音符头 -->
      <template v-else-if="spec.kind === 'audio'">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M9 18V5l12-2v13" />
        <circle cx="6" cy="18" r="3" stroke-width="1.5" />
        <circle cx="18" cy="16" r="3" stroke-width="1.5" />
      </template>
      <!-- 视频：摄像机 -->
      <path
        v-else-if="spec.kind === 'video'"
        stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"
        d="M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z"
      />
      <!-- 图片：照片框 + 山 + 太阳 -->
      <path
        v-else-if="spec.kind === 'image'"
        stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"
        d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z"
      />
      <!-- PDF：纯文档轮廓 + 警示红（16px 下文字角标不可读，靠红色与形状区分） -->
      <path
        v-else-if="spec.kind === 'pdf'"
        stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"
        d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8zM14 2v6h6"
      />
      <!-- 文档/文本：带文字行的文档 -->
      <path
        v-else-if="spec.kind === 'doc'"
        stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"
        d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8zM14 2v6h6M16 13H8M16 17H8M10 9H8"
      />
      <!-- 表格：网格 -->
      <path
        v-else-if="spec.kind === 'sheet'"
        stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"
        d="M5 4h14a1 1 0 011 1v14a1 1 0 01-1 1H5a1 1 0 01-1-1V5a1 1 0 011-1zM8 4v16M16 4v16M4 8h16M4 16h16"
      />
      <!-- 演示：柱状图 -->
      <path
        v-else-if="spec.kind === 'slide'"
        stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"
        d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z"
      />
      <!-- 压缩包：收纳箱 -->
      <template v-else-if="spec.kind === 'archive'">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M21 8l-9-5-9 5v8l9 5 9-5V8z" />
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M3 8l9 5 9-5M12 13v8" />
      </template>
      <!-- 代码：尖括号 -->
      <path
        v-else-if="spec.kind === 'code'"
        stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"
        d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4"
      />
      <!-- 安装包/可执行：圆角方块 + 播放键（与视频摄像机形状区分） -->
      <template v-else-if="spec.kind === 'app'">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M7 3h10a4 4 0 014 4v10a4 4 0 01-4 4H7a4 4 0 01-4-4V7a4 4 0 014-4z" />
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M10 9.5v5l4.5-2.5L10 9.5z" />
      </template>
      <!-- 光盘镜像：光盘 -->
      <template v-else-if="spec.kind === 'disc'">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
      </template>
    </svg>
  </span>
</template>
