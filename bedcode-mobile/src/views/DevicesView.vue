<template>
  <div class="h-full flex flex-col" style="background: var(--mobile-bg-primary)">
    <!-- Header -->
    <div class="page-header flex-shrink-0">
      <div class="page-header-row">
        <div class="min-w-0 flex-1">
          <h1 class="page-title truncate">{{ t('mobile.connection.title') }}</h1>
          <!-- 连接成功后标题下方仅显示「已连接」小字，未连接/连接中显示状态文字 -->
          <p class="page-subtitle connection-status-subtitle">
            <template v-if="isConnected && currentDevice">
              <span class="inline-flex items-center gap-1.5">
                <span class="status-dot dot-emerald"></span>
                {{ t('mobile.connection.connected') }}
              </span>
            </template>
            <template v-else>
              <span
                class="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full border"
                style="border-color: color-mix(in srgb, var(--mobile-accent) 30%, transparent)"
              >
                <span
                  class="status-dot"
                  :style="{ background: connectionStatus === 'connecting' ? 'var(--mobile-accent)' : 'var(--mobile-text-muted)' }"
                ></span>
                <span class="text-[var(--mobile-text-secondary)]">{{ connectionStatusText }}</span>
              </span>
            </template>
          </p>
        </div>
        <button
          class="flex-shrink-0 p-2 -mr-2 rounded-lg transition-colors active:opacity-80"
          style="color: var(--mobile-accent)"
          :class="{ 'opacity-50': connection.isConnecting.value, 'bg-[var(--mobile-accent-muted)]': showDiscovery }"
          :disabled="connection.isConnecting.value"
          :title="t('mobile.connection.discoverDevices')"
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

    <!-- Main Content -->
    <div class="flex-1 overflow-y-auto overflow-x-hidden px-4 min-h-0">
      <!-- Connection Status (connecting / error) -->
      <Transition name="fade">
      <div
        v-if="connectionStatus === 'connecting' || connectionStatus === 'error'"
        class="mb-4"
      >
        <div class="group-card">
          <div class="group-row">
            <div v-if="connectionStatus === 'connecting'" class="w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin" style="color: var(--mobile-accent)" />
            <svg v-else class="w-5 h-5" style="color: var(--mobile-chip-red)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z" />
            </svg>
            <span class="group-row-sub">{{ connectionStatusText }}</span>
          </div>
        </div>
      </div>
      </Transition>

      <!-- Connected: 当前设备（会话配置已迁移到会话页） -->
      <div v-if="isConnected" class="pb-8">
        <div class="pt-2 space-y-3">
          <!-- Connected device info + disconnect（同行，断开按钮位于卡片右侧） -->
          <div v-if="currentDevice" class="bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl p-4 transition-colors duration-300">
            <div class="flex items-center gap-3">
              <span class="device-icon chip-emerald">
                <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
                </svg>
              </span>
              <div class="flex-1 min-w-0">
                <span class="text-base font-medium text-[var(--mobile-text-primary)] truncate block">{{ currentDevice.name }}</span>
                <p class="text-xs mt-1 font-mono text-[var(--mobile-text-muted)]">{{ currentDevice.address }}</p>
              </div>
              <!-- 断开按钮：与连接信息同行，不独占一行 -->
              <button
                class="flex-shrink-0 h-11 px-3.5 rounded-xl flex items-center gap-1.5 chip-red font-medium text-sm transition-opacity duration-300 active:opacity-80 hover:opacity-90"
                @click="handleDisconnect"
              >
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M13 10V3L4 14h7v7l9-11h-7z" />
                </svg>
                <span>{{ t('mobile.connection.disconnect') }}</span>
              </button>
            </div>
          </div>

        </div>
      </div>

      <!-- Not Connected: 内嵌扫描面板（扫描整合进连接页，替代独立扫描页） -->
      <div v-else-if="showScanner" class="pb-8">
        <ScanPanel @close="showScanner = false" @scan-result="handleScanResult" />
      </div>

      <!-- Not Connected: mDNS 扫描发现（融合进连接页：点击页头扫描按钮展开，扫描动画+结果在此展示，区域可关闭） -->
      <div v-else-if="showDiscovery" class="pb-8 pt-2">
        <!-- 区域头部：扫描发现 + × 关闭（页面标题保持「连接配对」不变，无跳转） -->
        <div class="flex items-center justify-between pb-2">
          <span class="text-sm font-semibold text-[var(--mobile-text-muted)]">
            {{ t('mobile.discover.title') }}
            <span v-if="discoveredServices.length > 0" class="ml-1 px-1.5 py-0.5 rounded-full text-xs" style="background: var(--mobile-bg-elevated); color: var(--mobile-text-secondary)">{{ discoveredServices.length }}</span>
          </span>
          <button
            class="p-2 -mr-2 rounded-lg transition-colors active:opacity-80 flex-shrink-0"
            style="color: var(--mobile-text-muted)"
            :title="t('common.button.close')"
            @click="closeDiscovery"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <!-- Scanning status（扫描动画：spinner + 状态文字 + 已发现数） -->
        <div v-if="isScanning" class="pb-3 flex items-center gap-3">
          <div class="w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin" style="color: var(--mobile-accent)" />
          <span class="group-row-sub">{{ t('mobile.discover.scanning') }}</span>
          <span v-if="discoveredServices.length > 0" class="text-sm font-medium ml-auto" style="color: var(--mobile-accent)">
            {{ t('mobile.discover.deviceFound', { count: discoveredServices.length }) }}
          </span>
        </div>

        <!-- 空态（未扫描且无设备） -->
        <div v-if="discoveredServices.length === 0 && !isScanning" class="flex flex-col items-center justify-center py-12">
          <svg class="w-12 h-12 mb-4" style="color: var(--mobile-text-disabled)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8.111 16.404a5.5 5.5 0 017.778 0M12 20h.01m-7.08-7.071c3.904-3.905 10.236-3.905 14.141 0M1.394 9.393c5.857-5.858 15.355-5.858 21.213 0" />
          </svg>
          <p class="group-row-sub mb-1">{{ t('mobile.discover.noDevices') }}</p>
          <p class="text-sm" style="color: var(--mobile-text-disabled)">{{ t('mobile.discover.noDevicesHint') }}</p>
        </div>

        <!-- 扫描空态动画（雷达扫射） -->
        <div v-if="discoveredServices.length === 0 && isScanning" class="flex flex-col items-center justify-center py-12">
          <div class="relative mb-6">
            <div class="w-28 h-28 rounded-full relative" style="border: 2px solid color-mix(in srgb, var(--mobile-accent) 20%, transparent)">
              <div class="absolute inset-2 rounded-full" style="border: 1px solid color-mix(in srgb, var(--mobile-accent) 10%, transparent)"></div>
              <div class="absolute inset-4 rounded-full" style="border: 1px solid color-mix(in srgb, var(--mobile-accent) 5%, transparent)"></div>
              <div class="absolute inset-0 animate-[spin_3s_linear_infinite] origin-center">
                <div class="w-1/2 h-0.5 absolute top-1/2 left-1/2 -translate-y-1/2" style="background: linear-gradient(to right, var(--mobile-accent), transparent)"></div>
              </div>
              <div class="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-3 h-3 rounded-full" style="background: var(--mobile-accent)"></div>
            </div>
          </div>
          <p class="group-row-sub">{{ t('mobile.discover.scanning') }}</p>
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
            <span class="device-chip chip-cyan">
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
              </svg>
            </span>
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2 min-w-0">
                <span class="device-name truncate">{{ service.device_name }}</span>
                <span
                  class="status-badge ml-auto"
                  :class="isDiscoverCurrentDevice(service) ? 'badge-emerald' : 'badge-cyan'"
                >
                  <span v-if="isDiscoverCurrentDevice(service)" class="status-dot dot-emerald"></span>
                  {{ isDiscoverCurrentDevice(service) ? t('mobile.discover.connected') : t('mobile.discover.connectToDevice') }}
                </span>
              </div>
              <div class="device-addr font-mono truncate">{{ service.address }}:{{ service.port }}</div>
            </div>
          </div>
        </div>
      </div>

      <!-- Not Connected: History + Actions -->
      <div v-else class="pb-8">
        <div class="pt-2 space-y-3">
          <!-- Connection History header（带条数） -->
          <div class="flex items-center justify-between pt-2">
            <span class="text-sm font-semibold text-[var(--mobile-text-muted)]">
              {{ t('mobile.connection.connectionHistory') }}
              <span v-if="connectionHistory.length > 0" class="ml-1 px-1.5 py-0.5 rounded-full text-xs" style="background: var(--mobile-bg-elevated); color: var(--mobile-text-secondary)">{{ connectionHistory.length }}</span>
            </span>
            <button
              v-if="connectionHistory.length > 0"
              class="text-sm transition-colors active:opacity-80"
              style="color: var(--mobile-text-muted)"
              @click="showClearHistoryConfirm = true"
            >
              {{ t('mobile.connection.clearHistory') }}
            </button>
          </div>

          <div v-if="connectionHistory.length === 0" class="text-center py-8">
            <p class="text-sm" style="color: var(--mobile-text-disabled)">{{ t('mobile.connection.noHistory') }}</p>
          </div>

          <TransitionGroup v-else name="config-list" tag="div" class="space-y-3">
            <button
              v-for="item in connectionHistory"
              :key="item.address"
              class="w-full bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl p-4 text-left cursor-pointer transition-[border-color,opacity] duration-300 active:opacity-90 hover:border-[var(--mobile-border-hover)]"
              :disabled="connection.isConnecting.value"
              :class="{ 'opacity-50 pointer-events-none': connection.isConnecting.value }"
              @click="handleConnectFromHistory(item)"
            >
              <div class="flex items-center gap-3">
                <span class="device-icon chip-cyan">
                  <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
                  </svg>
                </span>
                <div class="flex-1 min-w-0">
                  <div class="text-base font-medium text-[var(--mobile-text-primary)] truncate">{{ item.name || item.address }}</div>
                  <p class="text-xs mt-1 font-mono text-[var(--mobile-text-muted)]">{{ item.address }}</p>
                  <p class="text-xs mt-0.5" style="color: var(--mobile-text-disabled)">{{ formatLastConnected(item.lastConnected) }}</p>
                </div>
                <button
                  class="p-1.5 rounded-lg transition-colors active:opacity-80 flex-shrink-0"
                  style="color: var(--mobile-text-disabled)"
                  :disabled="connection.isConnecting.value"
                  @click.stop="removeFromHistory(item.address)"
                >
                  <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </div>
            </button>
          </TransitionGroup>

          <!-- 扫描结果：与连接历史同级别的独立区块（识别到二维码停止扫描后出现），可关闭 -->
          <div v-if="scanResult" class="pt-4">
            <div class="flex items-center justify-between pb-2">
              <span class="text-sm font-semibold text-[var(--mobile-text-muted)]">
                {{ t('mobile.scan.scanResult') }}
              </span>
              <button
                class="p-2 -mr-2 rounded-lg transition-colors active:opacity-80 flex-shrink-0"
                style="color: var(--mobile-text-muted)"
                :title="t('common.button.close')"
                @click="scanResult = null"
              >
                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
            <button
              class="w-full bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-xl p-4 text-left cursor-pointer transition-[border-color,opacity] duration-300 active:opacity-90 hover:border-[var(--mobile-border-hover)]"
              :disabled="connection.isConnecting.value"
              :class="{ 'opacity-50 pointer-events-none': connection.isConnecting.value }"
              @click="connectFromScanResult"
            >
              <div class="flex items-center gap-3">
                <span class="device-icon chip-emerald">
                  <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M12 4v1m6 11h2m-6 0h-2m0 0H8m4 0h4m-4-8a1 1 0 011-1h1.586a1 1 0 01.707.293l3.828 3.828a1 1 0 01.293.707V17a1 1 0 01-1 1H8a1 1 0 01-1-1V7a1 1 0 011-1z" />
                  </svg>
                </span>
                <div class="flex-1 min-w-0">
                  <div class="text-base font-medium text-[var(--mobile-text-primary)] truncate">Desktop</div>
                  <p class="text-xs mt-1 font-mono text-[var(--mobile-text-muted)]">{{ scanResult.host }}:{{ scanResult.port }}</p>
                </div>
              </div>
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- Action Buttons (when not connected)：mDNS 扫描发现 / 二维码扫描 / 默认 三种模式互斥 -->
    <div v-if="!isConnected" class="flex-shrink-0 p-4 space-y-3" style="padding-bottom: max(1rem, var(--safe-area-bottom, 0px))">
      <!-- mDNS 扫描发现：扫描中「停止扫描」/ 已停止「重新扫描」 -->
      <button
        v-if="showDiscovery"
        class="w-full h-11 rounded-xl text-sm font-medium transition-colors active:opacity-80"
        style="background: var(--mobile-group-bg); border: 1px solid var(--mobile-group-border); color: var(--mobile-text-secondary)"
        :class="{ 'opacity-50': connection.isConnecting.value }"
        :disabled="connection.isConnecting.value"
        @click="toggleMdnsScan"
      >
        {{ isScanning ? t('mobile.discover.stopScan') : t('mobile.discover.restartScan') }}
      </button>

      <!-- 二维码扫描：启动后按钮切换为「停止扫描」（红色） -->
      <button
        v-else-if="showScanner"
        class="w-full h-11 rounded-xl text-base font-medium transition-colors active:opacity-80 flex items-center justify-center gap-2"
        style="background: var(--mobile-chip-red-bg); color: var(--mobile-chip-red); border: 1px solid color-mix(in srgb, var(--mobile-chip-red) 25%, transparent)"
        :class="{ 'opacity-50': connection.isConnecting.value }"
        :disabled="connection.isConnecting.value"
        @click="toggleScanner"
      >
        <svg class="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
          <rect x="6" y="6" width="12" height="12" rx="2" />
        </svg>
        {{ t('mobile.scan.stopScan') }}
      </button>

      <template v-else>
        <button
          class="w-full h-11 rounded-xl text-base font-medium transition-colors active:opacity-80 flex items-center justify-center gap-2"
          style="background: var(--mobile-accent); color: var(--mobile-text-on-accent)"
          :class="{ 'opacity-50': connection.isConnecting.value }"
          :disabled="connection.isConnecting.value"
          @click="toggleScanner"
        >
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v1m6 11h2m-6 0h-2m0 0H8m4 0h4m-4-8a1 1 0 011-1h1.586a1 1 0 01.707.293l3.828 3.828a1 1 0 01.293.707V17a1 1 0 01-1 1H8a1 1 0 01-1-1V7a1 1 0 011-1z" />
          </svg>
          {{ t('mobile.connection.scanConnect') }}
        </button>
        <button
          class="w-full h-11 rounded-xl text-base font-medium transition-colors active:opacity-80 flex items-center justify-center gap-2"
          style="background: var(--mobile-group-bg); color: var(--mobile-text-secondary); border: 1px solid var(--mobile-group-border)"
          :class="{ 'opacity-50': connection.isConnecting.value }"
          :disabled="connection.isConnecting.value"
          @click="showManualConnect = true"
        >
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13.828 10.172a4 4 0 00-5.656 0l-4 4a4 4 0 105.656 5.656l1.102-1.101m-.758-4.899a4 4 0 005.656 0l4-4a4 4 0 00-5.656-5.656l-1.1 1.1" />
          </svg>
          {{ t('mobile.connection.manualConnect') }}
        </button>
      </template>
    </div>

    <!-- Manual Connect Dialog -->
    <BottomSheet
      v-model="showManualConnect"
      :title="t('mobile.connection.connectNewDevice')"
      :placeholder="t('mobile.connection.addressPlaceholder')"
      :loading="connection.isConnecting.value"
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
    <Modal v-model="showClearHistoryConfirm" :title="t('mobile.connection.clearHistory')" size="sm">
      <p style="color: var(--mobile-text-disabled)">
        {{ t('mobile.connection.clearHistoryConfirm') }}
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showClearHistoryConfirm = false">{{ t('common.button.cancel') }}</Button>
          <Button variant="danger" @click="confirmClearHistory">{{ t('common.button.confirm') }}</Button>
        </div>
      </template>
    </Modal>

    <!-- Disconnect Confirmation Modal -->
    <Modal v-model="showDisconnectConfirm" :title="t('mobile.connection.disconnect')" size="sm">
      <p style="color: var(--mobile-text-disabled)">
        {{ t('mobile.connection.confirmDisconnectMsg') }}
      </p>
      <template #footer>
        <div class="flex justify-end gap-3">
          <Button variant="ghost" @click="showDisconnectConfirm = false">{{ t('common.button.cancel') }}</Button>
          <Button variant="danger" @click="confirmDisconnect">{{ t('mobile.connection.disconnect') }}</Button>
        </div>
      </template>
    </Modal>

    <!-- 全局遮罩 Loading -->
    <Teleport to="body">
      <Transition name="fade">
        <div
          v-if="showPairingLoading"
          class="fixed inset-0 z-[100] flex items-center justify-center backdrop-blur-sm mobile-ui"
          style="background: var(--mobile-overlay)"
        >
          <div class="rounded-2xl p-6 shadow-xl flex flex-col items-center gap-4 min-w-[200px]" style="background: var(--mobile-group-bg)">
            <div class="w-10 h-10 border-4 border-current border-t-transparent rounded-full animate-spin" style="color: var(--mobile-accent)" />
            <p class="text-sm font-medium" style="color: var(--mobile-text-secondary)">{{ t('mobile.connection.pairingRequest') }}</p>
          </div>
        </div>
      </Transition>
    </Teleport>

    <!-- Loading Dialog: 连接中（弹窗遮罩，点击连接历史/扫码/手动连接后展示，阻断重复点击） -->
    <LoadingDialog
      :visible="showConnectLoading"
      :message="pendingDevice?.name
        ? t('mobile.connection.connecting', { name: pendingDevice.name })
        : t('mobile.connection.connectingPlain')"
    />

  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onActivated, onDeactivated, watch } from 'vue'
import { logger } from '@/utils/frontendLogger'
import { useI18n } from 'vue-i18n'
import { useMobileConnection, type RemoteDevice } from '@/composables/useMobileConnection'
import { useMobileSettings } from '@/composables/useMobileSettings'
import { useMdnsDiscovery, type DiscoveredService } from '@/composables/useMdnsDiscovery'
import { wsGetBiometricKeyStatus, wsAuthenticateWithQr } from '@/composables/useMobileCommands'
import { useToast } from '@/composables/useToast'
import BottomSheet from '@/components/BottomSheet.vue'
import PairingInput from '@/components/PairingInput.vue'
import BiometricAuthDialog from '@/components/BiometricAuthDialog.vue'
import Modal from '@/components/Modal.vue'
import Button from '@/components/Button.vue'
import LoadingDialog from '@/components/LoadingDialog.vue'
import ScanPanel from '@/components/ScanPanel.vue'

const connection = useMobileConnection()
const { settings: mobileSettings } = useMobileSettings()
const toast = useToast()
const { t } = useI18n()

// 使用全局状态
const connectionHistory = connection.connectionHistory

// 内嵌扫描面板开关（扫描整合进连接页；连接成功后视图自动切换，面板随之卸载）
const showScanner = ref(false)

// mDNS 扫描发现开关（融合进连接页：点击页头扫描按钮展开，扫描动画+结果在当前页展示，区域可关闭）
const showDiscovery = ref(false)

// mDNS 发现状态（全局单例 composable，扫描状态跨页面共享）
const { discoveredServices, isScanning, startDiscovery, stopDiscovery } = useMdnsDiscovery()

// 扫描结果：识别到有效二维码后停止扫描，结果以卡片展示在连接历史下方（同级别区块，可关闭）
interface QrScanResult {
  host: string
  port: number
  token: string
}
const scanResult = ref<QrScanResult | null>(null)

const showManualConnect = ref(false)
const showPairing = ref(false)
const showPairingLoading = ref(false)  // 全局遮罩 loading（配对请求时）
const showConnectLoading = ref(false)  // 连接中弹窗遮罩（点击连接后展示，阻断重复点击）
const isPairing = ref(false)
const pairingError = ref('')
const connectionError = ref('')

// 认证弹窗（JWT 失效后：根据认证设置直接弹对应弹窗，不再选择；配对码兜底，生物认证便捷）
const showBiometricDialog = ref(false)
const authBiometricAvailable = ref(false)
const authDialogError = ref('')
const authDialogLoading = ref(false)

// Current device being connected
const pendingDevice = ref<RemoteDevice | null>(null)

// 使用后端统一的连接状态
const connectionStatus = computed(() => connection.connectionStatus.value)

// 使用统一的连接状态
const isConnected = computed(() => connection.connectionStatus.value === 'connected' || connection.connectionStatus.value === 'paired')

// 当前连接的设备
const currentDevice = computed(() => connection.currentDevice.value)

// 活跃会话 ID
const activeSessionId = computed(() => connection.activeSessionId.value)

const connectionStatusText = computed(() => {
  switch (connection.connectionStatus.value) {
    case 'connecting':
      return pendingDevice.value?.name
        ? t('mobile.connection.connecting', { name: pendingDevice.value.name })
        : t('mobile.connection.connectingPlain')
    case 'connected':
      return t('mobile.connection.pairing')
    case 'pairing':
      return t('mobile.connection.enterCode')
    case 'paired':
      return t('mobile.connection.authenticated')
    case 'error':
      // connectionError 可能是 i18n key（如 'mobile.connection.unreachable'）或原始错误字符串
      // t() 对未知 key 返回原字符串，因此两种情况都能正常显示
      return connectionError.value ? t(connectionError.value) : t('mobile.connection.connectFailed')
    default:
      return connection.connectionStatus.value === 'disconnected' ? t('mobile.connection.notConnected') : ''
  }
})

// 使用全局连接历史方法
function removeFromHistory(address: string) {
  connection.removeFromConnectionHistory(address)
}

function clearHistory() {
  connection.clearConnectionHistory()
}

// 清除连接历史（带确认弹窗，防误触）
const showClearHistoryConfirm = ref(false)

function confirmClearHistory() {
  showClearHistoryConfirm.value = false
  connection.clearConnectionHistory()
}

// ==================== 相对时间格式化（连接历史 meta） ====================

/** 上次连接时间 → 友好相对文案（刚刚 / x 分钟前 / x 小时前 / x 天前 / 日期） */
function formatLastConnected(iso?: string): string {
  if (!iso) return ''
  const ts = new Date(iso).getTime()
  if (Number.isNaN(ts)) return ''
  const diffMs = Date.now() - ts
  const minutes = Math.floor(diffMs / 60000)
  if (minutes < 1) return t('mobile.time.justNow')
  if (minutes < 60) return t('mobile.time.minutesAgo', { count: minutes })
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return t('mobile.time.hoursAgo', { count: hours })
  const days = Math.floor(hours / 24)
  if (days < 30) return t('mobile.time.daysAgo', { count: days })
  const d = new Date(ts)
  return `${String(d.getFullYear())}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
}

// 从 Discover/终端等页面返回时重新加载连接历史（force=true，本地状态可能在其它页面更新）
onActivated(() => {
  connection.loadConnectionHistory(true)
  // 切回连接页时若扫描发现区仍展开，恢复扫描（keep-alive 激活时 onMounted 不会重新触发）
  if (showDiscovery.value) {
    startMdnsScan()
  }
})

onMounted(() => {
  connection.loadConnectionHistory()
})

// 切走连接页时停掉 mDNS 扫描（keep-alive 缓存页面，避免后台持续扫描）
onDeactivated(() => {
  stopMdnsScan()
})

// 监听连接状态变化，认证完成时加载会话数据
// 注意：重连场景下，ws_paired 事件会触发 status 变为 paired，此时需要重新加载会话
watch([isConnected, connection.connectionStatus], async ([connected, status], [oldConnected, oldStatus]) => {
  // 认证完成时加载会话数据（包括首次连接和重连）
  if (connected && status === 'paired') {
    // 如果是从非 paired 状态变为 paired，或者从断开变为连接，都需要加载
    if (oldStatus !== 'paired' || !oldConnected) {
      logger.log('[DevicesView] Status changed to paired, loading sessions...')
      await connection.loadSessionConfigs()
      await connection.loadActiveSessions()
    }
  }
})

// 连接成功后收起两个扫描视图（二维码 / mDNS），断开后回到历史列表布局
watch(isConnected, (connected) => {
  if (connected) {
    showScanner.value = false
    showDiscovery.value = false
  }
})

// Connect from history
async function handleConnectFromHistory(item: any) {
  // 连接中禁止重复点击（弹窗遮罩已阻断，此处兜底）
  if (connection.isConnecting.value) return

  const [host, portStr] = item.address.split(':')
  const savedSettings = JSON.parse(localStorage.getItem('mobile-settings') || '{}')
  const defaultPort = savedSettings.defaultPort || 8765
  const port = portStr ? parseInt(portStr) : defaultPort

  const device: RemoteDevice = {
    id: `${host}:${port}`,
    name: item.name,
    address: host,
    port,
    isPaired: false,
  }

  // 清理残留的会话数据（connect() 内部会处理断开旧连接）
  connection.clearSessionConfigs()
  connection.clearActiveSessions()

  // 从历史连接，允许使用已存储的 token 跳过配对
  await startConnection(device, true)
}

// Manual address input
async function handleConnectManual(address: string) {
  // 连接中禁止重复点击
  if (connection.isConnecting.value) return

  const [host, portStr] = address.split(':')
  const savedSettings = JSON.parse(localStorage.getItem('mobile-settings') || '{}')
  const defaultPort = savedSettings.defaultPort || 8765
  const port = portStr ? parseInt(portStr) : defaultPort

  const device: RemoteDevice = {
    id: `${host}:${port}`,
    name: host,
    address: host,
    port,
    isPaired: false,
  }

  // 关闭手动连接弹窗，后续由 PairingInput 接管
  showManualConnect.value = false

  // 清理残留的会话数据（connect() 内部会处理断开旧连接）
  connection.clearSessionConfigs()
  connection.clearActiveSessions()

  // 手动连接，必须走配对流程
  await startConnection(device, false)
}

// 切换二维码扫描：非扫描态点击「扫码连接」打开内嵌相机；扫描中点击「停止扫描」收起，
// 若已识别到二维码则下方出现扫描结果卡片（与 mDNS 发现互斥）
function toggleScanner() {
  showScanner.value = !showScanner.value
  if (showScanner.value) {
    showDiscovery.value = false
  }
}

/** ScanPanel 识别到有效二维码：关闭扫描视图，结果以卡片展示在连接历史下方 */
function handleScanResult(result: QrScanResult) {
  scanResult.value = result
  showScanner.value = false
}

// ==================== mDNS 扫描发现（融合进连接页） ====================

/** 切换 mDNS 扫描发现区（页头雷达按钮）：展开自动开始扫描，收起停止扫描 */
function toggleDiscovery() {
  showDiscovery.value = !showDiscovery.value
  if (showDiscovery.value) {
    showScanner.value = false
    startMdnsScan()
  } else {
    stopMdnsScan()
  }
}

/** 关闭扫描发现区（× 按钮） */
function closeDiscovery() {
  showDiscovery.value = false
  stopMdnsScan()
}

/** 开始/重新开始 mDNS 扫描 */
async function startMdnsScan() {
  try {
    await startDiscovery()
  } catch (e) {
    logger.error('[DevicesView] mDNS start failed:', e)
  }
}

/** 停止 mDNS 扫描 */
async function stopMdnsScan() {
  await stopDiscovery()
}

/** 操作栏按钮：扫描中「停止扫描」/ 已停止「重新扫描」 */
function toggleMdnsScan() {
  if (isScanning.value) {
    stopMdnsScan()
  } else {
    startMdnsScan()
  }
}

/** 点击发现的设备：直接走页面级连接流程（融合后不再经路由 state 传递） */
function handleDiscoverConnect(service: DiscoveredService) {
  if (connection.isConnected.value && connection.currentDevice.value?.address === service.address) return
  if (connection.isConnecting.value) return

  const device: RemoteDevice = {
    id: `${service.address}:${service.port}`,
    name: service.device_name,
    address: service.address,
    port: service.port,
    isPaired: false,
  }

  connection.clearSessionConfigs()
  connection.clearActiveSessions()
  startConnection(device, true)
}

/** 该发现设备是否为当前已连接的设备（连接态行样式 + 点击守卫） */
function isDiscoverCurrentDevice(service: DiscoveredService): boolean {
  return connection.isConnected.value && connection.currentDevice.value?.address === service.address
}

// Start connection flow
// @param skipPairing - 如果为 true，尝试使用已存储的 token 跳过配对流程
//                       如果为 false，必须走配对流程（手动连接场景）
async function startConnection(device: RemoteDevice, skipPairing: boolean = false) {
  pendingDevice.value = device
  connectionError.value = ''
  connection.isConnecting.value = true
  // 连接中弹窗遮罩（连接/认证阶段展示；进入配对/生物认证弹窗后由各自 UI 接管）
  showConnectLoading.value = true

  logger.log('[DevicesView] startConnection: Step 1 connect...')
  console.time('startConnection')

  try {
    // Step 1: Connect to device - 带前端超时保护
    // Rust 端有 10 秒超时，前端额外设置 12 秒超时作为兜底
    const connectTimeout = new Promise<never>((_, reject) =>
      setTimeout(() => reject(new Error(t('mobile.connection.timeout'))), 12000)
    )

    await Promise.race([
      connection.connect(device),
      connectTimeout,
    ])
    logger.log('[DevicesView] startConnection: Step 1 done')

    // Step 2: 如果允许跳过配对，尝试使用已存储的 JWT token
    if (skipPairing) {
      logger.log('[DevicesView] startConnection: Step 2 authenticate (skipPairing=true)...')
      const authenticated = await connection.authenticate()
      logger.log('[DevicesView] startConnection: Step 2 done, authenticated=', authenticated)
      if (authenticated) {
        pendingDevice.value = null
        connection.addToConnectionHistory(`${device.address}:${device.port}`, device.name)
        await connection.loadSessionConfigs()
        return
      }
    } else {
      logger.log('[DevicesView] startConnection: Step 2 skipped (skipPairing=false, must pair)')
    }

    // Step 2.5: JWT 认证失败（或手动连接）→ 根据认证设置直接弹出对应认证弹窗。
    // 设置优先生物认证且已绑定 → 弹生物认证弹窗并自动触发指纹；否则配对码兜底。
    const keyStatus = await wsGetBiometricKeyStatus().catch(() => null)
    authBiometricAvailable.value = !!(keyStatus?.deviceSupported && keyStatus?.hasKey)
    authDialogError.value = ''
    pairingError.value = ''
    logger.log('[DevicesView] startConnection: Step 2.5 auth, preferred=', mobileSettings.value.preferredAuthMethod, 'canBiometric=', authBiometricAvailable.value)

    const preferBiometric = mobileSettings.value.preferredAuthMethod === 'biometric' && authBiometricAvailable.value
    if (preferBiometric) {
      await openBiometricAuth()
    } else {
      await startPairingFlow()
    }
  } catch (error) {
    connectionError.value = String(error)
    logger.error('[DevicesView] startConnection failed:', error)

    // 显示友好的错误提示
    const errorMsg = String(error)
    if (errorMsg.includes('timeout') || errorMsg.includes('超时')) {
      toast.error(t('mobile.connection.timeoutToast'))
    } else if (errorMsg.includes('refused') || errorMsg.includes('rejected')) {
      toast.error(t('mobile.connection.refusedToast'))
    } else if (errorMsg.includes('unreachable') || errorMsg.includes('network')) {
      toast.error(t('mobile.connection.unreachableToast'))
    } else {
      toast.error(t('mobile.connection.connectFailedToast', { error: errorMsg }))
    }

    // 连接失败时确保前后端状态一致：断开后端连接 + 重置前端状态
    await connection.disconnect()
    connectionError.value = String(error)
  } finally {
    console.timeEnd('startConnection')
    connection.isConnecting.value = false
    showConnectLoading.value = false  // 确保在任何情况下都隐藏 loading
    showPairingLoading.value = false  // 确保在任何情况下都隐藏 loading
  }
}

// 从扫描结果卡片发起连接：WebSocket 连接 + QR token 认证（原 ScanPanel 内逻辑迁入，统一走页面级 loading 与错误提示）
async function connectFromScanResult() {
  const result = scanResult.value
  if (!result || connection.isConnecting.value) return

  const device: RemoteDevice = {
    id: `qr-${Date.now()}`,
    name: 'Desktop',
    address: result.host,
    port: result.port,
    isPaired: false,
  }

  pendingDevice.value = device
  connectionError.value = ''
  connection.isConnecting.value = true
  showConnectLoading.value = true

  try {
    // Step 1: 建立 WebSocket 连接
    await connection.connect(device)

    // 等待连接建立完成（解决竞态问题）
    const maxWaitTime = 10000 // 最多等待 10 秒
    const checkInterval = 200 // 每 200ms 检查一次
    const startTime = Date.now()
    while (!connection.isConnected.value) {
      if (Date.now() - startTime > maxWaitTime) {
        throw new Error(t('mobile.scan.timeout'))
      }
      await new Promise(resolve => setTimeout(resolve, checkInterval))
    }

    // Step 2: 发送 QR token 认证
    logger.log('[DevicesView] QR connect, auth with token...')
    const creds = await wsAuthenticateWithQr(result.token)
    if (!creds) {
      throw new Error(t('mobile.scan.qrExpired'))
    }
    connection.saveCredentials(creds)
    connection.addToConnectionHistory(`${device.address}:${device.port}`, device.name)
    scanResult.value = null
    pendingDevice.value = null
  } catch (error) {
    connectionError.value = String(error)
    logger.error('[DevicesView] QR connect failed:', error)

    const errorMsg = String(error)
    if (errorMsg.includes('timeout') || errorMsg.includes('超时')) {
      toast.error(t('mobile.connection.timeoutToast'))
    } else if (errorMsg.includes('refused') || errorMsg.includes('rejected')) {
      toast.error(t('mobile.connection.refusedToast'))
    } else if (errorMsg.includes('unreachable') || errorMsg.includes('network')) {
      toast.error(t('mobile.connection.unreachableToast'))
    } else {
      toast.error(errorMsg)
    }

    // 连接失败时确保前后端状态一致：断开后端连接 + 重置前端状态
    await connection.disconnect()
  } finally {
    connection.isConnecting.value = false
    showConnectLoading.value = false
  }
}

// Cancel connection
async function handleCancelConnection() {
  showConnectLoading.value = false
  await connection.cancelConnection()
  connection.isConnecting.value = false
  connectionError.value = t('mobile.connection.userCancelled')
}

// 打开生物认证弹窗并立即触发指纹验证（打开即弹系统生物识别）
async function openBiometricAuth() {
  // 连接中弹窗让位于生物认证弹窗
  showConnectLoading.value = false
  showBiometricDialog.value = true
  authDialogError.value = ''
  await runBiometricAuth()
}

// 生物认证：指纹识别成功后才走挑战-应答拿到密钥配对，失败留在弹窗内可重试或切换
async function runBiometricAuth() {
  const device = pendingDevice.value
  if (!device) return

  authDialogLoading.value = true
  authDialogError.value = ''
  try {
    const bioOk = await connection.authenticateWithBiometric()
    if (bioOk) {
      showBiometricDialog.value = false
      pendingDevice.value = null
      connection.addToConnectionHistory(`${device.address}:${device.port}`, device.name)
      await connection.loadSessionConfigs()
      return
    }
    // 生物认证失败/取消 → 弹窗内展示错误，可重试指纹或切换配对码
    authDialogError.value = t('mobile.connection.biometricFailed')
  } catch (e) {
    logger.error('[DevicesView] Biometric auth error:', e)
    authDialogError.value = biometricAuthErrorText(e)
  } finally {
    authDialogLoading.value = false
  }
}

// 将桌面端拒绝原因（AppError::Auth 透传）映射为友好 i18n 文案
function biometricAuthErrorText(e: unknown): string {
  const msg = e instanceof Error ? e.message : String(e)
  // 桌面端未绑定该设备的生物凭证（绑定在另一实例/数据被重置）：提示重新绑定或改用配对码
  if (/CREDENTIAL_NOT_BOUND|NOT_PAIRED|credential not bound|device not paired/i.test(msg)) {
    return t('mobile.connection.biometricNotBoundOnDesktop')
  }
  // 挑战值过期/重复使用/验签失败
  if (/CHALLENGE_INVALID|SIGNATURE_INVALID|challenge|signature/i.test(msg)) {
    return t('mobile.connection.biometricFailed')
  }
  return msg
}

// 配对码认证（兑底方式）：请求配对码后进入输入弹窗
async function startPairingFlow() {
  const device = pendingDevice.value
  if (!device) return

  // 连接中弹窗让位于配对请求弹窗
  showConnectLoading.value = false
  showPairingLoading.value = true
  try {
    const pairingTimeout = new Promise<never>((_, reject) =>
      setTimeout(() => reject(new Error(t('mobile.connection.pairingTimeout'))), 15000)
    )
    await Promise.race([connection.requestPairing(), pairingTimeout])
    showPairing.value = true
    connection.addToConnectionHistory(`${device.address}:${device.port}`, device.name)
  } catch (pairingError) {
    // 配对失败或超时时断开连接
    logger.error('[DevicesView] Pairing failed:', pairingError)
    connectionError.value = String(pairingError)
    toast.error(String(pairingError))
    await connection.disconnect()
  } finally {
    showPairingLoading.value = false
  }
}

// 配对码弹窗 → 切换到生物认证；未绑定生物认证时提示，无法切换
async function handleSwitchToBiometric() {
  if (!authBiometricAvailable.value) {
    toast.warning(t('mobile.connection.biometricNotBound'))
    return
  }
  showPairing.value = false
  pairingError.value = ''
  await openBiometricAuth()
}

// 生物认证弹窗 → 切换到配对码（兑底方式）
async function handleSwitchToPairing() {
  showBiometricDialog.value = false
  authDialogError.value = ''
  await startPairingFlow()
}

// 用户关闭生物认证弹窗 → 断开连接，保持前后端状态一致
function handleBiometricDialogClose() {
  showBiometricDialog.value = false
  authDialogError.value = ''
  connectionError.value = t('mobile.connection.userCancelled')
  connection.disconnect()
}

// 用户关闭配对码弹窗 → 断开连接，保持前后端状态一致
function handlePairingClose() {
  showPairing.value = false
  pairingError.value = ''
  connectionError.value = t('mobile.connection.userCancelled')
  connection.disconnect()
}

// Verify pairing code
async function handlePairingSubmit(code: string) {
  if (!pendingDevice.value) return

  isPairing.value = true
  pairingError.value = ''

  try {
    const success = await connection.verifyPairingCode(code)

    if (success) {
      showPairing.value = false
      pendingDevice.value = null

      // Load session configs instead of navigating to terminal
      await connection.loadSessionConfigs()
      // Also fetch active sessions for the Sessions tab
      await connection.loadActiveSessions()
    } else {
      pairingError.value = t('mobile.connection.codeVerifyFailed')
    }
  } catch (error) {
    pairingError.value = String(error)
  } finally {
    isPairing.value = false
  }
}

const showDisconnectConfirm = ref(false)

async function handleDisconnect() {
  showDisconnectConfirm.value = true
}

async function confirmDisconnect() {
  showDisconnectConfirm.value = false
  await connection.disconnect()
  connection.clearSessionConfigs()
  connection.clearActiveSessions()
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
</style>
