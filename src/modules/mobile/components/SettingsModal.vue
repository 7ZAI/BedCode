<template>
  <Teleport to="body">
    <div
      v-if="visible"
      class="fixed inset-0 z-[100] flex items-center justify-center p-4"
      @click.self="emit('close')"
    >
      <div class="absolute inset-0 bg-black/50" @click="emit('close')"></div>
      <div class="relative bg-white dark:bg-dark-800 rounded-xl w-full max-w-sm p-5 shadow-xl">
        <div class="flex items-center justify-between mb-5">
          <span class="font-semibold text-gray-900 dark:text-dark-100 text-lg">输入助手设置</span>
          <button
            class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-dark-700"
            @click="emit('close')"
          >
            <svg class="w-5 h-5 text-gray-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <!-- 大小调节 -->
        <div class="mb-6">
          <div class="flex items-center justify-between mb-2">
            <span class="text-sm font-medium text-gray-700 dark:text-dark-200">悬浮球大小</span>
            <span class="text-sm text-primary-500">{{ localSettings.size }}px</span>
          </div>
          <input
            type="range"
            v-model.number="localSettings.size"
            min="36"
            max="64"
            step="4"
            class="w-full h-2 bg-gray-200 dark:bg-dark-600 rounded-lg appearance-none cursor-pointer accent-primary-500"
          />
          <div class="flex justify-between text-xs text-gray-400 mt-1">
            <span>36px</span>
            <span>64px</span>
          </div>
        </div>

        <!-- 手势开关 -->
        <div class="mb-6">
          <span class="text-sm font-medium text-gray-700 dark:text-dark-200 mb-3 block">手势开关</span>

          <div class="space-y-3">
            <div class="flex items-center justify-between">
              <span class="text-sm text-gray-600 dark:text-dark-300">双击输入</span>
              <button
                class="w-11 h-6 rounded-full transition-colors"
                :class="localSettings.gestures.doubleTap ? 'bg-primary-500' : 'bg-gray-300 dark:bg-dark-600'"
                @click="localSettings.gestures.doubleTap = !localSettings.gestures.doubleTap"
              >
                <span
                  class="block w-5 h-5 bg-white rounded-full shadow transform transition-transform"
                  :class="localSettings.gestures.doubleTap ? 'translate-x-5' : 'translate-x-0.5'"
                ></span>
              </button>
            </div>

            <div class="flex items-center justify-between">
              <span class="text-sm text-gray-600 dark:text-dark-300">向下滑动 - 清屏</span>
              <button
                class="w-11 h-6 rounded-full transition-colors"
                :class="localSettings.gestures.swipeDown ? 'bg-primary-500' : 'bg-gray-300 dark:bg-dark-600'"
                @click="localSettings.gestures.swipeDown = !localSettings.gestures.swipeDown"
              >
                <span
                  class="block w-5 h-5 bg-white rounded-full shadow transform transition-transform"
                  :class="localSettings.gestures.swipeDown ? 'translate-x-5' : 'translate-x-0.5'"
                ></span>
              </button>
            </div>

            <div class="flex items-center justify-between">
              <span class="text-sm text-gray-600 dark:text-dark-300">向上滑动 - Ctrl+C</span>
              <button
                class="w-11 h-6 rounded-full transition-colors"
                :class="localSettings.gestures.swipeUp ? 'bg-primary-500' : 'bg-gray-300 dark:bg-dark-600'"
                @click="localSettings.gestures.swipeUp = !localSettings.gestures.swipeUp"
              >
                <span
                  class="block w-5 h-5 bg-white rounded-full shadow transform transition-transform"
                  :class="localSettings.gestures.swipeUp ? 'translate-x-5' : 'translate-x-0.5'"
                ></span>
              </button>
            </div>

            <div class="flex items-center justify-between">
              <span class="text-sm text-gray-600 dark:text-dark-300">向左滑动 - 快捷键</span>
              <button
                class="w-11 h-6 rounded-full transition-colors"
                :class="localSettings.gestures.swipeLeft ? 'bg-primary-500' : 'bg-gray-300 dark:bg-dark-600'"
                @click="localSettings.gestures.swipeLeft = !localSettings.gestures.swipeLeft"
              >
                <span
                  class="block w-5 h-5 bg-white rounded-full shadow transform transition-transform"
                  :class="localSettings.gestures.swipeLeft ? 'translate-x-5' : 'translate-x-0.5'"
                ></span>
              </button>
            </div>

            <div class="flex items-center justify-between">
              <span class="text-sm text-gray-600 dark:text-dark-300">向右滑动 - 输入</span>
              <button
                class="w-11 h-6 rounded-full transition-colors"
                :class="localSettings.gestures.swipeRight ? 'bg-primary-500' : 'bg-gray-300 dark:bg-dark-600'"
                @click="localSettings.gestures.swipeRight = !localSettings.gestures.swipeRight"
              >
                <span
                  class="block w-5 h-5 bg-white rounded-full shadow transform transition-transform"
                  :class="localSettings.gestures.swipeRight ? 'translate-x-5' : 'translate-x-0.5'"
                ></span>
              </button>
            </div>
          </div>
        </div>

        <!-- 恢复默认按钮 -->
        <button
          class="w-full py-2.5 text-sm text-gray-500 dark:text-dark-400 border border-gray-300 dark:border-dark-600 rounded-lg hover:bg-gray-50 dark:hover:bg-dark-700 transition-colors"
          @click="handleReset"
        >
          恢复默认设置
        </button>

        <!-- 保存按钮 -->
        <button
          class="w-full mt-3 py-2.5 text-sm font-medium text-white bg-primary-500 rounded-lg hover:bg-primary-600 transition-colors"
          @click="handleSave"
        >
          保存设置
        </button>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { useInputAssistantStore } from '@/modules/shared/stores/inputAssistant'

const props = defineProps<{
  visible: boolean
}>()

const emit = defineEmits<{
  close: []
}>()

const store = useInputAssistantStore()

// 本地设置副本
const localSettings = ref({
  size: store.settings.size,
  gestures: { ...store.settings.gestures }
})

// 监听弹窗打开，同步设置
watch(() => props.visible, (show) => {
  if (show) {
    localSettings.value = {
      size: store.settings.size,
      gestures: { ...store.settings.gestures }
    }
  }
})

function handleSave() {
  store.saveSettings(localSettings.value)
  emit('close')
}

function handleReset() {
  store.resetSettings()
  localSettings.value = {
    size: 48,
    gestures: {
      doubleTap: true,
      swipeDown: true,
      swipeUp: true,
      swipeLeft: true,
      swipeRight: true,
    }
  }
}
</script>