<script setup lang="ts">
/**
 * FileTypeIcon — 按文件扩展名匹配类型图标（模仿常规文件系统）
 *
 * 常用类型（音乐/视频/图片/PDF/文档/表格/演示/压缩包/代码/安装包/光盘镜像）
 * 各配专属线性图标 + chip 底色（宿主 --mobile-chip-* 变量）；未知扩展名回退通用文件图标。
 * 目录固定文件夹图标。图标沿用 heroicons 24 线性描边风格，与宿主 icon-chip 语言一致。
 */
import { computed } from 'vue'

const props = defineProps<{ name: string; isDir: boolean }>()

interface TypeSpec {
  /** 图标类型键（模板分支匹配） */
  kind: string
  /** chip 底色类（宿主 --mobile-chip-* 变量） */
  chip: string
}

/** 类型分组：[kind, chip, 扩展名列表]；保持顺序无关，扩展名查表唯一 */
const TYPE_GROUPS: Array<[string, string, string[]]> = [
  ['audio', 'chip-violet', ['mp3', 'wav', 'flac', 'm4a', 'aac', 'ogg', 'wma', 'ape', 'aiff']],
  ['video', 'chip-amber', ['mp4', 'mkv', 'avi', 'mov', 'wmv', 'flv', 'webm', 'm4v', 'mpg', 'mpeg', 'ts', 'rmvb', '3gp']],
  ['image', 'chip-emerald', ['jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp', 'svg', 'ico', 'heic', 'heif', 'tif', 'tiff', 'raw']],
  ['pdf', 'chip-red', ['pdf']],
  ['doc', 'chip-zinc', ['doc', 'docx', 'txt', 'md', 'rtf', 'odt']],
  ['sheet', 'chip-zinc', ['xls', 'xlsx', 'csv', 'ods']],
  ['slide', 'chip-zinc', ['ppt', 'pptx', 'odp']],
  ['archive', 'chip-zinc', ['zip', 'rar', '7z', 'tar', 'gz', 'bz2', 'xz']],
  ['code', 'chip-zinc', ['js', 'ts', 'jsx', 'tsx', 'json', 'html', 'css', 'py', 'rs', 'go', 'java', 'c', 'cpp', 'h', 'vue', 'sh', 'bash', 'yml', 'yaml', 'toml', 'sql', 'php', 'rb', 'kt', 'swift']],
  ['app', 'chip-zinc', ['exe', 'apk', 'msi', 'dmg', 'deb', 'rpm', 'appimage']],
  ['disc', 'chip-zinc', ['iso', 'img']],
]

/** 扩展名 → 类型查表（小写） */
const EXT_MAP = new Map<string, TypeSpec>()
for (const [kind, chip, exts] of TYPE_GROUPS) {
  for (const ext of exts) EXT_MAP.set(ext, { kind, chip })
}

/** 当前条目的类型（目录优先；未知扩展名回退通用文件） */
const spec = computed<TypeSpec>(() => {
  if (props.isDir) return { kind: 'folder', chip: 'chip-cyan' }
  const ext = props.name.split('.').pop()?.toLowerCase() ?? ''
  return EXT_MAP.get(ext) ?? { kind: 'file', chip: 'chip-zinc' }
})
</script>

<template>
  <span class="icon-chip flex-shrink-0" :class="spec.chip">
    <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <!-- 目录 -->
      <path
        v-if="spec.kind === 'folder'"
        stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
        d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"
      />
      <!-- 通用文件（未知扩展名回退） -->
      <path
        v-else-if="spec.kind === 'file'"
        stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
        d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z"
      />
      <!-- 音乐：音符 + 两个音符头 -->
      <template v-else-if="spec.kind === 'audio'">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 18V5l12-2v13" />
        <circle cx="6" cy="18" r="3" stroke-width="2" />
        <circle cx="18" cy="16" r="3" stroke-width="2" />
      </template>
      <!-- 视频：摄像机 -->
      <path
        v-else-if="spec.kind === 'video'"
        stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
        d="M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z"
      />
      <!-- 图片：照片框 + 山 + 太阳 -->
      <template v-else-if="spec.kind === 'image'">
        <path
          stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
          d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z"
        />
      </template>
      <!-- PDF：文档 + 角标文字 -->
      <template v-else-if="spec.kind === 'pdf'">
        <path
          stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
          d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z"
        />
        <text
          x="12" y="15.8" text-anchor="middle"
          font-size="5.8" font-weight="700" fill="currentColor" stroke="none"
        >PDF</text>
      </template>
      <!-- 文档/文本：带文字行的文档 -->
      <path
        v-else-if="spec.kind === 'doc'"
        stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
        d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
      />
      <!-- 表格：网格 -->
      <path
        v-else-if="spec.kind === 'sheet'"
        stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
        d="M5 4h14a1 1 0 011 1v14a1 1 0 01-1 1H5a1 1 0 01-1-1V5a1 1 0 011-1zM8 4v16M16 4v16M4 8h16M4 16h16"
      />
      <!-- 演示：柱状图 -->
      <path
        v-else-if="spec.kind === 'slide'"
        stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
        d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z"
      />
      <!-- 压缩包：收纳箱 -->
      <template v-else-if="spec.kind === 'archive'">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 8a2 2 0 012-2h12a2 2 0 012 2v10a2 2 0 01-2 2H6a2 2 0 01-2-2V8z" />
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 6V4a1 1 0 011-1h4a1 1 0 011 1v2" />
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 10v4" />
      </template>
      <!-- 代码：尖括号 -->
      <path
        v-else-if="spec.kind === 'code'"
        stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
        d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4"
      />
      <!-- 安装包/可执行：播放键 -->
      <path
        v-else-if="spec.kind === 'app'"
        stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
        d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z"
      />
      <!-- 光盘镜像：光盘 -->
      <template v-else-if="spec.kind === 'disc'">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
      </template>
    </svg>
  </span>
</template>
