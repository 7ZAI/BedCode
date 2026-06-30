<template>
  <aside class="w-64 bg-white dark:bg-dark-800 border-r border-slate-200 dark:border-dark-700 flex flex-col">
    <!-- Navigation -->
    <nav class="flex-1 p-4">
      <ul class="space-y-2">
        <li>
          <router-link
            to="/devices"
            class="flex items-center gap-3 px-4 py-2 rounded-lg transition-colors"
            :class="[
              $route.path === '/devices'
                ? 'bg-primary-600 text-white'
                : 'text-slate-600 dark:text-dark-300 hover:bg-slate-100 dark:hover:bg-dark-700 hover:text-slate-900 dark:hover:text-white'
            ]"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 18h.01M8 21h8a2 2 0 002-2V5a2 2 0 00-2-2H8a2 2 0 00-2 2v14a2 2 0 002 2z" />
            </svg>
            {{ $t('desktop.sidebar.devicePairing') }}
          </router-link>
        </li>
        <li>
          <router-link
            to="/sessions"
            class="flex items-center gap-3 px-4 py-2 rounded-lg transition-colors"
            :class="[
              $route.path === '/sessions'
                ? 'bg-primary-600 text-white'
                : 'text-slate-600 dark:text-dark-300 hover:bg-slate-100 dark:hover:bg-dark-700 hover:text-slate-900 dark:hover:text-white'
            ]"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01" />
            </svg>
            {{ $t('desktop.sidebar.sessionConfig') }}
          </router-link>
        </li>
        <li>
          <router-link
            to="/session-manager"
            class="flex items-center gap-3 px-4 py-2 rounded-lg transition-colors"
            :class="[
              $route.path === '/session-manager'
                ? 'bg-primary-600 text-white'
                : 'text-slate-600 dark:text-dark-300 hover:bg-slate-100 dark:hover:bg-dark-700 hover:text-slate-900 dark:hover:text-white'
            ]"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
            </svg>
            {{ $t('desktop.sidebar.sessionManager') }}
          </router-link>
        </li>
    
        <li>
          <router-link
            to="/settings"
            class="flex items-center gap-3 px-4 py-2 rounded-lg transition-colors"
            :class="[
              $route.path === '/settings'
                ? 'bg-primary-600 text-white'
                : 'text-slate-600 dark:text-dark-300 hover:bg-slate-100 dark:hover:bg-dark-700 hover:text-slate-900 dark:hover:text-white'
            ]"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
            </svg>
            {{ $t('desktop.sidebar.settings') }}
          </router-link>
        </li>
      </ul>
    </nav>

    <!-- Status Bar -->
    <div class="p-4 border-t border-slate-200 dark:border-dark-700">
      <div class="flex items-center gap-2 text-sm">
        <div class="w-2 h-2 rounded-full bg-green-500"></div>
        <span class="text-slate-500 dark:text-dark-400">{{ $t('desktop.sidebar.serviceRunning') }}</span>
      </div>
    </div>
  </aside>
</template>
