<template>
  <aside class="w-64 bg-dark-800 border-r border-dark-700 flex flex-col">
    <!-- Logo -->
    <div class="h-12 px-4 border-b border-dark-700 flex items-center">
      <h1 class="text-lg font-bold text-primary-400">BedCode</h1>
    </div>

    <!-- Navigation -->
    <nav class="flex-1 p-4">
      <ul class="space-y-2">
        <li>
          <router-link
            to="/sessions"
            class="flex items-center gap-3 px-4 py-2 rounded-lg transition-colors"
            :class="[
              $route.path === '/sessions'
                ? 'bg-primary-600 text-white'
                : 'text-dark-300 hover:bg-dark-700 hover:text-white'
            ]"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
            </svg>
            会话管理
          </router-link>
        </li>
        <li>
          <router-link
            to="/devices"
            class="flex items-center gap-3 px-4 py-2 rounded-lg transition-colors"
            :class="[
              $route.path === '/devices'
                ? 'bg-primary-600 text-white'
                : 'text-dark-300 hover:bg-dark-700 hover:text-white'
            ]"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 18h.01M8 21h8a2 2 0 002-2V5a2 2 0 00-2-2H8a2 2 0 00-2 2v14a2 2 0 002 2z" />
            </svg>
            设备配对
          </router-link>
        </li>
        <li>
          <router-link
            to="/settings"
            class="flex items-center gap-3 px-4 py-2 rounded-lg transition-colors"
            :class="[
              $route.path === '/settings'
                ? 'bg-primary-600 text-white'
                : 'text-dark-300 hover:bg-dark-700 hover:text-white'
            ]"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
            </svg>
            设置
          </router-link>
        </li>
      </ul>
    </nav>

    <!-- Status Bar -->
    <div class="p-4 border-t border-dark-700">
      <div class="flex items-center gap-2 text-sm">
        <div class="w-2 h-2 rounded-full bg-green-500"></div>
        <span class="text-dark-400">服务运行中</span>
      </div>
    </div>
  </aside>
</template>
