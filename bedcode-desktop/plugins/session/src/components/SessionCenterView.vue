<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- ==================== 工具栏页头：左标题，右刷新/新建 ==================== -->
    <div class="wb-toolbar">
      <div class="flex items-center gap-2.5">
        <svg
          class="w-4 h-4 text-[var(--text-secondary)]"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="1.5"
            d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"
          />
        </svg>
        <h2 class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">
          {{ t('session.sidebar.title') }}
        </h2>
      </div>
      <div class="flex items-center gap-2">
        <button class="wb-btn-ghost" @click="refresh">
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.75"
              d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
            />
          </svg>
          {{ t('session.button.refresh') }}
        </button>
        <button class="wb-btn-primary" @click="openCreateDialog">
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.75"
              d="M12 4v16m8-8H4"
            />
          </svg>
          {{ t('session.form.title.new') }}
        </button>
      </div>
    </div>

    <!-- ==================== Tab 切换：终端配置 / 运行中的会话 ==================== -->
    <div class="px-6 pt-3 flex-shrink-0">
      <div class="flex items-center gap-1 p-1 rounded-lg bg-[var(--bg-hover)]">
        <button
          v-for="tab in sessionTabs"
          :key="tab.key"
          class="h-8 flex-1 px-4 rounded-md text-[calc(12px*var(--ui-scale))] font-medium transition-colors duration-200"
          :class="
            activeTab === tab.key
              ? 'bg-[var(--bg-card)] text-[var(--text-primary)] shadow-sm'
              : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
          "
          @click="activeTab = tab.key"
        >
          {{ tab.label }}
        </button>
      </div>
    </div>

    <!-- 内容区：按功能分 tab -->
    <div class="flex-1 overflow-auto px-6 py-6">
      <div class="max-w-5xl mx-auto">
        <!-- Loading -->
        <div v-if="isLoading" class="flex flex-col items-center justify-center py-20">
          <svg
            class="w-5 h-5 animate-spin text-[var(--text-secondary)] mb-3"
            fill="none"
            viewBox="0 0 24 24"
          >
            <circle
              class="opacity-25"
              cx="12"
              cy="12"
              r="10"
              stroke="currentColor"
              stroke-width="2"
            ></circle>
            <path
              class="opacity-75"
              fill="currentColor"
              d="M4 12a8 8 0 018-8v2a6 6 0 00-6 6H4z"
            ></path>
          </svg>
          <p class="wb-mono text-xs text-[var(--text-secondary)]">
            {{ t('session.operating.processing') }}
          </p>
        </div>

        <Transition name="tab-fade" mode="out-in">
          <!-- Tab1：终端配置 -->
          <div v-if="activeTab === 'configs'" class="space-y-6">
            <!-- Empty -->
            <div
              v-if="configs.length === 0"
              class="flex flex-col items-center justify-center py-20"
            >
              <p class="text-sm text-[var(--text-primary)]">{{ t('session.empty.noConfig') }}</p>
              <p class="text-xs text-[var(--text-secondary)] mt-1">
                {{ t('session.empty.noConfigHint') }}
              </p>
              <button class="wb-btn-primary mt-5 h-8 px-4" @click="openCreateDialog">
                <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="1.75"
                    d="M12 4v16m8-8H4"
                  />
                </svg>
                {{ t('session.form.title.new') }}
              </button>
            </div>

            <!-- Section：配置 -->
            <section v-else>
              <h3 class="wb-section-title">
                {{ t('session.section.configs', { count: configs.length }) }}
              </h3>
              <div class="grid grid-cols-1 2xl:grid-cols-2 gap-4">
                <div
                  v-for="config in configs"
                  :key="config.id"
                  class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] p-4 hover:shadow-sm transition-shadow"
                >
                  <!-- 卡片头：名称 + 环境/自启动标签 -->
                  <div class="flex items-center gap-2">
                    <h4 class="text-sm font-semibold text-[var(--text-primary)] truncate flex-1">
                      {{ config.name }}
                    </h4>
                    <span
                      class="wb-mono text-[calc(10.5px*var(--ui-scale))] uppercase px-1.5 py-0.5 rounded border border-[var(--border-strong)] text-[var(--text-secondary)] flex-shrink-0"
                    >
                      {{ envBadge(config.environment) }}
                    </span>
                    <span
                      v-if="config.autoStart"
                      class="wb-mono text-[calc(10.5px*var(--ui-scale))] uppercase px-1.5 py-0.5 rounded border border-[var(--border-strong)] text-[var(--text-secondary)] flex-shrink-0"
                      >auto</span
                    >
                  </div>

                  <!-- 技术值：路径 + 命令，mono -->
                  <div class="mt-3 space-y-1.5 min-w-0">
                    <div class="flex items-center gap-2 text-[var(--text-secondary)] min-w-0">
                      <svg
                        class="w-3.5 h-3.5 flex-shrink-0"
                        fill="none"
                        stroke="currentColor"
                        viewBox="0 0 24 24"
                      >
                        <path
                          stroke-linecap="round"
                          stroke-linejoin="round"
                          stroke-width="1.5"
                          d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"
                        />
                      </svg>
                      <span class="wb-mono truncate" :title="config.workingDir">{{
                        config.workingDir || '—'
                      }}</span>
                    </div>
                    <div class="flex items-center gap-2 text-[var(--text-secondary)] min-w-0">
                      <svg
                        class="w-3.5 h-3.5 flex-shrink-0"
                        fill="none"
                        stroke="currentColor"
                        viewBox="0 0 24 24"
                      >
                        <path
                          stroke-linecap="round"
                          stroke-linejoin="round"
                          stroke-width="1.5"
                          d="M8 9l3 3-3 3m5 0h3"
                        />
                      </svg>
                      <span class="wb-mono truncate" :title="config.command">{{
                        config.command || '—'
                      }}</span>
                    </div>
                  </div>

                  <!-- 该配置下的会话 -->
                  <div
                    v-if="sessionsOf(config.id).length > 0"
                    class="mt-3 pt-3 border-t border-[var(--border)] space-y-1"
                  >
                    <div
                      v-for="session in sessionsOf(config.id)"
                      :key="session.id"
                      class="flex items-center gap-2 text-xs rounded-[6px] px-1.5 py-1 -mx-1.5 cursor-pointer hover:bg-[var(--bg-hover)] transition-colors"
                      :title="t('session.terminal.view')"
                      @click="viewSession(session)"
                    >
                      <span
                        :class="[
                          'w-1.5 h-1.5 rounded-full flex-shrink-0',
                          statusDot(session.status),
                        ]"
                      ></span>
                      <span class="text-[var(--text-primary)] truncate">{{ session.name }}</span>
                      <span
                        class="wb-mono text-[calc(11px*var(--ui-scale))] text-[var(--text-secondary)] flex-shrink-0"
                      >
                        {{
                          isRunning(session.status)
                            ? runTimeText(session)
                            : formatDateTime(session.startedAt || session.createdAt || '')
                        }}
                      </span>
                      <span class="flex-1"></span>
                      <button
                        class="w-6 h-6 rounded-[6px] flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors flex-shrink-0"
                        :title="t('session.terminal.view')"
                        @click.stop="viewSession(session)"
                      >
                        <svg
                          class="w-3.5 h-3.5"
                          fill="none"
                          stroke="currentColor"
                          viewBox="0 0 24 24"
                        >
                          <path
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            stroke-width="1.5"
                            d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"
                          />
                        </svg>
                      </button>
                      <button
                        class="w-6 h-6 rounded-[6px] flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors flex-shrink-0"
                        :title="t('session.terminal.restart')"
                        @click.stop="doRestart(session)"
                      >
                        <svg
                          class="w-3.5 h-3.5"
                          fill="none"
                          stroke="currentColor"
                          viewBox="0 0 24 24"
                        >
                          <path
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            stroke-width="1.5"
                            d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
                          />
                        </svg>
                      </button>
                      <button
                        class="w-6 h-6 rounded-[6px] flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors flex-shrink-0"
                        :title="t('session.button.stop')"
                        @click.stop="confirmStopSession(session)"
                      >
                        <svg
                          class="w-3.5 h-3.5"
                          fill="none"
                          stroke="currentColor"
                          viewBox="0 0 24 24"
                        >
                          <path
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            stroke-width="1.5"
                            d="M6 6h12v12H6z"
                          />
                        </svg>
                      </button>
                      <button
                        class="w-6 h-6 rounded-[6px] flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-red-600 dark:hover:text-red-400 transition-colors flex-shrink-0"
                        :title="t('session.button.delete')"
                        @click.stop="confirmDeleteSession(session)"
                      >
                        <svg
                          class="w-3.5 h-3.5"
                          fill="none"
                          stroke="currentColor"
                          viewBox="0 0 24 24"
                        >
                          <path
                            stroke-linecap="round"
                            stroke-linejoin="round"
                            stroke-width="1.5"
                            d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
                          />
                        </svg>
                      </button>
                    </div>
                  </div>

                  <!-- 卡片底部操作 -->
                  <div class="mt-3 pt-3 border-t border-[var(--border)] flex items-center gap-2">
                    <button class="wb-btn-primary" @click="doStart(config.id)">
                      <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path
                          stroke-linecap="round"
                          stroke-linejoin="round"
                          stroke-width="1.75"
                          d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z"
                        />
                      </svg>
                      {{ t('session.button.start') }}
                    </button>
                    <span class="flex-1"></span>
                    <span
                      v-if="runningOf(config.id).length > 0"
                      class="wb-mono text-[calc(11px*var(--ui-scale))] text-green-600 dark:text-green-400 flex items-center gap-1.5"
                    >
                      <span class="w-1.5 h-1.5 rounded-full bg-green-500 animate-pulse"></span>
                      {{ runningOf(config.id).length }}
                    </span>
                    <button class="wb-btn-ghost" @click="openEditDialog(config)">
                      {{ t('session.button.edit') }}
                    </button>
                    <button
                      class="wb-btn-ghost hover:!text-red-600 dark:hover:!text-red-400"
                      @click="confirmDeleteConfig(config.id)"
                    >
                      {{ t('session.button.delete') }}
                    </button>
                  </div>
                </div>
              </div>
            </section>
          </div>

          <!-- Tab2：运行中的会话（跨配置汇总，操作与配置卡片内会话一致） -->
          <div v-else class="space-y-6">
            <h3 class="wb-section-title">
              {{ t('session.section.running', { count: runningSessions.length }) }}
            </h3>
            <div
              v-if="runningSessions.length === 0"
              class="flex flex-col items-center justify-center py-20"
            >
              <p class="text-sm text-[var(--text-primary)]">
                {{ t('session.empty.noSessions') }}
              </p>
              <p class="text-xs text-[var(--text-secondary)] mt-1">
                {{ t('session.empty.noSessionsHint') }}
              </p>
            </div>
            <div
              v-else
              class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] divide-y divide-[var(--border)]"
            >
              <div
                v-for="session in runningSessions"
                :key="session.id"
                class="flex items-center gap-3 px-4 h-12"
              >
                <span
                  :class="['w-2 h-2 rounded-full flex-shrink-0', statusDot(session.status)]"
                ></span>
                <span
                  class="text-xs font-medium text-[var(--text-primary)] truncate cursor-pointer hover:underline"
                  @click="viewSession(session)"
                  >{{ session.name }}</span
                >
                <span
                  class="text-[calc(11px*var(--ui-scale))] text-[var(--text-secondary)] flex-shrink-0"
                  >{{ statusText(session.status) }}</span
                >
                <span class="flex-1"></span>
                <span
                  class="wb-mono text-[calc(11.5px*var(--ui-scale))] text-[var(--text-secondary)] flex-shrink-0"
                  >{{ runTimeText(session) }}</span
                >
                <!-- 查看终端 -->
                <button
                  class="w-7 h-7 rounded-[6px] flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors flex-shrink-0"
                  :title="t('session.terminal.view')"
                  @click.stop="viewSession(session)"
                >
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      stroke-width="1.5"
                      d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"
                    />
                  </svg>
                </button>
                <!-- 重启 -->
                <button
                  class="w-7 h-7 rounded-[6px] flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors flex-shrink-0"
                  :title="t('session.terminal.restart')"
                  @click.stop="doRestart(session)"
                >
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      stroke-width="1.5"
                      d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
                    />
                  </svg>
                </button>
                <!-- 停止 -->
                <button
                  class="w-7 h-7 rounded-[6px] flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors flex-shrink-0"
                  :title="t('session.button.stop')"
                  @click.stop="confirmStopSession(session)"
                >
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      stroke-width="1.5"
                      d="M6 6h12v12H6z"
                    />
                  </svg>
                </button>
                <!-- 删除 -->
                <button
                  class="w-7 h-7 rounded-[6px] flex items-center justify-center text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-red-600 dark:hover:text-red-400 transition-colors flex-shrink-0"
                  :title="t('session.button.delete')"
                  @click.stop="confirmDeleteSession(session)"
                >
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      stroke-width="1.5"
                      d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
                    />
                  </svg>
                </button>
              </div>
            </div>
          </div>
        </Transition>
      </div>
    </div>

    <!-- 创建/编辑对话框 -->
    <PluginModal
      v-model="showCreateDialog"
      :title="editingConfig ? t('session.form.title.edit') : t('session.form.title.new')"
      size="lg"
    >
      <SessionConfigForm
        ref="sessionFormRef"
        :config="editingConfig"
        :defaults="formDefaults"
        @save="handleSaveConfig"
      />
      <template #footer>
        <div class="flex justify-end gap-3">
          <button class="wb-btn-ghost" @click="showCreateDialog = false">
            {{ t('session.button.cancel') }}
          </button>
          <button class="wb-btn-primary" @click="submitForm">
            {{ editingConfig ? t('session.button.save') : t('session.button.create') }}
          </button>
        </div>
      </template>
    </PluginModal>

    <!-- 删除配置确认 -->
    <PluginModal
      v-model="showDeleteConfirmDialog"
      :title="t('session.confirm.deleteConfigTitle')"
      size="sm"
    >
      <p class="text-[var(--text-primary)] text-[calc(13px*var(--ui-scale))]">
        {{ t('session.confirm.deleteConfigMsg') }}
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <button class="wb-btn-ghost" @click="showDeleteConfirmDialog = false">
            {{ t('session.button.cancel') }}
          </button>
          <button class="wb-btn-primary bg-[var(--color-danger)]" @click="confirmDeleteConfigNow">
            {{ t('session.button.delete') }}
          </button>
        </div>
      </template>
    </PluginModal>

    <!-- 停止会话确认 -->
    <PluginModal v-model="showStopConfirmDialog" :title="t('session.confirm.stopTitle')" size="sm">
      <p class="text-[var(--text-primary)] text-[calc(13px*var(--ui-scale))]">
        {{ t('session.confirm.stopMsg', { name: pendingSession?.name }) }}
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <button class="wb-btn-ghost" @click="showStopConfirmDialog = false">
            {{ t('session.button.cancel') }}
          </button>
          <button
            class="wb-btn-primary bg-[var(--color-danger)]"
            :disabled="isOperating"
            @click="confirmStop"
          >
            {{ t('session.button.stop') }}
          </button>
        </div>
      </template>
    </PluginModal>

    <!-- 删除会话确认 -->
    <PluginModal
      v-model="showDeleteSessionConfirmDialog"
      :title="t('session.confirm.deleteSessionTitle')"
      size="sm"
    >
      <p class="text-[var(--text-primary)] text-[calc(13px*var(--ui-scale))]">
        {{ t('session.confirm.deleteRunningMsg', { name: pendingSession?.name }) }}
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <button class="wb-btn-ghost" @click="showDeleteSessionConfirmDialog = false">
            {{ t('session.button.cancel') }}
          </button>
          <button
            class="wb-btn-primary bg-[var(--color-danger)]"
            :disabled="isOperating"
            @click="confirmDeleteSessionNow"
          >
            {{ t('session.confirm.stopAndDelete') }}
          </button>
        </div>
      </template>
    </PluginModal>

    <!-- 操作中遮罩（Teleport：同 Modal 约定，避免父容器 overflow/transform 裁剪） -->
    <Teleport to="body">
      <div
        v-if="isOperating"
        class="fixed inset-0 bg-black/40 flex items-center justify-center z-50"
      >
        <div
          class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] px-6 py-5 flex items-center gap-3"
        >
          <svg
            class="w-4 h-4 animate-spin text-[var(--text-secondary)]"
            fill="none"
            viewBox="0 0 24 24"
          >
            <circle
              class="opacity-25"
              cx="12"
              cy="12"
              r="10"
              stroke="currentColor"
              stroke-width="2"
            ></circle>
            <path
              class="opacity-75"
              fill="currentColor"
              d="M4 12a8 8 0 018-8v2a6 6 0 00-6 6H4z"
            ></path>
          </svg>
          <p class="wb-mono text-xs text-[var(--text-primary)]">{{ operatingMessage }}</p>
        </div>
      </div>
      <!-- 终端窗口打开中遮罩：窗口就绪（就绪事件或 4s 兜底）后消失 -->
      <div
        v-if="isTerminalOpening"
        class="fixed inset-0 bg-black/40 flex items-center justify-center z-50"
      >
        <div
          class="rounded-[10px] border border-[var(--border)] bg-[var(--bg-card)] px-6 py-5 flex items-center gap-3"
        >
          <svg
            class="w-4 h-4 animate-spin text-[var(--text-secondary)]"
            fill="none"
            viewBox="0 0 24 24"
          >
            <circle
              class="opacity-25"
              cx="12"
              cy="12"
              r="10"
              stroke="currentColor"
              stroke-width="2"
            ></circle>
            <path
              class="opacity-75"
              fill="currentColor"
              d="M4 12a8 8 0 018-8v2a6 6 0 00-6 6H4z"
            ></path>
          </svg>
          <p class="wb-mono text-xs text-[var(--text-primary)]">
            {{ t('session.terminal.opening') }}
          </p>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
/**
 * SessionCenterView — 会话中心页面（宿主 `SessionsConfigView.vue` 的插件版，票 13）
 *
 * What to build（票面）：侧边栏出现插件贡献的会话目录，宿主内置入口让位，页面外观、
 * 排序、交互、快捷键与今天一致；页内数据全部经插件上下文 → 插件后端 → 宿主原语取得。
 *
 * 结构对齐宿主原页：工具栏页头（刷新/新建）→ Tab（终端配置 / 运行中的会话）→
 * 配置卡片（含该配置的会话行与行内操作）→ 运行中汇总列表 → 四个弹窗 + 两个遮罩。
 * 文案全部走插件 i18n（`session.*`），文案与宿主逐字一致。
 *
 * 与宿主原页的刻意差异（票内记录）：
 * - 终端窗口不搬（spec D3）：经 `context.session.openTerminal/closeTerminal/
 *   isTerminalOpen/predictTerminalSize` 触发宿主窗口，渲染管线与贴靠手感全留宿主；
 * - 配置 CRUD 与创建/停止走上层插件命令通道（真源在本插件私有库）；
 * - 宿主 `PluginPageToolbar target="sessions"` 不再渲染（宿主原页让位后其挂载点消失），
 *   页面工具栏扩展点对「会话页」的贡献面归票 18 复评。
 */
import { computed, inject, onMounted, onUnmounted, ref } from 'vue'
import { toast } from 'vue-sonner'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import PluginModal from './PluginModal.vue'
import SessionConfigForm, { type SessionConfigFormData } from './SessionConfigForm.vue'
import { useSessionCenter, isRunningStatus, type SessionDto } from '../composables/useSessionCenter'
import { formatDateTime } from '../utils/format'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)

const {
  configs,
  sessions,
  runningSessions,
  isLoading,
  now,
  load,
  startConfig,
  viewTerminal,
  stopSession,
  removeSession,
  restartSession,
  saveConfig,
  removeConfig,
  isTerminalOpen,
  startTicker,
  stopTicker,
} = useSessionCenter(context)

// ==================== Tab 切换：终端配置 / 运行中的会话 ====================
type SessionTabKey = 'configs' | 'running'
const activeTab = ref<SessionTabKey>('configs')
const sessionTabs = computed<{ key: SessionTabKey; label: string }[]>(() => [
  { key: 'configs', label: t('session.tab.configs') },
  { key: 'running', label: `${t('session.tab.running')} (${runningSessions.value.length})` },
])

const showCreateDialog = ref(false)
const editingConfig = ref<(typeof configs.value)[number] | null>(null)
const showDeleteConfirmDialog = ref(false)
const pendingDeleteConfigId = ref<string | null>(null)
const sessionFormRef = ref<InstanceType<typeof SessionConfigForm> | null>(null)

// 会话操作对话框状态
const showStopConfirmDialog = ref(false)
const showDeleteSessionConfirmDialog = ref(false)
const pendingSession = ref<SessionDto | null>(null)

// 操作中的 loading 状态
const isOperating = ref(false)
const operatingMessage = ref(t('session.operating.processing'))
// 终端窗口打开中的 loading 状态（新建窗口时显示，直到窗口就绪）
const isTerminalOpening = ref(false)

/** 新建配置的默认值（插件存储的上次成功保存值；表单不自行持久化） */
const formDefaults = ref<{
  environment?: string
  wslDistro?: string
  workingDir?: string
  command?: string
}>({})

const FORM_DEFAULTS_KEY = 'session.formDefaults'

async function loadFormDefaults(): Promise<void> {
  try {
    const stored = await context.storage.get<typeof formDefaults.value>(FORM_DEFAULTS_KEY)
    if (stored && typeof stored === 'object') formDefaults.value = stored
  } catch (e) {
    // 默认值缺失只影响预填，不阻断页面（无默认值时表单用平台默认环境）
    console.warn('[Session Center] form defaults unavailable:', e)
  }
}

// ==================== 数据辅助 ====================

function sessionsOf(configId: string): SessionDto[] {
  return sessions.value.filter((s) => s.configId === configId)
}

function runningOf(configId: string): SessionDto[] {
  return sessionsOf(configId).filter((s) => isRunningStatus(s.status))
}

function isRunning(status: string): boolean {
  return isRunningStatus(status)
}

/**
 * 环境徽标文本——在配置卡片上以紧凑字串标注执行环境。
 * windows/wsl2/linux 三档分别对应 win/wsl2/linux；历史 'powershell'/'cmd' 归为 win。
 */
function envBadge(env: string | undefined | null): string {
  const v = (env ?? '').toLowerCase()
  if (v === 'wsl2') return 'wsl2'
  if (v === 'linux') return 'linux'
  return 'win'
}

function statusDot(status: string): string {
  switch (status) {
    case 'running':
      return 'bg-green-500 animate-pulse'
    case 'waitingInput':
      return 'bg-amber-500'
    case 'error':
      return 'bg-red-500'
    default:
      return 'bg-[var(--text-tertiary)]'
  }
}

function statusText(status: string): string {
  switch (status) {
    case 'starting':
      return t('session.status.starting')
    case 'running':
      return t('session.status.running')
    case 'waitingInput':
      return t('session.status.asking')
    case 'error':
      return t('session.status.error')
    case 'stopped':
      return t('session.status.stopped')
    default:
      return t('session.status.unknown')
  }
}

function runTimeText(session: SessionDto): string {
  const start = session.startedAt || session.createdAt
  if (!start) return '--'
  const diff = Math.floor((now.value - new Date(start).getTime()) / 1000)
  if (diff < 0) return '--'
  if (diff < 60) return t('session.time.secondsAgo', { n: diff })
  if (diff < 3600)
    return t('session.time.minutesSecondsAgo', { m: Math.floor(diff / 60), s: diff % 60 })
  return t('session.time.hoursMinutesAgo', {
    h: Math.floor(diff / 3600),
    m: Math.floor((diff % 3600) / 60),
  })
}

// ==================== 操作 ====================

async function refresh(): Promise<void> {
  try {
    await load()
    toast.info(t('session.toast.listRefreshed'))
  } catch (e) {
    toast.error(t('session.error.loadFailed'))
  }
}

function openCreateDialog(): void {
  editingConfig.value = null
  showCreateDialog.value = true
}

function openEditDialog(config: (typeof configs.value)[number]): void {
  editingConfig.value = config
  showCreateDialog.value = true
}

/**
 * 启动会话：先预测宿主终端窗口网格（PTY 以正确行列 openpty），再经插件命令通道
 * 创建并启动；两阶段与命名/映射编排在本插件 WASM 完成。
 */
async function doStart(configId: string): Promise<void> {
  isOperating.value = true
  operatingMessage.value = t('session.operating.starting')
  try {
    await startConfig(configId)
    toast.success(t('session.toast.started'))
  } catch (e: any) {
    console.error('[Session Center] start session failed:', e)
    toast.error(t('session.error.startFailed', { error: e?.message || e }))
  } finally {
    isOperating.value = false
  }
}

async function viewSession(session: SessionDto): Promise<void> {
  // 已有终端窗口：直接聚焦，无需 loading
  if (isTerminalOpen(session.id)) {
    await viewTerminal(session)
    return
  }
  isTerminalOpening.value = true
  try {
    const opened = await viewTerminal(session)
    if (!opened) toast.info(t('session.error.notRunning'))
  } catch (e) {
    console.error('[Session Center] openTerminal error:', e)
    toast.error(t('session.terminal.openFailed'))
  } finally {
    isTerminalOpening.value = false
  }
}

function confirmStopSession(session: SessionDto): void {
  pendingSession.value = session
  showStopConfirmDialog.value = true
}

async function confirmStop(): Promise<void> {
  if (!pendingSession.value) return
  isOperating.value = true
  operatingMessage.value = t('session.operating.stopping')
  try {
    await stopSession(pendingSession.value.id)
    toast.info(t('session.toast.stopped'))
  } catch (e: any) {
    toast.error(t('session.error.stopFailed', { error: e?.message || e }))
  } finally {
    isOperating.value = false
    showStopConfirmDialog.value = false
    pendingSession.value = null
  }
}

async function doRestart(session: SessionDto): Promise<void> {
  isOperating.value = true
  operatingMessage.value = t('session.operating.restarting')
  try {
    await restartSession(session.id)
    toast.success(t('session.toast.restarted'))
  } catch (e: any) {
    toast.error(t('session.error.restartFailed', { error: e?.message || e }))
  } finally {
    isOperating.value = false
  }
}

function confirmDeleteSession(session: SessionDto): void {
  pendingSession.value = session
  // 运行中的会话提示将先停止再删除，已停止的会话直接删除（宿主原页同语义）
  if (isRunningStatus(session.status)) {
    showDeleteSessionConfirmDialog.value = true
  } else {
    void confirmDeleteSessionNow()
  }
}

async function confirmDeleteSessionNow(): Promise<void> {
  if (!pendingSession.value) return
  const sessionId = pendingSession.value.id
  const running = isRunningStatus(pendingSession.value.status)

  isOperating.value = true
  operatingMessage.value = running
    ? t('session.operating.stoppingAndDeleting')
    : t('session.operating.processing')

  try {
    // 运行中的会话先停止（close 保留记录），再移除记录
    if (running) await stopSession(sessionId)
    await removeSession(sessionId)
    toast.success(t('session.toast.deleted'))
  } catch (e: any) {
    toast.error(t('session.error.deleteFailed', { error: e?.message || e }))
  } finally {
    isOperating.value = false
    showDeleteSessionConfirmDialog.value = false
    pendingSession.value = null
  }
}

function confirmDeleteConfig(configId: string): void {
  pendingDeleteConfigId.value = configId
  showDeleteConfirmDialog.value = true
}

async function confirmDeleteConfigNow(): Promise<void> {
  if (!pendingDeleteConfigId.value) return
  try {
    await removeConfig(pendingDeleteConfigId.value)
    toast.success(t('session.toast.configDeleted'))
  } catch (e: any) {
    toast.error(t('session.error.saveFailed', { error: e?.message || e }))
  } finally {
    showDeleteConfirmDialog.value = false
    pendingDeleteConfigId.value = null
  }
}

function submitForm(): void {
  if (sessionFormRef.value) handleSaveConfig(sessionFormRef.value.form)
}

async function handleSaveConfig(form: SessionConfigFormData): Promise<void> {
  const draft = {
    id: editingConfig.value?.id,
    name: form.name,
    environment: form.environment,
    wslDistro: form.wslDistro || undefined,
    workingDir: form.workingDir || '',
    command: form.command || '',
    autoStart: form.autoStart,
  }
  try {
    await saveConfig(draft)
    toast.success(
      editingConfig.value ? t('session.toast.configUpdated') : t('session.toast.configCreated'),
    )
    showCreateDialog.value = false
    editingConfig.value = null
    // 记住本次取值作为下次新建的默认值：宿主设置分组的会话默认值迁移归票 14，
    // 在此之前用「上次成功保存值」承接默认值语义
    formDefaults.value = {
      environment: form.environment,
      wslDistro: form.wslDistro || undefined,
      workingDir: form.workingDir || undefined,
      command: form.command || undefined,
    }
    void context.storage.set(FORM_DEFAULTS_KEY, formDefaults.value)
  } catch (e: any) {
    console.error('[Session Center] save config failed:', e)
    toast.error(t('session.error.saveFailed', { error: e?.message || e }))
  }
}

// ==================== 页面级键盘快捷键（宿主原页同款：Ctrl/⌘+N 新建、Esc 关闭） ====================

function handleKeydown(event: KeyboardEvent): void {
  const target = event.target as HTMLElement | null
  const tag = target?.tagName?.toLowerCase() ?? ''
  const inInput =
    tag === 'input' || tag === 'textarea' || tag === 'select' || target?.isContentEditable === true

  // Ctrl/⌘ + N：新建配置（输入框内不触发）
  if ((event.ctrlKey || event.metaKey) && !event.shiftKey && event.key.toLowerCase() === 'n') {
    if (inInput) return
    event.preventDefault()
    openCreateDialog()
    return
  }

  // Esc：关闭全部弹窗（ignoreInput：输入框内也生效，与宿主原页一致）
  if (event.key === 'Escape') {
    showCreateDialog.value = false
    showDeleteConfirmDialog.value = false
    showStopConfirmDialog.value = false
    showDeleteSessionConfirmDialog.value = false
  }
}

onMounted(async () => {
  isLoading.value = true
  try {
    await load()
    await loadFormDefaults()
  } catch (e) {
    console.error('[Session Center] load failed:', e)
    toast.error(t('session.error.loadFailed'))
  }
  isLoading.value = false
  startTicker()
  document.addEventListener('keydown', handleKeydown)
})

onUnmounted(() => {
  stopTicker()
  document.removeEventListener('keydown', handleKeydown)
})
</script>

<style scoped>
/* Tab 切换过渡：淡入淡出 + 轻微 Y 位移，避免切换闪现（宿主原页同款） */
.tab-fade-enter-active,
.tab-fade-leave-active {
  transition:
    opacity 0.16s ease,
    transform 0.16s ease;
}
.tab-fade-enter-from {
  opacity: 0;
  transform: translateY(4px);
}
.tab-fade-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
