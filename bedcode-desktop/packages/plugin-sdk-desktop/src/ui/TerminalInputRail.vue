<script setup lang="ts">
/**
 * 终端输入导航条 — SDK 共享 UI 组件（桌面端终端右侧悬浮）
 *
 * 一根横线 = 一次用户输入，横线垂直位置映射该输入在终端 buffer 中的相对位置（
 * 全量 buffer 压缩进条带内，非视口映射）。
 * 状态：
 * - 默认态：右侧竖直居中、固定高度（h-56）一列主题色横线，背景全透明（不遮挡终端内容）
 * - 展开态：鼠标移入 → 背景卡片 + 输入列表（每条输入一行：截断文本 + 右侧横线，
 *   文字与横线同一水平线；点击行/横线高亮选中并 emit navigate）
 * - 无输入 / alternate buffer（TUI 全屏程序）时整体隐藏
 * 点击横线/列表行 → emit navigate(line)，父组件执行 terminal.scrollToLine。
 *
 * 用法：
 * ```vue
 * <TerminalInputRail
 *   :markers="visibleMarkers"
 *   :buffer-length="bufferLength"
 *   :is-alt-buffer="isAltBuffer"
 *   @navigate="(line) => terminal?.scrollToLine(line)"
 * />
 * ```
 */
import { computed, ref, watch } from 'vue'
/** 一次用户输入的位置标记（宿主端由 useTerminalInputMarkers 等 composable 产出，结构相同） */
export interface InputMarker {
  id: number
  /** buffer 绝对行号（xterm 坐标，随 scrollback 淘汰自动校正，-1 = 已淘汰） */
  line: number
  /** 输入文本（不含提示符；多行粘贴取首行） */
  text: string
}

interface Props {
  /** 输入标记（已过滤 line >= 0，按时间正序，最多 maxMarkers 条） */
  markers: InputMarker[]
  /** 当前 buffer 总行数（横线位置百分比 = line / (bufferLength - 1)） */
  bufferLength: number
  /** alternate buffer（vim 等 TUI 全屏程序）时隐藏 */
  isAltBuffer: boolean
}

const props = withDefaults(defineProps<Props>(), {
  bufferLength: 0,
  isAltBuffer: false,
})

const emit = defineEmits<{
  (e: 'navigate', line: number): void
}>()

const expanded = ref(false)
/** 当前选中（点击导航的）输入行 id，用于选中高亮态 */
const selectedId = ref<number | null>(null)

// 无输入时重置展开态与选中态，避免会话切换后残留展开卡片
watch(
  () => props.markers.length,
  (len) => {
    if (len === 0) {
      expanded.value = false
      selectedId.value = null
    }
  },
)

/** 点击横线/列表行：记录选中 → 通知父组件滚动到该输入行 */
function navigate(m: PositionedMarker) {
  selectedId.value = m.id
  emit('navigate', m.line)
}

/** 相邻横线最小间距（容器高度百分比，约 4px）：间距过近时保留较新的那条，避免重叠 */
const MIN_GAP_PERCENT = 2

interface PositionedMarker extends InputMarker {
  top: number
}

/** 计算每个标记的垂直位置（0-100%）并过滤重叠项（保留较新） */
const positionedMarkers = computed<PositionedMarker[]>(() => {
  const len = Math.max(props.bufferLength - 1, 1)
  const items = props.markers.map((m) => ({ ...m, top: (m.line / len) * 100 }))
  // 从最新往回遍历：与已保留的较新一项间距不足时丢弃（旧标记被新输出"挤"在一起）
  const kept: PositionedMarker[] = []
  for (let i = items.length - 1; i >= 0; i--) {
    const item = items[i]
    const newer = kept[kept.length - 1]
    if (newer && newer.top - item.top < MIN_GAP_PERCENT) continue
    kept.push(item)
  }
  return kept.reverse()
})
</script>

<template>
  <div
    v-if="markers.length > 0 && !isAltBuffer"
    :class="expanded ? 'w-[284px]' : 'w-[18px]'"
    class="absolute right-0 top-1/2 -translate-y-1/2 z-20 h-56 max-h-full"
    @mouseenter="expanded = true"
    @mouseleave="expanded = false"
  >
    <!-- 横线轨道：仅默认态渲染（收起时的位置映射条带），展开后由行内横线替代 -->
    <div v-if="!expanded" class="absolute inset-0">
      <button
        v-for="m in positionedMarkers"
        :key="'line-' + m.id"
        class="rail-line"
        :style="{ top: `min(max(${m.top}%, 2px), calc(100% - 2px))` }"
        :title="`$ ${m.text}`"
        @click="navigate(m)"
      />
    </div>

    <Transition name="rail" mode="out-in">
      <!-- 展开态：背景卡片 + 输入列表，每行 = 截断文本 + 右侧主题色横线（同一水平线） -->
      <div
        v-if="expanded"
        class="absolute right-1 inset-y-0 w-[280px] max-w-[60%] bg-card border border-[var(--border)] rounded-card shadow-card p-2 overflow-y-auto"
      >
        <div
          v-for="m in positionedMarkers"
          :key="'row-' + m.id"
          class="group flex items-center gap-2 px-2 py-1 rounded-btn text-xs cursor-pointer transition-colors duration-200 hover:bg-[var(--bg-hover)]"
          :class="selectedId === m.id ? 'bg-[color-mix(in_srgb,var(--color-primary)_15%,transparent)]' : ''"
          :title="`$ ${m.text}`"
          @click="navigate(m)"
        >
          <span
            class="font-mono truncate min-w-0 flex-1 transition-colors duration-200"
            :class="selectedId === m.id ? 'text-[var(--text-primary)]' : 'text-[var(--text-secondary)]'"
          >$ {{ m.text }}</span>
          <span
            class="w-[10px] h-[2px] rounded-full bg-[var(--color-primary)] flex-shrink-0 transition-opacity duration-200"
            :class="selectedId === m.id ? 'opacity-100' : 'opacity-60 group-hover:opacity-100'"
          />
        </div>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
/* 默认态横线：主题色，垂直位置映射 buffer 行，透明背景下仅此可见 */
.rail-line {
  position: absolute;
  right: 4px;
  width: 10px;
  height: 2px;
  border: none;
  border-radius: 9999px;
  background: var(--color-primary);
  opacity: 0.45;
  cursor: pointer;
  transform: translateY(-50%);
  transition: opacity 0.2s ease;
}

.rail-line:hover {
  opacity: 1;
}

/* 默认态 ↔ 展开态切换：fade + 轻微横向位移（GPU 合成属性） */
.rail-enter-active,
.rail-leave-active {
  transition: opacity 0.2s ease, transform 0.2s ease;
}

.rail-enter-from,
.rail-leave-to {
  opacity: 0;
  transform: translateX(8px);
}
</style>