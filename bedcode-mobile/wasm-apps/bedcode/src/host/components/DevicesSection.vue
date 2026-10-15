<template>
  <div class="h-full flex flex-col" style="background: var(--mobile-bg-primary)">
    <!-- ==================== Header（旧 DevicesView 页头：标题 + 连接状态小字 + 扫描发现开关） ==================== -->
    <div class="page-header flex-shrink-0">
      <div class="page-header-row">
        <div class="min-w-0 flex-1">
          <h1 class="page-title truncate">{{ t('hub.title') }}</h1>
          <!-- 连接成功后标题下方仅显示「已连接」小字，未连接/连接中显示状态文字 -->
          <p class="page-subtitle connection-status-subtitle">
            <template v-if="isConnected && currentDevice">
              <span class="inline-flex items-center gap-1.5">
                <span class="status-dot dot-emerald"></span>
                {{ t('hub.status.connected') }}
              </span>
            </template>
            <template v-else>
              <span
                class="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full border"
                style="border-color: color-mix(in srgb, var(--mobile-accent) 30%, transparent)"
              >
                <span
                  class="status-dot"
                  :style="{
                    background:
                      connectionStatus === 'connecting'
                        ? 'var(--mobile-accent)'
                        : 'var(--mobile-text-muted)',
                  }"
                ></span>
                <span class="text-[var(--mobile-text-secondary)]">{{ connectionStatusText }}</span>
              </span>
            </template>
          </p>
        </div>
        <button
          class="flex-shrink-0 p-2 -mr-2 rounded-lg transition-colors active:opacity-80"
          style="color: var(--mobile-accent)"
          :class="{ 'opacity-50': isConnecting, 'bg-[var(--mobile-accent-muted)]': showDiscovery }"
          :disabled="isConnecting"
          :title="t('hub.discoverDevices')"
          @click="toggleDiscovery"
        >
          <!-- 雷达扫描图标：完整同心圆 + 45° 扫描射线 + 中心点（声呐式） -->
          <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <circle cx="12" cy="12" r="10" />
            <circle cx="12" cy="12" r="6" />
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 12l7.07-7.07" />
            <circle cx="12" cy="12" r="1.2" fill="currentColor" stroke="none" />
          </svg>
        </button>
      </div>
    </div>

    <!-- Main Content（页面内容滚动容器：scrollbar-gutter-stable 防滚动条显隐引起整页横向抖动） -->
    <div
      ref="contentScrollRef"
      class="scrollbar-gutter-stable flex-1 overflow-y-auto overflow-x-hidden px-4 min-h-0"
    >
      <!-- 连接失败提示（附加在页面区块上方，不替换列表；连接中 loading 由弹窗 LoadingDialog 承担，
          避免状态卡片与列表的快速显示/隐藏跳变闪动） -->
      <Transition name="fade">
        <div v-if="connectionStatus === 'error'" class="mb-4">
          <div class="group-card">
            <div class="group-row">
              <svg
                class="w-5 h-5"
                style="color: var(--mobile-chip-red)"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="2"
                  d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z"
                />
              </svg>
              <span class="group-row-sub">{{ connectionStatusText }}</span>
            </div>
          </div>
        </div>
      </Transition>

      <!-- 主内容区互斥分支（已连接 / 扫描面板 / mDNS 发现 / 历史列表）：切换时淡入淡出（mode="out-in"），
          避免卡片硬切闪动；分支均需显式 key（同标签 div 复用会跳过过渡） -->
      <Transition name="fade" mode="out-in">
        <!-- Connected: 当前设备（会话配置已迁移到会话页） -->
        <div v-if="isConnected" key="connected" class="pb-8">
          <div class="pt-2 space-y-3">
            <!-- Connected device info + disconnect（同行，断开按钮位于卡片右侧） -->
            <div
              v-if="currentDevice"
              class="bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl p-4 transition-colors duration-300"
            >
              <div class="flex items-center gap-3">
                <span class="device-icon chip-emerald">
                  <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      stroke-width="1.75"
                      d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z"
                    />
                  </svg>
                </span>
                <div class="flex-1 min-w-0">
                  <span class="text-base font-medium text-[var(--mobile-text-primary)] truncate block">
                    {{ currentDevice.name || currentDevice.address }}
                  </span>
                  <p class="text-xs mt-1 font-mono text-[var(--mobile-text-muted)]">
                    {{ currentDevice.address }}:{{ currentDevice.port }}
                  </p>
                </div>
                <!-- 断开按钮：与连接信息同行，不独占一行 -->
                <button
                  class="flex-shrink-0 h-11 px-3.5 rounded-xl flex items-center gap-1.5 chip-red font-medium text-sm transition-opacity duration-300 active:opacity-80 hover:opacity-90"
                  @click="handleDisconnect"
                >
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      stroke-width="1.75"
                      d="M13 10V3L4 14h7v7l9-11h-7z"
                    />
                  </svg>
                  <span>{{ t('hub.disconnect') }}</span>
                </button>
              </div>

              <!-- 会话页入口：连接 → 会话的联动（旧宿主由底部导航承担，此处就近给入口） -->
              <button
                v-if="isAuthenticated"
                class="mt-3 w-full h-11 rounded-xl text-sm font-medium transition-colors active:opacity-80"
                style="background: var(--mobile-bg-secondary); color: var(--mobile-text-primary)"
                @click="$emit('open-sessions')"
              >
                {{ t('hub.sessions') }}
              </button>
            </div>
          </div>
        </div>

        <!-- Not Connected: 内嵌扫描面板（扫描整合进连接页，替代独立扫描页） -->
        <div v-else-if="showScanner" key="scanner" class="pb-8">
          <ScanPanel @close="showScanner = false" @scan-result="handleScanResult" />
        </div>

        <!-- Not Connected: mDNS 扫描发现（融合进连接页：点击页头雷达按钮展开，扫描动画+结果在此展示，区域可关闭） -->
        <div v-else-if="showDiscovery" key="discovery" class="pb-8">
          <!-- 区域头部：扫描发现 + 控制（停止/重新扫描）+ × 关闭。吸顶在内容区顶部，
               设备列表较长时滚动中控制按钮仍可达 -->
          <div
            class="sticky top-0 z-10 -mx-4 px-4 pt-2 pb-2 flex items-center justify-between"
            style="background: var(--mobile-bg-primary)"
          >
            <span class="text-sm font-semibold text-[var(--mobile-text-muted)]">
              {{ t('hub.discoverTitle') }}
              <span
                v-if="discoveredServices.length > 0"
                class="ml-1 px-1.5 py-0.5 rounded-full text-xs"
                style="background: var(--mobile-bg-elevated); color: var(--mobile-text-secondary)"
                >{{ discoveredServices.length }}</span
              >
            </span>
            <div class="flex items-center -mr-2 flex-shrink-0">
              <button
                class="h-11 px-3 rounded-lg text-sm font-medium transition-colors duration-200 active:opacity-80"
                style="color: var(--mobile-text-secondary)"
                :disabled="isConnecting"
                :class="{ 'opacity-50': isConnecting }"
                @click="toggleMdnsScan"
              >
                {{ mdnsScanning ? t('hub.discoverStopScan') : t('hub.discoverRestartScan') }}
              </button>
              <button
                class="h-11 w-11 rounded-lg flex items-center justify-center transition-colors duration-200 active:opacity-80"
                style="color: var(--mobile-text-muted)"
                :title="t('hub.actionClose')"
                @click="closeDiscovery"
              >
                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
          </div>

          <!-- Scanning status（扫描动画：spinner + 状态文字 + 已发现数） -->
          <div v-if="mdnsScanning" class="pb-3 flex items-center gap-3">
            <div
              class="w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin"
              style="color: var(--mobile-accent)"
            />
            <span class="group-row-sub">{{ t('hub.discoverScanning') }}</span>
            <span
              v-if="discoveredServices.length > 0"
              class="text-sm font-medium ml-auto"
              style="color: var(--mobile-accent)"
            >
              {{ t('hub.discoverDeviceFound', { count: discoveredServices.length }) }}
            </span>
          </div>

          <!-- 空态（未扫描且无设备） -->
          <div
            v-if="discoveredServices.length === 0 && !mdnsScanning"
            class="flex flex-col items-center justify-center py-12"
          >
            <svg
              class="w-12 h-12 mb-4"
              style="color: var(--mobile-text-disabled)"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="1.5"
                d="M8.111 16.404a5.5 5.5 0 017.778 0M12 20h.01m-7.08-7.071c3.904-3.905 10.236-3.905 14.141 0M1.394 9.393c5.857-5.858 15.355-5.858 21.213 0"
              />
            </svg>
            <p class="group-row-sub mb-1">{{ t('hub.discoverNoDevices') }}</p>
            <p class="text-sm" style="color: var(--mobile-text-disabled)">
              {{ t('hub.discoverNoDevicesHint') }}
            </p>
          </div>

          <!-- 扫描空态动画（雷达扫射） -->
          <div
            v-if="discoveredServices.length === 0 && mdnsScanning"
            class="flex flex-col items-center justify-center py-12"
          >
            <div class="relative mb-6">
              <div
                class="w-28 h-28 rounded-full relative"
                style="border: 2px solid color-mix(in srgb, var(--mobile-accent) 20%, transparent)"
              >
                <div
                  class="absolute inset-2 rounded-full"
                  style="border: 1px solid color-mix(in srgb, var(--mobile-accent) 10%, transparent)"
                ></div>
                <div
                  class="absolute inset-4 rounded-full"
                  style="border: 1px solid color-mix(in srgb, var(--mobile-accent) 5%, transparent)"
                ></div>
                <div class="absolute inset-0 radar-sweep origin-center">
                  <div
                    class="w-1/2 h-0.5 absolute top-1/2 left-1/2 -translate-y-1/2"
                    style="background: linear-gradient(to right, var(--mobile-accent), transparent)"
                  ></div>
                </div>
                <div
                  class="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-3 h-3 rounded-full"
                  style="background: var(--mobile-accent)"
                ></div>
              </div>
            </div>
            <p class="group-row-sub">{{ t('hub.discoverScanning') }}</p>
          </div>

          <!-- 扫描结果（发现的设备列表） -->
          <div v-if="discoveredServices.length > 0" class="group-card">
            <div
              v-for="service in discoveredServices"
              :key="service.instance_name"
              class="group-row device-row"
              :class="isDiscoverCurrentDevice(service) ? 'is-connected' : 'group-row-btn cursor-pointer'"
              @click="handleDiscoverConnect(service)"
            >
              <span class="device-chip" :class="isDiscoverCurrentDevice(service) ? '' : 'chip-cyan'">
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="2"
                    d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z"
                  />
                </svg>
              </span>
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2 min-w-0">
                  <span class="device-name truncate">{{ service.device_name || service.instance_name }}</span>
                  <span
                    class="status-badge ml-auto"
                    :class="isDiscoverCurrentDevice(service) ? 'badge-emerald' : 'badge-cyan'"
                  >
                    <span v-if="isDiscoverCurrentDevice(service)" class="status-dot dot-emerald"></span>
                    {{
                      isDiscoverCurrentDevice(service)
                        ? t('hub.discoverConnected')
                        : t('hub.discoverConnectToDevice')
                    }}
                  </span>
                </div>
                <div class="device-addr font-mono truncate">
                  {{ service.address }}:{{ service.port }}
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- Not Connected: Connection History + Scan Result。
             历史为空时整个历史区块（标题/条数/清除/列表）不渲染——空历史只剩标题与
             「暂无」文案纯属噪音；与扫码结果都为空时本分支整体不渲染，不留空内容区 -->
        <div v-else-if="hasHistoryOrScanResult" key="history" class="pb-8">
          <div class="pt-2 space-y-3">
            <!-- Connection History（仅非空时渲染，带条数与清除入口） -->
            <template v-if="hasConnectionHistory">
              <div class="flex items-center justify-between pt-2">
                <span class="text-sm font-semibold text-[var(--mobile-text-muted)]">
                  {{ t('hub.history') }}
                  <span
                    class="ml-1 px-1.5 py-0.5 rounded-full text-xs"
                    style="background: var(--mobile-bg-elevated); color: var(--mobile-text-secondary)"
                    >{{ connectionHistory.length }}</span
                  >
                </span>
                <button
                  class="text-sm transition-colors active:opacity-80"
                  style="color: var(--mobile-text-muted)"
                  @click="showClearHistoryConfirm = true"
                >
                  {{ t('hub.clearHistory') }}
                </button>
              </div>

              <TransitionGroup name="config-list" tag="div" class="space-y-3">
                <button
                  v-for="item in connectionHistory"
                  :key="item.address"
                  class="w-full bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl p-4 text-left cursor-pointer transition-[border-color,opacity] duration-300 active:opacity-90 hover:border-[var(--mobile-border-hover)]"
                  :disabled="isConnecting"
                  :class="{ 'opacity-50 pointer-events-none': isConnecting }"
                  @click="handleConnectFromHistory(item)"
                >
                  <div class="flex items-center gap-3">
                    <span class="device-icon chip-cyan">
                      <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path
                          stroke-linecap="round"
                          stroke-linejoin="round"
                          stroke-width="1.75"
                          d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z"
                        />
                      </svg>
                    </span>
                    <div class="flex-1 min-w-0">
                      <div class="text-base font-medium text-[var(--mobile-text-primary)] truncate">
                        {{ item.name || item.address }}
                      </div>
                      <p class="text-xs mt-1 font-mono text-[var(--mobile-text-muted)]">
                        {{ item.address }}
                      </p>
                      <p class="text-xs mt-0.5" style="color: var(--mobile-text-disabled)">
                        {{ formatLastConnected(item.lastConnected, t) }}
                      </p>
                    </div>
                    <button
                      class="p-1.5 rounded-lg transition-colors active:opacity-80 flex-shrink-0"
                      style="color: var(--mobile-text-disabled)"
                      :disabled="isConnecting"
                      :aria-label="t('hub.remove')"
                      :title="t('hub.remove')"
                      @click.stop="removeFromHistory(item.address)"
                    >
                      <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path
                          stroke-linecap="round"
                          stroke-linejoin="round"
                          stroke-width="2"
                          d="M6 18L18 6M6 6l12 12"
                        />
                      </svg>
                    </button>
                  </div>
                </button>
              </TransitionGroup>
            </template>

            <!-- 扫描结果：与连接历史同级别的独立区块（识别到二维码停止扫描后出现），可关闭；
                 历史为空时不再让位（无 pt-4 缩进），直接贴顶 -->
            <Transition name="fade">
              <div v-if="scanResult" :class="hasConnectionHistory ? 'pt-4' : 'pt-0'">
                <div class="flex items-center justify-between pb-2">
                  <span class="text-sm font-semibold text-[var(--mobile-text-muted)]">
                    {{ t('hub.scanResultTitle') }}
                  </span>
                  <button
                    class="p-2 -mr-2 rounded-lg transition-colors active:opacity-80 flex-shrink-0"
                    style="color: var(--mobile-text-muted)"
                    :title="t('hub.actionClose')"
                    @click="scanResult = null"
                  >
                    <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        stroke-width="2"
                        d="M6 18L18 6M6 6l12 12"
                      />
                    </svg>
                  </button>
                </div>
                <button
                  class="w-full bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl p-4 text-left cursor-pointer transition-[border-color,opacity] duration-300 active:opacity-90 hover:border-[var(--mobile-border-hover)]"
                  :disabled="isConnecting"
                  :class="{ 'opacity-50 pointer-events-none': isConnecting }"
                  @click="onQrConnect"
                >
                  <div class="flex items-center gap-3">
                    <span class="device-icon chip-emerald">
                      <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path
                          stroke-linecap="round"
                          stroke-linejoin="round"
                          stroke-width="1.75"
                          d="M12 4v1m6 11h2m-6 0h-2m0 0H8m4 0h4m-4-8a1 1 0 011-1h1.586a1 1 0 01.707.293l3.828 3.828a1 1 0 01.293.707V17a1 1 0 01-1 1H8a1 1 0 01-1-1V7a1 1 0 011-1z"
                        />
                      </svg>
                    </span>
                    <div class="flex-1 min-w-0">
                      <div class="text-base font-medium text-[var(--mobile-text-primary)] truncate">
                        Desktop
                      </div>
                      <p class="text-xs mt-1 font-mono text-[var(--mobile-text-muted)]">
                        {{ scanResult.host }}:{{ scanResult.port }}
                      </p>
                    </div>
                  </div>
                </button>
              </div>
            </Transition>
          </div>
        </div>
      </Transition>
    </div>

    <!-- Action Buttons (when not connected)：底部两枚 CTA 常驻（扫码 / 手动），不再随 mDNS
         开关换布局——扫描控制已移入发现面板头部，切换扫描时底部高度与内容均不跳变 -->
    <div
      v-if="!isConnected"
      class="flex-shrink-0 p-4 space-y-3"
      style="padding-bottom: max(1rem, var(--safe-area-bottom, 0px))"
    >
      <!-- 相机态与默认态互斥：切换淡入淡出，避免按钮块硬切闪动 -->
      <Transition name="fade" mode="out-in">
        <!-- 二维码扫描：启动后按钮切换为「停止扫描」（红色） -->
        <button
          v-if="showScanner"
          key="scan"
          class="w-full h-11 rounded-xl text-base font-medium transition-colors active:opacity-80 flex items-center justify-center gap-2"
          style="background: var(--mobile-chip-red-bg); color: var(--mobile-chip-red); border: 1px solid color-mix(in srgb, var(--mobile-chip-red) 25%, transparent)"
          :class="{ 'opacity-50': isConnecting }"
          :disabled="isConnecting"
          @click="toggleScanner"
        >
          <svg class="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
            <rect x="6" y="6" width="12" height="12" rx="2" />
          </svg>
          {{ t('hub.scanStop') }}
        </button>

        <div v-else key="default" class="space-y-3">
          <button
            class="w-full h-11 rounded-xl text-base font-medium transition-colors active:opacity-80 flex items-center justify-center gap-2"
            style="background: var(--mobile-accent); color: var(--mobile-text-on-accent)"
            :class="{ 'opacity-50': isConnecting }"
            :disabled="isConnecting"
            @click="toggleScanner"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M12 4v1m6 11h2m-6 0h-2m0 0H8m4 0h4m-4-8a1 1 0 011-1h1.586a1 1 0 01.707.293l3.828 3.828a1 1 0 01.293.707V17a1 1 0 01-1 1H8a1 1 0 01-1-1V7a1 1 0 011-1z"
              />
            </svg>
            {{ t('hub.qrConnect') }}
          </button>
          <button
            class="w-full h-11 rounded-xl text-base font-medium transition-colors active:opacity-80 flex items-center justify-center gap-2"
            style="background: var(--mobile-group-bg); color: var(--mobile-text-secondary); border: 1px solid var(--mobile-group-border)"
            :class="{ 'opacity-50': isConnecting }"
            :disabled="isConnecting"
            @click="showManualConnect = true"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M13.828 10.172a4 4 0 00-5.656 0l-4 4a4 4 0 105.656 5.656l1.102-1.101m-.758-4.899a4 4 0 005.656 0l4-4a4 4 0 00-5.656-5.656l-1.1 1.1"
              />
            </svg>
            {{ t('hub.manualConnect') }}
          </button>
        </div>
      </Transition>
    </div>

    <!-- Manual Connect Dialog -->
    <BottomSheet
      v-model="showManualConnect"
      :title="t('hub.connectNewDevice')"
      :placeholder="t('hub.addressPlaceholder')"
      :loading="isConnecting"
      :connecting-text="t('hub.bottomSheetConnecting')"
      :cancel-connect-text="t('hub.bottomSheetCancelConnect')"
      :cancel-text="t('hub.actionCancel')"
      :confirm-text="t('hub.actionConfirm')"
      @submit="handleConnectManual"
      @cancel="handleCancelConnection"
    />

    <!-- Biometric Auth Dialog -->
    <BiometricAuthDialog
      v-model="showBiometricDialog"
      :error="authDialogError"
      :loading="authDialogLoading"
      @authenticate="runBiometricAuth"
      @switch-to-pairing="handleSwitchToPairing"
      @close="handleBiometricDialogClose"
    />

    <!-- Pairing Dialog -->
    <PairingInput
      v-model="showPairing"
      :loading="isPairing"
      :error="pairingError"
      @submit="handlePairingSubmit"
      @switch="handleSwitchToBiometric"
      @close="handlePairingClose"
    />

    <!-- Clear History Confirmation Modal -->
    <Modal v-model="showClearHistoryConfirm" :title="t('hub.clearHistory')" size="sm">
      <p style="color: var(--mobile-text-disabled)">{{ t('hub.historyClearConfirm') }}</p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showClearHistoryConfirm = false">
            {{ t('hub.actionCancel') }}
          </Button>
          <Button variant="danger" @click="confirmClearHistory">{{ t('hub.actionConfirm') }}</Button>
        </div>
      </template>
    </Modal>

    <!-- Disconnect Confirmation Modal -->
    <Modal v-model="showDisconnectConfirm" :title="t('hub.disconnect')" size="sm">
      <p style="color: var(--mobile-text-disabled)">{{ t('hub.disconnectConfirm') }}</p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showDisconnectConfirm = false">
            {{ t('hub.actionCancel') }}
          </Button>
          <Button variant="danger" @click="confirmDisconnect">{{ t('hub.disconnect') }}</Button>
        </div>
      </template>
    </Modal>

    <!-- 全局遮罩 Loading（配对请求） -->
    <Teleport to="body">
      <Transition name="fade">
        <div
          v-if="showPairingLoading"
          class="fixed inset-0 z-[100] flex items-center justify-center backdrop-blur-sm mobile-ui"
          style="background: var(--mobile-overlay)"
        >
          <div
            class="rounded-2xl p-6 shadow-xl flex flex-col items-center gap-4 min-w-[200px]"
            style="background: var(--mobile-group-bg)"
          >
            <div
              class="w-10 h-10 border-4 border-current border-t-transparent rounded-full animate-spin"
              style="color: var(--mobile-accent)"
            />
            <p class="text-sm font-medium" style="color: var(--mobile-text-secondary)">
              {{ t('hub.pairingRequesting') }}
            </p>
          </div>
        </div>
      </Transition>
    </Teleport>

    <!-- Loading Dialog: 连接中（弹窗遮罩，阻断重复点击） -->
    <LoadingDialog :visible="showConnectLoading" :message="connectLoadingMessage" />
  </div>
</template>

<script setup lang="ts">
/**
 * 设备 / 连接区（旧宿主 `Views/DevicesView.vue` 形态还原，2026-10-15）
 * -----------------------------------------------------------------------------
 * 还原范围（与旧宿主逐块对齐）：
 * - 页头（标题 + 连接状态小字 + 雷达扫描按钮）；内容区互斥分支（已连接 / 扫码 /
 *   mDNS 发现 / 历史 + 扫码结果）；底部常驻双 CTA（二维码连接 ⇄ 停止扫描 / 手动连接）
 * - 连接流程：手动连接必走配对；历史 / 发现连接允许存量凭据静默换新；未认证时
 *   **按 `preferredAuthMethod` 选弹窗**（生物优先且已绑定 → 直接弹并自动触发指纹，
 *   否则请求配对码）；弹窗可互切，关闭弹窗 = 断开；扫码走「连上 → 等 isConnected
 *   （200ms×10s）→ QR token 认证」
 * - 机制：二次确认弹窗（清空历史 / 断开）、全屏配对请求遮罩、连接中 LoadingDialog、
 *   连接历史相对时间 + 单项删除
 *
 * 数据面（与旧实现同源）：引擎事实走 `useHostPage`（宿主连接域投影），配对 / 认证
 * 编排走插件命令面（`terminal-session.request-pairing` 等）；本组件不直连宿主命令。
 *
 * 与旧实现的三处**有意**差异（其余逐步还原）：
 * 1. 扫码面板未迁「我的二维码」信息弹窗（插件侧暂无平台投影，见 ScanPanel 头注）
 * 2. 连接历史写入时机收敛为「认证成功」（旧实现在请求配对成功后即写入——
 *    也就是说配对码还没验证就已经记了一笔；此处改为认证成功才记，语义更准）
 * 3. 新增「已连未认证」兜底入口：重连 / 启动恢复等未走本页连接流程的场景下，
 *    状态为 connected/pairing 且未认证时自动弹认证入口（旧实现在这种场景无入口，
 *    配对口不可达——对锁用例 C-C7 的保护目标改由该兜底承接）
 */
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { useHostPage, type HostDeviceTarget } from '../useHostPage'
import { connectionErrorKey } from '../errors'
import { formatLastConnected, parseAddressWithDefault } from '../utils'
import type { QrPayload } from '../qr'
import BottomSheet from './ui/BottomSheet.vue'
import Button from './ui/Button.vue'
import LoadingDialog from './ui/LoadingDialog.vue'
import Modal from './ui/Modal.vue'
import BiometricAuthDialog from './BiometricAuthDialog.vue'
import PairingInput from './PairingInput.vue'
import ScanPanel from './ScanPanel.vue'

/** mDNS 发现项（宿主 mdnsServices 投影；只取本组件需要的字段） */
interface DiscoveredService {
  instance_name: string
  device_name?: string
  address: string
  port: number
}

const props = withDefaults(defineProps<{ active?: boolean }>(), { active: true })
defineEmits<{
  /** 打开会话页（连接 → 会话联动入口） */
  (e: 'open-sessions'): void
}>()

const ctrl = useHostPage()
const t = ctrl.t
const toast = ctrl.toast
const logger = ctrl.logger

// ── 引擎事实（SDK 投影的 Ref 经本组件 computed 收口：跨包 `import('vue').Ref`
//    类型在模板里不解引用，直接绑定会让 vue-tsc 报错；运行时 Vue 按 __v_isRef 解引用） ──
const connectionStatus = computed(() => ctrl.connectionStatus.value)
const isConnecting = computed(() => ctrl.isConnecting.value)
const currentDevice = computed(() => ctrl.currentDevice.value)
const isAuthenticated = computed(() => ctrl.isAuthenticated.value)
const mdnsScanning = computed(() => ctrl.mdnsScanning.value)
const connectionHistory = computed(() => ctrl.connectionHistory.value ?? [])

// 与旧 DevicesView / 宿主 `isConnected` 同口径：传输层已连（connected）或已认证（paired）；
// 「pairing」（已连待输入配对码）不算，否则配对弹窗背后会多出一张已连接设备卡
const isConnected = computed(
  () => connectionStatus.value === 'connected' || connectionStatus.value === 'paired',
)

// ── 本域状态（旧 DevicesView 同名同义） ──
const contentScrollRef = ref<HTMLElement | null>(null)
const showScanner = ref(false)
const showDiscovery = ref(false)
const scanResult = ref<QrPayload | null>(null)
const showManualConnect = ref(false)
const showPairing = ref(false)
const showPairingLoading = ref(false)
const showConnectLoading = ref(false)
const isPairing = ref(false)
const pairingError = ref('')
const connectionError = ref('')
const showBiometricDialog = ref(false)
const authBiometricAvailable = ref(false)
const authDialogError = ref('')
const authDialogLoading = ref(false)
const showClearHistoryConfirm = ref(false)
const showDisconnectConfirm = ref(false)
/**
 * 认证流程重入闸（同步置位）
 *
 * 两个入口会触发认证流程：`startConnection` 主动调用、「已连未认证」状态 watch 兜底。
 * 状态置位与 watch 回调可能同时在飞（连接成功到 active 确认之间），无闸时会发起两次
 * `request-pairing` / 弹出两个弹窗。
 */
let authFlowActive = false
/**
 * 扫码流程期间抑制「已连未认证」兜底入口
 *
 * 扫码是自带一次性 token 的认证路径（connect → 等 isConnected → QR 认证），
 * 途中状态会短暂落在 connected 且未认证；不抑制的话兜底 watch 会抢先请求配对码，
 * 页面同时冒出配对弹窗与扫码认证两条路径。
 */
let qrFlowActive = false
/** 本页发起的连接流程在飞（含连接后的静默认证窗口）：兜底认证入口 watch 在此期间让位 */
let connectFlowActive = false
/** 本次连接的目标（页头状态小字 / 连接弹窗文案用；与引擎 currentDevice 区分） */
const pendingDevice = ref<HostDeviceTarget | null>(null)

const hasConnectionHistory = computed(() => connectionHistory.value.length > 0)
const hasHistoryOrScanResult = computed(() => hasConnectionHistory.value || scanResult.value !== null)
const discoveredServices = computed<DiscoveredService[]>(() =>
  Array.isArray(ctrl.mdnsServices.value) ? (ctrl.mdnsServices.value as DiscoveredService[]) : [],
)

/** 手动连接的默认端口（旧实现读 `mobile-settings.defaultPort`，缺省 8765） */
const defaultPort = computed(() => {
  const value = Number(ctrl.mobileSettings.value?.defaultPort)
  return Number.isFinite(value) && value > 0 ? value : 8765
})

/** 连接中弹窗文案（旧口径：有目标名带名，否则纯「正在连接…」） */
const connectLoadingMessage = computed(() =>
  pendingDevice.value?.name
    ? t('hub.connectionStatus.connecting', { name: pendingDevice.value.name })
    : t('hub.connectionStatus.connectingPlain'),
)

/** 页头状态小字（旧 `connectionStatusText` 口径逐字还原） */
const connectionStatusText = computed(() => {
  switch (connectionStatus.value) {
    case 'connecting':
      return pendingDevice.value?.name
        ? t('hub.connectionStatus.connecting', { name: pendingDevice.value.name })
        : t('hub.connectionStatus.connectingPlain')
    case 'connected':
      return t('hub.connectionStatus.connected')
    case 'pairing':
      return t('hub.connectionStatus.pairing')
    case 'paired':
      return t('hub.connectionStatus.paired')
    case 'error':
      return connectionError.value ? t(connectionError.value) : t('hub.connectFailed')
    default:
      return connectionStatus.value === 'disconnected' ? t('hub.connectionStatus.disconnected') : ''
  }
})

// ==================== 生命周期（旧 onMounted / onActivated / onDeactivated） ====================

/** 页面可见时重读历史；发现区展开则后台续扫并保留已发现设备（旧 onActivated 口径） */
function onPageActivated(): void {
  void ctrl
    .loadHistory(true)
    .catch((e) => logger.warn(`[host] load connection history failed: ${errText(e)}`))
  if (showDiscovery.value) void startMdnsScan({ keepResults: true })
}

onMounted(() => {
  onPageActivated()
  void ctrl.refreshBiometric()
})

// 页签容器常驻挂载（AppRoot 首次访问后不卸载面板）：用 active 复刻旧
// onActivated / onDeactivated 语义——切走停扫、切回续扫并重读历史
watch(
  () => props.active,
  (visible) => {
    if (visible) onPageActivated()
    else void stopMdnsScan()
  },
)

// 组件真被销毁时也停扫描（面板卸载路径），否则 mDNS 一直跑到进程结束
onUnmounted(() => {
  void stopMdnsScan()
})

// 模式切换（二维码 / 扫描发现 / 历史）复位内容区滚动位置：各区块高度不同，内容收缩时
// scrollTop 会被浏览器夹到更小的值，切回长列表时停在半空中段（看起来像页面跳了一下）
watch([showScanner, showDiscovery], async () => {
  await nextTick()
  const el = contentScrollRef.value
  if (el) el.scrollTop = 0
})

// 连接成功后收起两个扫描视图（二维码 / mDNS），断开后回到历史列表布局
watch(isConnected, (connected) => {
  if (connected) {
    showScanner.value = false
    closeDiscovery()
  }
})

// 认证完成（false → true）：记连接历史 + 拉起会话数据（会话与连接的联动，旧 DevicesView
// 的 watch(pairing) 口径）；重新连接同一设备时也刷新
watch(isAuthenticated, (authenticated, wasAuthenticated) => {
  if (!authenticated || wasAuthenticated) return
  const device = pendingDevice.value ?? currentDevice.value
  if (device) void ctrl.addHistory(`${device.address}:${device.port}`, device.name || device.address)
  pendingDevice.value = null
  void ctrl
    .refreshSessions()
    .catch((e) => logger.warn(`[host] refresh sessions after auth failed: ${errText(e)}`))
})

// 「已连未认证」兜底入口：重连成功 / 启动恢复 JWT 失效等**未走本页连接流程**的场景，
// 旧实现在此无任何认证入口（受保护端点全 401）。按首选认证方式弹入口，保证配对口可达。
// 本页自己发起的连接（connectFlowActive）与扫码流程（qrFlowActive）自行决定认证入口，
// 本 watch 一律让位——否则会在静默换新 JWT 未返回的窗口里抢先弹配对口。
watch(
  [connectionStatus, isAuthenticated],
  ([status, authenticated]) => {
    if (authenticated || isConnecting.value || qrFlowActive || connectFlowActive) return
    if (status !== 'connected' && status !== 'pairing') return
    if (showPairing.value || showBiometricDialog.value || authFlowActive) return
    // 本页未发起过连接（重连 / 启动恢复场景）时，用引擎当前设备作认证目标
    if (!pendingDevice.value && currentDevice.value) pendingDevice.value = { ...currentDevice.value }
    void openAuthEntry()
  },
  // immediate：页面挂载时就已处于「已连未认证」（重连恢复）也要给认证入口
  { immediate: true },
)

// ==================== 连接编排（旧 startConnection） ====================

/**
 * 连接 + 认证编排
 *
 * @param skipPairing 历史 / 发现连接为 true：允许用宿主持有的凭据静默换新（免重配对）；
 *                    手动连接为 false：必须走认证流程
 */
async function startConnection(device: HostDeviceTarget, skipPairing = false): Promise<void> {
  pendingDevice.value = device
  connectionError.value = ''
  showConnectLoading.value = true
  // 本页连接流程在飞：期间由本函数决定认证入口，兜底 watch（专为重连/挂载场景而设）
  // 不得插手——否则它在「已连上、静默换新还没返回」的窗口里抢先请求配对码
  connectFlowActive = true

  try {
    const { authenticated } = await ctrl.connectDevice(device, { silentAuth: skipPairing })
    // 以本次静默认证的结果为准（不读状态位：认证成功与 ws_paired 事件到达之间
    // 存在窗口，读状态位会把「已静默换新成功」误判成需配对）；watch(isAuthenticated)
    // 负责后续收尾（记历史 / 拉会话）
    if (authenticated || isAuthenticated.value) return

    showConnectLoading.value = false
    // 状态 watch 可能已领先一步发起认证（openAuthEntry 已把弹窗打开）：不重复请求配对码
    if (showPairing.value || showBiometricDialog.value || authFlowActive) return
    await openAuthEntry()
  } catch (error) {
    const msg = errText(error)
    connectionError.value = ctrl.deviceError.value || msg
    logger.error(`[host] startConnection failed: address=${device.address} ${msg}`)
    toast.showToast(t(connectionErrorKey(msg)), 'error')
    // 连接失败时确保前后端状态一致：断开后端连接 + 重置前端状态
    await ctrl.disconnect().catch(() => {})
  } finally {
    connectFlowActive = false
    showConnectLoading.value = false
    showPairingLoading.value = false
  }
}

/** 认证入口选择（旧 Step 2.5）：生物优先且已绑定 → 生物弹窗；否则请求配对码 */
async function openAuthEntry(): Promise<void> {
  // 闸门双层：流程在飞（authFlowActive）与弹窗已开（状态 watch 与连接流程两条入口）
  if (authFlowActive || showPairing.value || showBiometricDialog.value) return
  authFlowActive = true
  try {
    await ctrl.refreshBiometric()
    const keyStatus = ctrl.biometric.value
    authBiometricAvailable.value = !!(keyStatus?.deviceSupported && keyStatus?.hasKey)
    authDialogError.value = ''
    pairingError.value = ''

    const preferBiometric =
      ctrl.mobileSettings.value?.preferredAuthMethod === 'biometric' && authBiometricAvailable.value
    if (preferBiometric) await openBiometricAuth()
    else await startPairingFlow()
  } finally {
    authFlowActive = false
  }
}

// Cancel connection
async function handleCancelConnection(): Promise<void> {
  showConnectLoading.value = false
  await ctrl.cancelConnect()
  connectionError.value = t('hub.userCancelled')
}

// 打开生物认证弹窗并立即触发指纹验证（打开即弹系统生物识别）
async function openBiometricAuth(): Promise<void> {
  // 连接中弹窗让位于生物认证弹窗
  showConnectLoading.value = false
  showBiometricDialog.value = true
  authDialogError.value = ''
  await runBiometricAuth()
}

/** 生物认证：指纹识别成功后才走挑战-应答拿到凭据，失败留在弹窗内可重试或切换 */
async function runBiometricAuth(): Promise<void> {
  const device = pendingDevice.value ?? currentDevice.value
  if (!device) return

  authDialogLoading.value = true
  authDialogError.value = ''
  try {
    const ok = await ctrl.authenticateWithBiometric()
    if (ok) {
      showBiometricDialog.value = false
      return
    }
    // 生物认证失败/取消 → 弹窗内展示错误，可重试指纹或切换配对码
    authDialogError.value = t('hub.biometricFailed')
  } catch (e) {
    authDialogError.value = biometricAuthErrorText(e)
  } finally {
    authDialogLoading.value = false
  }
}

/** 将桌面端拒绝原因映射为友好文案（旧 `biometricAuthErrorText` 口径） */
function biometricAuthErrorText(e: unknown): string {
  const msg = errText(e)
  if (/CREDENTIAL_NOT_BOUND|NOT_PAIRED|credential not bound|device not paired/i.test(msg)) {
    return t('hub.biometricNotBoundOnDesktop')
  }
  if (/CHALLENGE_INVALID|SIGNATURE_INVALID|challenge|signature/i.test(msg)) {
    return t('hub.biometricFailed')
  }
  return msg
}

/** 配对码认证（兜底方式）：请求配对码后进入输入弹窗 */
async function startPairingFlow(): Promise<void> {
  const device = pendingDevice.value ?? currentDevice.value
  if (!device) return

  // 连接中弹窗让位于配对请求弹窗
  showConnectLoading.value = false
  showPairingLoading.value = true
  try {
    const pairingTimeout = new Promise<never>((_, reject) =>
      setTimeout(() => reject(new Error(t('hub.pairingTimeout'))), 15000),
    )
    await Promise.race([ctrl.requestPairing(), pairingTimeout])
    showPairing.value = true
  } catch (error) {
    // 配对失败或超时时断开连接
    logger.error(`[host] pairing request failed: ${errText(error)}`)
    connectionError.value = errText(error)
    toast.showToast(t(errText(error)), 'error')
    await ctrl.disconnect().catch(() => {})
  } finally {
    showPairingLoading.value = false
  }
}

/** 配对码弹窗 → 切换到生物认证；未绑定生物认证时提示，无法切换 */
async function handleSwitchToBiometric(): Promise<void> {
  if (!authBiometricAvailable.value) {
    toast.showToast(t('hub.biometricNotBound'), 'warning')
    return
  }
  showPairing.value = false
  pairingError.value = ''
  await openBiometricAuth()
}

/** 生物认证弹窗 → 切换到配对码（兜底方式） */
async function handleSwitchToPairing(): Promise<void> {
  showBiometricDialog.value = false
  authDialogError.value = ''
  await startPairingFlow()
}

/** 用户关闭生物认证弹窗 → 断开连接，保持前后端状态一致 */
function handleBiometricDialogClose(): void {
  showBiometricDialog.value = false
  authDialogError.value = ''
  connectionError.value = t('hub.userCancelled')
  void ctrl.disconnect().catch(() => {})
}

/** 用户关闭配对码弹窗 → 断开连接，保持前后端状态一致 */
function handlePairingClose(): void {
  showPairing.value = false
  pairingError.value = ''
  connectionError.value = t('hub.userCancelled')
  void ctrl.disconnect().catch(() => {})
}

/** 提交配对码（旧 `handlePairingSubmit`） */
async function handlePairingSubmit(code: string): Promise<void> {
  if (!pendingDevice.value) return

  isPairing.value = true
  pairingError.value = ''
  try {
    const success = await ctrl.verifyPairingCode(code)
    if (success) {
      showPairing.value = false
    } else {
      pairingError.value = t('hub.codeVerifyFailed')
    }
  } catch (error) {
    pairingError.value = errText(error)
  } finally {
    isPairing.value = false
  }
}

// ==================== 连接入口（历史 / 手动 / mDNS / 扫码） ====================

/** 从连接历史连接（允许存量凭据静默换新） */
async function handleConnectFromHistory(item: { address: string; name?: string }): Promise<void> {
  if (isConnecting.value) return
  const parsed = parseAddressWithDefault(`${item.address}`, defaultPort.value)
  if (!parsed) {
    logger.warn(`[host] history entry address invalid: ${item.address}`)
    return
  }
  await startConnection({ ...parsed, name: item.name || parsed.address }, true)
}

/** 手动连接（必须走配对流程；未写端口回落默认端口） */
async function handleConnectManual(address: string): Promise<void> {
  if (isConnecting.value) return
  const parsed = parseAddressWithDefault(address, defaultPort.value)
  if (!parsed) {
    // 格式错误是本地校验失败（非引擎错误）：明确告知期望格式，同时落日志便于排障
    logger.warn(`[host] manual address invalid: ${address}`)
    toast.showToast(t('hub.manualInvalid'), 'error')
    return
  }
  showManualConnect.value = false
  await startConnection({ ...parsed, name: parsed.address }, false)
}

/** 点击发现的设备（允许存量凭据静默换新） */
async function handleDiscoverConnect(service: DiscoveredService): Promise<void> {
  if (isDiscoverCurrentDevice(service)) return
  if (isConnecting.value) return
  await startConnection(
    { address: service.address, port: service.port, name: service.device_name || service.instance_name },
    true,
  )
}

/** 该发现设备是否为当前已连接的设备（连接态行样式 + 点击守卫） */
function isDiscoverCurrentDevice(service: DiscoveredService): boolean {
  return isConnected.value && currentDevice.value?.address === service.address
}

// 切换二维码扫描：非扫描态点击「二维码连接」打开内嵌相机；扫描中点击「停止扫描」收起，
// 若已识别到二维码则下方出现扫描结果卡片（与 mDNS 发现互斥）
function toggleScanner(): void {
  const next = !showScanner.value
  showScanner.value = next
  // 二维码与 mDNS 发现互斥：打开相机时收起发现区，并停掉后台 mDNS 扫描
  if (next) closeDiscovery()
}

/** ScanPanel 识别到有效二维码：关闭扫描视图，结果以卡片展示在连接历史下方 */
function handleScanResult(result: QrPayload): void {
  scanResult.value = result
  showScanner.value = false
}

/** 从扫描结果卡片发起连接：WebSocket 连接 + QR token 认证（旧 `connectFromScanResult`） */
async function onQrConnect(): Promise<void> {
  const result = scanResult.value
  if (!result || isConnecting.value) return

  const device: HostDeviceTarget = {
    address: result.host,
    port: result.port,
    name: 'Desktop',
  }
  pendingDevice.value = device
  connectionError.value = ''
  showConnectLoading.value = true
  qrFlowActive = true

  try {
    // Step 1: 建立连接（扫码自带一次性 token：跳过「旧凭据静默换新」）
    await ctrl.connectDevice(device, { silentAuth: false })

    // Step 2: 等待连接建立完成（旧实现 200ms × 最多 10s 轮询，解决竞态）
    await waitForConnected(10000)

    // Step 3: 用 QR token 认证
    const ok = await ctrl.authenticateWithQr(result.token)
    if (!ok) throw new Error(t('hub.scanQrExpired'))
    scanResult.value = null
  } catch (error) {
    const msg = errText(error)
    connectionError.value = ctrl.deviceError.value || msg
    logger.error(`[host] QR connect failed: ${msg}`)
    toast.showToast(t(connectionErrorKey(msg)), 'error')
    // 连接失败时确保前后端状态一致：断开后端连接 + 重置前端状态
    await ctrl.disconnect().catch(() => {})
    await ctrl.loadHistory(true).catch(() => {})
  } finally {
    qrFlowActive = false
    showConnectLoading.value = false
  }
}

/** 等传输层连上（旧 QR 路径轮询口径：200ms 一次，超时抛错） */
async function waitForConnected(timeoutMs: number): Promise<void> {
  const start = Date.now()
  while (!ctrl.api.isConnected.value && !isAuthenticated.value) {
    if (Date.now() - start > timeoutMs) throw new Error(t('hub.scanTimeout'))
    await new Promise((resolve) => setTimeout(resolve, 200))
  }
}

// ==================== mDNS 扫描发现 ====================

/** 切换 mDNS 扫描发现区（页头雷达按钮）：展开自动开始扫描，收起停止扫描 */
function toggleDiscovery(): void {
  const next = !showDiscovery.value
  showDiscovery.value = next
  if (next) {
    showScanner.value = false
    void startMdnsScan()
  } else {
    void stopMdnsScan()
  }
}

/** 关闭扫描发现区（面板头部 × 按钮，或切换到其它模式时） */
function closeDiscovery(): void {
  showDiscovery.value = false
  void stopMdnsScan()
}

/**
 * 开始/重新开始 mDNS 扫描
 *
 * @param options.keepResults 保留已发现列表（切回连接页时的后台续扫用）
 */
async function startMdnsScan(options?: { keepResults?: boolean }): Promise<void> {
  await ctrl.startScan(options?.keepResults ?? false)
}

/** 停止 mDNS 扫描 */
async function stopMdnsScan(): Promise<void> {
  await ctrl.stopScan()
}

/** 操作栏按钮：扫描中「停止扫描」/ 已停止「重新扫描」 */
function toggleMdnsScan(): void {
  if (mdnsScanning.value) void stopMdnsScan()
  else void startMdnsScan()
}

// ==================== 连接历史维护 ====================

function removeFromHistory(address: string): void {
  void ctrl.removeHistory(address)
}

function confirmClearHistory(): void {
  showClearHistoryConfirm.value = false
  void ctrl.clearHistory().catch((e) => {
    logger.error(`[host] clear connection history failed: ${errText(e)}`)
    toast.showToast(t('hub.historyClearFailed'), 'error')
  })
}

// ==================== 断开 ====================

function handleDisconnect(): void {
  showDisconnectConfirm.value = true
}

async function confirmDisconnect(): Promise<void> {
  showDisconnectConfirm.value = false
  try {
    await ctrl.disconnect()
  } catch (e) {
    logger.error(`[host] disconnect failed: ${errText(e)}`)
  }
}

function errText(e: unknown): string {
  return e instanceof Error ? e.message : String(e)
}
</script>

<style scoped>
/* 标题下连接状态小字：比 page-subtitle 默认更小，手机上 10px、平板 11px */
.connection-status-subtitle {
  font-size: clamp(0.5625rem, 0.625rem + (100vw - 360px) / 840, 0.6875rem);
}

/* 设备图标容器：与 PluginIcon md 尺寸一致（48px, rounded-xl） */
.device-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 3rem;
  height: 3rem;
  border-radius: 0.75rem;
  flex-shrink: 0;
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

.config-list-enter-active {
  transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
}

.config-list-leave-active {
  transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
}

.config-list-enter-from {
  opacity: 0;
  transform: translateY(8px);
}

.config-list-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}

.config-list-move {
  transition: transform 0.3s cubic-bezier(0.4, 0, 0.2, 1);
}

/* 动效降级：系统开启「减少动态效果」时，本视图的模式切换/列表动效直接呈现终态 */
@media (prefers-reduced-motion: reduce) {
  .fade-enter-active,
  .fade-leave-active,
  .config-list-enter-active,
  .config-list-leave-active,
  .config-list-move {
    transition: none !important;
  }
}

/* ==================== mDNS 发现设备列表（融合自原扫描页 DiscoverView） ==================== */

.device-row {
  gap: clamp(0.5rem, 0.625rem + (100vw - 360px) / 840 * 2, 0.75rem);
  padding: clamp(0.5rem, 0.625rem + (100vw - 360px) / 840 * 2, 0.75rem) 0.75rem;
  min-height: 3rem;
}

.device-chip {
  display: flex;
  align-items: center;
  justify-content: center;
  width: clamp(1.5rem, 1.75rem + (100vw - 360px) / 840 * 4, 2rem);
  height: clamp(1.5rem, 1.75rem + (100vw - 360px) / 840 * 4, 2rem);
  border-radius: clamp(0.375rem, 0.4375rem + (100vw - 360px) / 840, 0.5rem);
  flex-shrink: 0;
}

.device-name {
  font-size: var(--font-size-base);
  font-weight: 500;
  line-height: 1.2;
  color: var(--mobile-row-title);
}

.device-addr {
  margin-top: 0.125rem;
  font-size: var(--font-size-sm);
  line-height: 1.2;
  color: var(--mobile-row-sub);
}

/* 已连接设备：非交互行，无按压反馈 */
.device-row.is-connected {
  cursor: default;
}

.device-row.is-connected:active {
  background: none;
}

.device-row.is-connected .device-chip {
  color: var(--mobile-chip-emerald);
  background: var(--mobile-chip-emerald-bg);
}

/* 发现区雷达扫射：旧实现用 Tailwind 任意值 animate-[spin_3s_linear_infinite]，
   插件构建不含该任意值动画类，改为 scoped keyframes（等价 3s 线性无限旋转） */
.radar-sweep {
  animation: radar-sweep-spin 3s linear infinite;
}

@keyframes radar-sweep-spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

@media (prefers-reduced-motion: reduce) {
  .radar-sweep {
    animation: none;
  }
}
</style>
