<template>
  <Teleport to="body">
    <Transition name="overlay">
      <div
        v-if="visible"
        class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
      >
        <div class="bg-[var(--bg-card)] border border-[var(--border)] rounded-xl px-10 py-8 shadow-xl flex flex-col items-center gap-5 min-w-[260px]">
          <!-- 加载动画：呼吸图标 + 双层扩散光环 + 流动加载条 -->
          <div class="loading-overlay-orb">
            <span class="loading-overlay-ring"></span>
            <span class="loading-overlay-ring loading-overlay-ring--delay"></span>
            <span class="loading-overlay-icon">
              <slot name="icon">
                <!-- 默认图标：拼图块（插件/模块加载语义） -->
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M11 4a2 2 0 114 0v1a1 1 0 001 1h3a1 1 0 011 1v3a1 1 0 01-1 1h-1a2 2 0 100 4h1a1 1 0 011 1v3a1 1 0 01-1 1h-3a1 1 0 01-1-1v-1a2 2 0 10-4 0v1a1 1 0 01-1 1H7a1 1 0 01-1-1v-3a1 1 0 00-1-1H4a2 2 0 110-4h1a1 1 0 001-1V7a1 1 0 011-1h3a1 1 0 001-1V4z" />
                </svg>
              </slot>
            </span>
          </div>
          <p v-if="message" class="text-[calc(13px*var(--ui-scale))] font-medium text-[var(--text-primary)]">{{ message }}</p>
          <div class="loading-overlay-bar"><span></span></div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * LoadingOverlay — 通用全屏加载遮罩弹窗
 *
 * 用于需要阻塞交互的异步操作（插件启停、初始化等），
 * 自带呼吸图标 + 扩散光环 + 不定进度条动画，全部 token-bound 跟随主题。
 * 通过 v-model:visible 控制显隐，icon 插槽可替换默认拼图图标。
 *
 * 用法：
 *   <LoadingOverlay v-model:visible="loading" :message="t('...')" />
 */
defineProps<{
  /** 是否显示遮罩（v-model:visible 双向绑定） */
  visible: boolean
  /** 遮罩说明文字（可空，仅显示动画） */
  message?: string
}>()
</script>

<style scoped>
/* ==================== 加载动画 ====================
 * 呼吸图标（1.2s）+ 双层扩散光环（1.8s 交错）+ 不定进度流动条（1.1s） */
.loading-overlay-orb {
  position: relative;
  width: 64px;
  height: 64px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.loading-overlay-icon {
  width: 44px;
  height: 44px;
  border-radius: 12px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--color-primary-light);
  color: var(--color-primary);
  box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--color-primary) 14%, transparent);
  animation: loading-overlay-breathe 1.2s ease-in-out infinite;
}

.loading-overlay-icon svg {
  width: 22px;
  height: 22px;
}

.loading-overlay-ring {
  position: absolute;
  inset: 0;
  border-radius: 50%;
  border: 2px solid var(--color-primary);
  opacity: 0;
  animation: loading-overlay-ring 1.8s cubic-bezier(0.4, 0, 0.2, 1) infinite;
}

.loading-overlay-ring--delay {
  animation-delay: 0.9s;
}

.loading-overlay-bar {
  width: 140px;
  height: 4px;
  border-radius: 2px;
  background: var(--bg-hover);
  overflow: hidden;
}

.loading-overlay-bar span {
  display: block;
  height: 100%;
  width: 38%;
  border-radius: 2px;
  background: linear-gradient(90deg, transparent, var(--color-primary), transparent);
  animation: loading-overlay-bar-flow 1.1s ease-in-out infinite;
}

@keyframes loading-overlay-breathe {
  0%, 100% { transform: scale(1); }
  50% { transform: scale(1.08); }
}

@keyframes loading-overlay-ring {
  0% { transform: scale(0.55); opacity: 0.55; }
  100% { transform: scale(1.55); opacity: 0; }
}

@keyframes loading-overlay-bar-flow {
  0% { transform: translateX(-110%); }
  100% { transform: translateX(360%); }
}
</style>
