<template>
  <div class="h-full flex flex-col bg-gray-50 dark:bg-dark-900">
    <!-- Header -->
    <header class="bg-white dark:bg-dark-800 border-b border-gray-200 dark:border-dark-700 px-4 pb-3" style="padding-top: 12px;">
      <h1 class="text-lg font-semibold">历史记录</h1>
    </header>

    <!-- Empty State -->
    <div class="flex-1 flex flex-col items-center justify-center p-4">
      <div class="w-16 h-16 rounded-full bg-white dark:bg-dark-800 flex items-center justify-center mb-4">
        <svg class="w-8 h-8 text-gray- dark:text-dark-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
        </svg>
      </div>
      <p class="text-gray- dark:text-dark-400 text-sm">历史记录功能已移除</p>
      <p class="text-gray- dark:text-dark-500 text-xs mt-2">输入/输出不再记录到数据库</p>
    </div>
  </div>
</template>
