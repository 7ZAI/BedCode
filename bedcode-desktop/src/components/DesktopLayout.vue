<template>
  <!-- 插件独立窗口路由（终端窗口 / 通用插件窗口）：不显示任何布局元素 -->
  <template v-if="isPluginWindow">
    <router-view />
  </template>

  <!-- 普通路由：显示完整布局 -->
  <template v-else>
    <div class="flex flex-col h-screen desktop-ui">
      <!-- Custom Title Bar -->
      <TitleBar />
      <div class="flex flex-1 overflow-hidden">
        <!-- Sidebar -->
        <Sidebar />

        <!-- Main Content：过渡容器必须带 page-swap（页面过渡体系的定位上下文，
             效果变体与时长 token 见 src/style.css「页面过渡动效」节） -->
        <main class="page-swap flex-1 overflow-hidden bg-page">
          <router-view v-slot="{ Component }">
            <!-- 不加 mode="out-in"：out-in 在「旧页退场完成」与「新页入场挂载」之间会
                 留出一帧以上的空容器窗口，露出近黑的 --bg-page，即黑屏 + 闪烁。默认
                 重叠模式下旧页作为绝对定位层退场、新页从第一帧就占位，容器永不断层。 -->
            <Transition name="page">
              <!-- 路由页面 KeepAlive：切换路由不销毁插件视图（AI 对话等插件页面保活——
                   切走时流式监听继续、切回保留离开时画面）；:key=fullPath 配合缓存：
                   同一路径命中同一实例，路由参数变化（插件 A→B）仍重建。max 限制
                   缓存总量（LRU 淘汰，防长时间使用后内存无限增长） -->
              <KeepAlive :max="8">
                <component :is="Component" :key="$route.fullPath" />
              </KeepAlive>
            </Transition>
          </router-view>
        </main>

        <PluginStatusBar />
      </div>
    </div>
    <PluginCommandPalette />
    <!-- 插件全局弹窗（SDK 共享组件：预设/组件两模式、定时关闭、按钮跳转） -->
    <PluginGlobalDialog />
  </template>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import TitleBar from '@/components/TitleBar.vue'
import Sidebar from '@/components/Sidebar.vue'
import PluginCommandPalette from '@/plugin/components/PluginCommandPalette.vue'
import PluginStatusBar from '@/plugin/components/PluginStatusBar.vue'
// SDK 共享全局弹窗宿主（经 packages 下文件路径 import，见 vite fs.allow 注释）
import PluginGlobalDialog from '@binblink/bedcode-plugin-sdk-desktop/ui/plugin-global-dialog'

const route = useRoute()

// 判据取路由 meta 而非路径前缀：新增的插件独立窗口路由自动获得「裸窗口」语义，
// 不必在每个宿主布局文件里再同步一遍路径清单
const isPluginWindow = computed(() => route.meta.bareWindow === true)
</script>
