<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- ==================== 工具栏页头 ==================== -->
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
            d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"
          />
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="1.5"
            d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"
          />
        </svg>
        <h2 class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)]">
          {{ t('settings.title') }}
        </h2>
      </div>
      <div class="flex items-center gap-2">
        <PluginPageToolbar target="settings" />
        <button class="wb-btn-ghost" @click="handleCheckUpdate">
          {{ getUpdateStatusText() }}
        </button>
      </div>
    </div>

    <!-- 内容区：按功能分 section，section 间 24px -->
    <div class="flex-1 overflow-auto px-6 py-6">
      <div
        class="lang-fade-content max-w-3xl mx-auto space-y-6"
        :class="{ 'lang-fading': langFading }"
        :style="{ transitionDuration: animationsEnabled ? '0.4s' : '0s' }"
      >
        <!-- ==================== APPEARANCE ==================== -->
        <section>
          <h3 class="wb-section-title">{{ t('settings.ui.title') }}</h3>
          <div
            class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)]"
          >
            <!-- 主题：分段控件 -->
            <div class="px-5 py-3.5 flex items-center justify-between gap-4">
              <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
                t('settings.appearance.theme')
              }}</span>
              <div
                class="flex border border-[var(--border-strong)] rounded-md overflow-hidden flex-shrink-0"
              >
                <button
                  v-for="opt in themeOptions"
                  :key="opt.value"
                  class="h-8 px-3 text-xs font-medium transition-colors"
                  :class="
                    themeValue === opt.value
                      ? 'bg-[var(--color-primary)] text-[var(--color-primary-contrast)]'
                      : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'
                  "
                  @click="themeValue = opt.value"
                >
                  {{ opt.label }}
                </button>
              </div>
            </div>

            <!-- 主题色板：调色台（色板卡片，切换即时生效） -->
            <div class="px-5 py-3.5 flex items-start justify-between gap-6">
              <div class="flex-shrink-0">
                <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
                  t('settings.appearance.palette')
                }}</span>
                <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
                  {{ t('settings.appearance.paletteDesc') }}
                </p>
              </div>
              <div class="flex items-start gap-2 flex-wrap justify-end">
                <button
                  v-for="opt in paletteOptions"
                  :key="opt.value"
                  class="w-[84px] rounded-[8px] border p-1.5 transition-colors"
                  :class="
                    paletteValue === opt.value
                      ? 'border-[var(--color-primary)] bg-[var(--color-primary-light)]'
                      : 'border-[var(--border-strong)] hover:border-[var(--text-tertiary)]'
                  "
                  :title="opt.label"
                  @click="paletteValue = opt.value"
                >
                  <!-- 色块预览：页面底 / 卡片底 / 强调色（取色板自身色值，预览切换后效果） -->
                  <div class="flex gap-1">
                    <span
                      class="w-4 h-4 rounded-[3px] border border-black/5"
                      :style="{ background: opt.swatches.page }"
                    ></span>
                    <span
                      class="w-4 h-4 rounded-[3px] border border-black/5"
                      :style="{ background: opt.swatches.card }"
                    ></span>
                    <span
                      class="w-4 h-4 rounded-[3px] border border-black/5"
                      :style="{ background: opt.swatches.primary }"
                    ></span>
                  </div>
                  <p
                    class="text-[calc(10px*var(--ui-scale))] text-[var(--text-secondary)] mt-1.5 text-center truncate"
                  >
                    {{ opt.label }}
                  </p>
                </button>
              </div>
            </div>

            <!-- 语言：分段控件 -->
            <div class="px-5 py-3.5 flex items-center justify-between gap-4">
              <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
                t('settings.appearance.language')
              }}</span>
              <div
                class="flex border border-[var(--border-strong)] rounded-md overflow-hidden flex-shrink-0"
              >
                <button
                  v-for="opt in languageOptions"
                  :key="opt.value"
                  class="h-8 px-4 text-xs font-medium transition-colors"
                  :class="
                    currentLanguage === opt.value
                      ? 'bg-[var(--color-primary)] text-[var(--color-primary-contrast)]'
                      : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'
                  "
                  @click="switchLanguage(opt.value)"
                >
                  {{ opt.label }}
                </button>
              </div>
            </div>

            <!-- 全局字体大小（终端字体在终端设置中独立配置）：小/正常/大/超大 档位间无级滑动 -->
            <div class="px-5 py-3.5 flex items-center justify-between gap-4">
              <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
                t('settings.appearance.fontSize')
              }}</span>
              <div class="w-64 flex-shrink-0">
                <div class="flex items-center gap-3">
                  <div class="flex-1">
                    <input
                      type="range"
                      :min="MIN_FONT_SIZE"
                      :max="MAX_FONT_SIZE"
                      step="1"
                      :value="settingsStore.settings.ui.font_size"
                      class="w-full h-1 appearance-none bg-[var(--border-strong)] cursor-pointer accent-[var(--color-primary)]"
                      @input="
                        settingsStore.settings.ui.font_size = Math.round(
                          Number(($event.target as HTMLInputElement).value),
                        )
                      "
                    />
                    <!-- 档位标签：点击跳到对应档位 -->
                    <div class="flex justify-between mt-1.5">
                      <button
                        v-for="lvl in fontSizeLevels"
                        :key="lvl.value"
                        class="text-[calc(10px*var(--ui-scale))] transition-colors"
                        :class="
                          fontSizeLevelValue === lvl.value
                            ? 'text-[var(--text-primary)] font-medium'
                            : 'text-[var(--text-tertiary)] hover:text-[var(--text-secondary)]'
                        "
                        @click="settingsStore.settings.ui.font_size = lvl.value"
                      >
                        {{ t(lvl.key) }}
                      </button>
                    </div>
                  </div>
                  <span
                    class="text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)] w-12 text-right flex-shrink-0"
                    >{{ fontSizeLevelLabel }}</span
                  >
                </div>
              </div>
            </div>

            <!-- 动画效果：方角开关（关闭后全局禁用页面过渡/动画） -->
            <div class="px-5 py-3.5 flex items-center justify-between gap-4">
              <div>
                <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
                  t('settings.appearance.animations')
                }}</span>
                <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
                  {{ t('settings.appearance.animationsDesc') }}
                </p>
              </div>
              <button
                class="relative w-10 h-5 rounded-[4px] border transition-colors flex-shrink-0"
                :class="
                  animationsEnabled
                    ? 'bg-[var(--color-primary)] border-[var(--color-primary)]'
                    : 'bg-[var(--bg-page)] border-[var(--border-strong)]'
                "
                role="switch"
                :aria-checked="animationsEnabled"
                @click="animationsEnabled = !animationsEnabled"
              >
                <span
                  class="absolute top-[3px] w-3 h-3 rounded-[2px] transition-all"
                  :class="
                    animationsEnabled
                      ? 'left-[22px] bg-[var(--color-primary-contrast)]'
                      : 'left-[3px] bg-[var(--border-strong)]'
                  "
                />
              </button>
            </div>
          </div>
        </section>

        <!-- ==================== PAIRING ==================== -->
        <section>
          <h3 class="wb-section-title">{{ t('settings.pairing.title') }}</h3>
          <div
            class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)]"
          >
            <!-- 默认端口：服务器启动时使用的端口 -->
            <div class="px-5 py-3.5 flex items-center justify-between gap-4">
              <div>
                <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
                  t('settings.pairing.defaultPort')
                }}</span>
                <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
                  {{ t('settings.pairing.defaultPortDesc') }}
                </p>
              </div>
              <input
                type="number"
                :value="settingsStore.settings.network.port"
                class="h-8 w-28 px-2.5 rounded-[6px] wb-mono text-right bg-[var(--bg-page)] border border-[var(--border-strong)] text-[var(--text-primary)] outline-none focus:border-[var(--color-primary)]"
                @input="
                  settingsStore.settings.network.port = Number(
                    ($event.target as HTMLInputElement).value,
                  )
                "
              />
            </div>

            <!-- 二维码有效期：配对时展示的二维码有效时间 -->
            <div class="px-5 py-3.5 flex items-center justify-between gap-4">
              <div>
                <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
                  t('settings.pairing.qrValidity')
                }}</span>
                <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
                  {{ t('settings.pairing.qrValidityDesc') }}
                </p>
              </div>
              <input
                type="number"
                :value="qrTokenTtl"
                class="h-8 w-28 px-2.5 rounded-[6px] wb-mono text-right bg-[var(--bg-page)] border border-[var(--border-strong)] text-[var(--text-primary)] outline-none focus:border-[var(--color-primary)]"
                @input="qrTokenTtl = Number(($event.target as HTMLInputElement).value)"
                @blur="saveQrTokenTtl"
              />
            </div>

            <!-- 配对码有效期：手动输入的配对码有效时间 -->
            <div class="px-5 py-3.5 flex items-center justify-between gap-4">
              <div>
                <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
                  t('settings.pairing.pairingCodeTtl')
                }}</span>
                <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
                  {{ t('settings.pairing.pairingCodeTtlDesc') }}
                </p>
              </div>
              <input
                type="number"
                :value="pairingCodeTtl"
                class="h-8 w-28 px-2.5 rounded-[6px] wb-mono text-right bg-[var(--bg-page)] border border-[var(--border-strong)] text-[var(--text-primary)] outline-none focus:border-[var(--color-primary)]"
                @input="pairingCodeTtl = Number(($event.target as HTMLInputElement).value)"
                @blur="savePairingCodeTtl"
              />
            </div>
          </div>
        </section>

        <!-- ==================== LINK CRYPTO ==================== -->
        <section>
          <h3 class="wb-section-title">{{ t('settings.linkCrypto.title') }}</h3>
          <div
            class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)]"
          >
            <!-- 主开关：默认关（opt-in），关时全服务明文与现状一致 -->
            <div class="px-5 py-3.5 flex items-center justify-between gap-4">
              <div class="min-w-0">
                <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
                  t('settings.linkCrypto.master')
                }}</span>
                <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
                  {{ t('settings.linkCrypto.masterDesc') }}
                </p>
              </div>
              <button
                class="relative w-10 h-5 rounded-[4px] border transition-colors flex-shrink-0"
                :class="
                  linkCryptoConfig?.enabled
                    ? 'bg-[var(--color-primary)] border-[var(--color-primary)]'
                    : 'bg-[var(--bg-page)] border-[var(--border-strong)]'
                "
                role="switch"
                :aria-checked="linkCryptoConfig?.enabled ?? false"
                @click="updateLinkCrypto({ enabled: !linkCryptoConfig?.enabled })"
              >
                <span
                  class="absolute top-[3px] w-3 h-3 rounded-[2px] transition-all"
                  :class="
                    linkCryptoConfig?.enabled
                      ? 'left-[22px] bg-[var(--color-primary-contrast)]'
                      : 'left-[3px] bg-[var(--border-strong)]'
                  "
                />
              </button>
            </div>

            <!-- 通道子开关：主开关关时置灰不可点（粒度收窄是显式动作） -->
            <div
              v-for="channel in linkCryptoChannels"
              :key="channel.field"
              class="px-5 py-3.5 flex items-center justify-between gap-4"
              :class="{ 'opacity-50': !linkCryptoConfig?.enabled }"
            >
              <div class="min-w-0">
                <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
                  t(channel.labelKey)
                }}</span>
                <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
                  {{ t(channel.descKey) }}
                </p>
              </div>
              <button
                class="relative w-10 h-5 rounded-[4px] border transition-colors flex-shrink-0 disabled:cursor-not-allowed"
                :class="
                  linkCryptoConfig?.[channel.field]
                    ? 'bg-[var(--color-primary)] border-[var(--color-primary)]'
                    : 'bg-[var(--bg-page)] border-[var(--border-strong)]'
                "
                role="switch"
                :aria-checked="linkCryptoConfig?.[channel.field] ?? false"
                :disabled="!linkCryptoConfig?.enabled"
                @click="updateLinkCrypto({ [channel.field]: !linkCryptoConfig?.[channel.field] })"
              >
                <span
                  class="absolute top-[3px] w-3 h-3 rounded-[2px] transition-all"
                  :class="
                    linkCryptoConfig?.[channel.field]
                      ? 'left-[22px] bg-[var(--color-primary-contrast)]'
                      : 'left-[3px] bg-[var(--border-strong)]'
                  "
                />
              </button>
            </div>

            <!-- 明文回退：关掉后非环回未协商请求一律拒绝（强加密模式） -->
            <div
              class="px-5 py-3.5 flex items-center justify-between gap-4"
              :class="{ 'opacity-50': !linkCryptoConfig?.enabled }"
            >
              <div class="min-w-0">
                <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
                  t('settings.linkCrypto.plaintextFallback')
                }}</span>
                <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
                  {{ t('settings.linkCrypto.plaintextFallbackDesc') }}
                </p>
              </div>
              <button
                class="relative w-10 h-5 rounded-[4px] border transition-colors flex-shrink-0 disabled:cursor-not-allowed"
                :class="
                  linkCryptoConfig?.allow_plaintext_fallback
                    ? 'bg-[var(--color-primary)] border-[var(--color-primary)]'
                    : 'bg-[var(--bg-page)] border-[var(--border-strong)]'
                "
                role="switch"
                :aria-checked="linkCryptoConfig?.allow_plaintext_fallback ?? false"
                :disabled="!linkCryptoConfig?.enabled"
                @click="
                  updateLinkCrypto({
                    allow_plaintext_fallback: !linkCryptoConfig?.allow_plaintext_fallback,
                  })
                "
              >
                <span
                  class="absolute top-[3px] w-3 h-3 rounded-[2px] transition-all"
                  :class="
                    linkCryptoConfig?.allow_plaintext_fallback
                      ? 'left-[22px] bg-[var(--color-primary-contrast)]'
                      : 'left-[3px] bg-[var(--border-strong)]'
                  "
                />
              </button>
            </div>

            <!-- 本机指纹：供双端人工核对（移动端 pin 展示比对） -->
            <div class="px-5 py-3.5 flex items-center justify-between gap-4">
              <div class="min-w-0">
                <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
                  t('settings.linkCrypto.fingerprint')
                }}</span>
                <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
                  {{ t('settings.linkCrypto.fingerprintDesc') }}
                </p>
              </div>
              <span
                class="wb-mono text-[calc(12px*var(--ui-scale))] text-[var(--text-secondary)] flex-shrink-0 truncate"
                >{{ linkCryptoFingerprint ?? '—' }}</span
              >
            </div>
          </div>
        </section>

        <!-- ==================== SESSION ==================== -->
        <section>
          <h3 class="wb-section-title">{{ t('settings.session.title') }}</h3>
          <div
            class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)]"
          >
            <!-- 默认执行环境：分段控件 -->
            <div class="px-5 py-3.5 flex items-center justify-between gap-4">
              <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
                t('settings.session.defaultEnvironment')
              }}</span>
              <div
                class="flex border border-[var(--border-strong)] rounded-md overflow-hidden flex-shrink-0"
              >
                <button
                  v-for="opt in availableEnvironmentOptions"
                  :key="opt.value"
                  class="h-8 px-4 text-xs font-medium wb-mono transition-colors"
                  :class="
                    defaultEnvironment === opt.value
                      ? 'bg-[var(--color-primary)] text-[var(--color-primary-contrast)]'
                      : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'
                  "
                  @click="defaultEnvironment = opt.value"
                >
                  {{ opt.label }}
                </button>
              </div>
            </div>

            <!-- 默认启动命令 -->
            <div class="px-5 py-3.5 flex items-center justify-between gap-4">
              <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
                t('settings.session.defaultCommand')
              }}</span>
              <input
                type="text"
                :value="settingsStore.settings.session.default_command || ''"
                class="h-8 w-56 px-2.5 rounded-[6px] wb-mono bg-[var(--bg-page)] border border-[var(--border-strong)] text-[var(--text-primary)] outline-none focus:border-[var(--color-primary)]"
                @input="
                  settingsStore.settings.session.default_command = (
                    $event.target as HTMLInputElement
                  ).value
                "
              />
            </div>
          </div>
        </section>

        <!-- ==================== SYSTEM ==================== -->
        <section>
          <h3 class="wb-section-title">{{ t('settings.system.title') }}</h3>
          <div
            class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)]"
          >
            <!-- 防止休眠：方角开关 -->
            <div class="px-5 py-3.5 flex items-center justify-between gap-4">
              <div>
                <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{
                  t('settings.system.preventSleep')
                }}</span>
                <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
                  {{ t('settings.system.preventSleepDesc') }}
                </p>
              </div>
              <button
                class="relative w-10 h-5 rounded-[4px] border transition-colors flex-shrink-0"
                :class="
                  preventSleep
                    ? 'bg-[var(--color-primary)] border-[var(--color-primary)]'
                    : 'bg-[var(--bg-page)] border-[var(--border-strong)]'
                "
                role="switch"
                :aria-checked="preventSleep"
                @click="preventSleep = !preventSleep"
              >
                <span
                  class="absolute top-[3px] w-3 h-3 rounded-[2px] transition-all"
                  :class="
                    preventSleep
                      ? 'left-[22px] bg-[var(--color-primary-contrast)]'
                      : 'left-[3px] bg-[var(--border-strong)]'
                  "
                />
              </button>
            </div>
          </div>
        </section>

        <!-- ==================== LOGGING ==================== -->
        <section>
          <h3 class="wb-section-title">{{ t('settings.log.title') }}</h3>
          <div
            class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] divide-y divide-[var(--border)]"
          >
            <!-- 日志级别：分段控件，点击即时热调（不重启） -->
            <div class="px-5 py-3.5 flex items-center justify-between gap-4">
              <div>
                <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{ t('settings.log.level') }}</span>
                <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
                  {{ t('settings.log.levelDesc') }}
                </p>
              </div>
              <div
                class="flex border border-[var(--border-strong)] rounded-md overflow-hidden flex-shrink-0"
              >
                <button
                  v-for="opt in logLevelOptions"
                  :key="opt.value"
                  class="h-8 px-3 text-xs font-medium transition-colors"
                  :class="
                    logFileLevel === opt.value
                      ? 'bg-[var(--color-primary)] text-[var(--color-primary-contrast)]'
                      : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'
                  "
                  @click="onLogLevelClick(opt.value)"
                >
                  {{ t(opt.label) }}
                </button>
              </div>
            </div>

            <!-- 日志格式：text/json，保存后重启生效 -->
            <div class="px-5 py-3.5 flex items-center justify-between gap-4">
              <div>
                <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{ t('settings.log.format') }}</span>
                <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
                  {{ t('settings.log.formatDesc') }}
                </p>
              </div>
              <div
                class="flex border border-[var(--border-strong)] rounded-md overflow-hidden flex-shrink-0"
              >
                <button
                  v-for="opt in logFormatOptions"
                  :key="opt.value"
                  class="h-8 px-3 text-xs font-medium transition-colors"
                  :class="
                    logFormat === opt.value
                      ? 'bg-[var(--color-primary)] text-[var(--color-primary-contrast)]'
                      : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'
                  "
                  @click="logFormat = opt.value"
                >
                  {{ t(opt.label) }}
                </button>
              </div>
            </div>

            <!-- 保留文件数：按天轮转保留数量，保存后重启生效 -->
            <div class="px-5 py-3.5 flex items-center justify-between gap-4">
              <div>
                <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{ t('settings.log.maxFiles') }}</span>
                <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
                  {{ t('settings.log.maxFilesDesc') }}
                </p>
              </div>
              <input
                type="number"
                :value="logMaxFiles"
                class="h-8 w-28 px-2.5 rounded-[6px] wb-mono text-right bg-[var(--bg-page)] border border-[var(--border-strong)] text-[var(--text-primary)] outline-none focus:border-[var(--color-primary)]"
                @input="logMaxFiles = Number(($event.target as HTMLInputElement).value)"
              />
            </div>

            <!-- 容量上限（MB）：保存后重启生效 -->
            <div class="px-5 py-3.5 flex items-center justify-between gap-4">
              <div>
                <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{ t('settings.log.capacityMb') }}</span>
                <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
                  {{ t('settings.log.capacityMbDesc') }}
                </p>
              </div>
              <input
                type="number"
                :value="logCapacityMb"
                class="h-8 w-28 px-2.5 rounded-[6px] wb-mono text-right bg-[var(--bg-page)] border border-[var(--border-strong)] text-[var(--text-primary)] outline-none focus:border-[var(--color-primary)]"
                @input="logCapacityMb = Number(($event.target as HTMLInputElement).value)"
              />
            </div>

            <!-- 打开日志目录 + 保存配置 -->
            <div class="px-5 py-3.5 flex items-center justify-between gap-4">
              <div>
                <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">{{ t('settings.log.persist') }}</span>
                <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
                  {{ t('settings.log.persistDesc') }}
                </p>
              </div>
              <div class="flex items-center gap-2 flex-shrink-0">
                <button class="wb-btn-ghost" @click="onOpenLogDir">
                  {{ t('settings.log.openDir') }}
                </button>
                <button class="wb-btn-primary" :disabled="logSaving" @click="onSaveLogConfig">
                  {{ t('settings.log.save') }}
                </button>
              </div>
            </div>
          </div>
        </section>

        <!-- ==================== ABOUT ==================== -->
        <section>
          <h3 class="wb-section-title">{{ t('settings.about.title') }}</h3>
          <div class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px] px-5 py-4">
            <div class="flex items-center justify-between gap-4">
              <div class="flex items-center gap-2">
                <span
                  class="text-[calc(13px*var(--ui-scale))] font-semibold text-[var(--text-primary)]"
                  >BedCode</span
                >
                <span class="wb-mono text-[var(--text-secondary)]">v{{ appVersion || '—' }}</span>
              </div>
              <div class="flex items-center gap-3">
                <!-- GitHub 仓库：系统浏览器打开 -->
                <button class="wb-btn-ghost" @click="openGitHub">
                  <svg class="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
                    <path
                      d="M12 .5C5.65.5.5 5.65.5 12c0 5.08 3.29 9.39 7.86 10.91.58.11.79-.25.79-.56 0-.27-.01-1.17-.02-2.12-3.2.7-3.87-1.36-3.87-1.36-.52-1.33-1.28-1.68-1.28-1.68-1.04-.71.08-.7.08-.7 1.15.08 1.76 1.18 1.76 1.18 1.02 1.75 2.68 1.25 3.34.95.1-.74.4-1.25.73-1.54-2.55-.29-5.23-1.28-5.23-5.68 0-1.26.45-2.28 1.18-3.09-.12-.29-.51-1.46.11-3.05 0 0 .96-.31 3.15 1.18a10.96 10.96 0 015.74 0c2.19-1.49 3.15-1.18 3.15-1.18.62 1.59.23 2.76.11 3.05.73.81 1.18 1.83 1.18 3.09 0 4.41-2.69 5.38-5.25 5.67.41.35.77 1.05.77 2.12 0 1.53-.01 2.76-.01 3.14 0 .31.21.67.8.56A11.51 11.51 0 0023.5 12C23.5 5.65 18.35.5 12 .5z"
                    />
                  </svg>
                  {{ t('settings.about.githubRepo') }}
                </button>
                <!-- 下载进度 -->
                <template v-if="updateStatus === 'downloading'">
                  <div class="w-32 h-1.5 bg-[var(--border)] overflow-hidden">
                    <div
                      class="h-full bg-[var(--color-primary)] transition-all duration-300"
                      :style="{ width: downloadPercent + '%' }"
                    />
                  </div>
                  <span class="wb-mono text-[var(--text-secondary)]">{{ downloadPercent }}%</span>
                </template>
                <button
                  v-else-if="updateStatus === 'available'"
                  class="wb-btn-primary"
                  @click="handleInstallUpdate"
                >
                  {{ t('settings.about.downloadUpdate') }}
                </button>
                <span
                  v-else-if="
                    updateStatus !== 'idle' &&
                    updateStatus !== 'latest' &&
                    updateStatus !== 'failed'
                  "
                  class="text-xs text-[var(--text-secondary)]"
                >
                  {{ getUpdateStatusText() }}
                </span>
              </div>
            </div>
            <p v-if="updateStatus === 'failed'" class="mt-2 text-xs text-red-500">
              {{ errorMessage }}
            </p>
          </div>
        </section>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 设置视图 — 桌面端设置页面
 * Warm Workbench 风格：分段控件 + 方角开关 + section 分组；支持多主题色板预留
 */
import { onBeforeUnmount, onMounted, ref, watch, computed } from 'vue'
import { logger } from '@/utils/frontendLogger'
import { useI18n } from 'vue-i18n'
import { useSettingsStore } from '@/stores/settings'
import { useI18nStore } from '@/stores/i18n'
import { useQrCodeApi } from '@/composables/useTauri'
import PluginPageToolbar from '@/plugin/components/PluginPageToolbar.vue'
import i18n from '@/locales'
import {
  getAppVersion,
  getPairingCodeTtl,
  setPairingCodeTtl,
} from '@/composables/useDesktopCommands'
import {
  useLinkCryptoConfig,
  type LinkCryptoConfig as LinkCryptoCfg,
} from '@/composables/useLinkCrypto'
import { open } from '@tauri-apps/plugin-shell'
import { useUpdateChecker } from '@/composables/useUpdateChecker'
import { MIN_FONT_SIZE, MAX_FONT_SIZE, NORMAL_FONT_SIZE } from '@/composables/useFontSize'
import { useToast } from '@/composables/useToast'
import { useAvailableEnvironments } from '@/composables/useAvailableEnvironments'
import { useLogSettings } from '@/composables/useLogSettings'
import { invoke } from '@/utils/invoke'

const { t } = useI18n()
const settingsStore = useSettingsStore()
const i18nStore = useI18nStore()
const qrApi = useQrCodeApi()
const toast = useToast()
const {
  status: updateStatus,
  downloadProgress,
  errorMessage,
  checkForUpdate,
  downloadAndInstall,
  getUpdateStatusText,
} = useUpdateChecker()

const appVersion = ref('')
const qrTokenTtl = ref(300)
const pairingCodeTtl = ref(60)

// ==================== 日志设置（desktop-logging-overhaul 04） ====================
const { setLogLevel, openLogDir, saveLogSettings } = useLogSettings()

const logLevelOptions = [
  { value: 'debug', label: 'settings.log.levelDebug' },
  { value: 'info', label: 'settings.log.levelInfo' },
  { value: 'warn', label: 'settings.log.levelWarn' },
  { value: 'error', label: 'settings.log.levelError' },
]
const logFormatOptions = [
  { value: 'text', label: 'settings.log.formatText' },
  { value: 'json', label: 'settings.log.formatJson' },
]
const logFileLevel = ref('info')
const logFormat = ref('text')
const logMaxFiles = ref(7)
const logCapacityMb = ref(512)
const logSaving = ref(false)

/** 从现有 AppConfig 的 log 段初始化（与 store 分离：store 的 Settings 类型不含 log 层） */
async function loadLogSettings() {
  try {
    const cfg = await invoke<{
      log: {
        file_level: string
        format: string
        max_files: number
        capacity_bytes: number
      }
    }>('get_app_settings')
    logFileLevel.value = cfg.log?.file_level || 'info'
    logFormat.value = cfg.log?.format || 'text'
    logMaxFiles.value = cfg.log?.max_files ?? 7
    logCapacityMb.value = Math.round((cfg.log?.capacity_bytes ?? 512 * 1024 * 1024) / (1024 * 1024))
  } catch (e) {
    logger.error('[Settings] Failed to load log settings:', e)
  }
}

/** 日志级别即时热调（不重启；失败回显原级别并提示） */
async function onLogLevelClick(level: string) {
  const previous = logFileLevel.value
  logFileLevel.value = level
  try {
    await setLogLevel(level)
    toast.success(i18n.global.t('settings.log.levelApplied'))
  } catch (e) {
    logger.error('[Settings] set_log_level failed:', e)
    logFileLevel.value = previous
    toast.error(i18n.global.t('settings.log.saveFailed'))
  }
}

async function onOpenLogDir() {
  try {
    await openLogDir()
  } catch (e) {
    logger.error('[Settings] open_log_dir failed:', e)
    toast.error(i18n.global.t('settings.log.saveFailed'))
  }
}

/** 持久化日志配置（format/rotation/max_files/capacity 重启生效） */
async function onSaveLogConfig() {
  logSaving.value = true
  try {
    await saveLogSettings({
      fileLevel: logFileLevel.value,
      rotation: 'daily',
      maxFiles: logMaxFiles.value,
      format: logFormat.value,
      capacityBytes: logCapacityMb.value * 1024 * 1024,
      consoleInRelease: false,
    })
    toast.success(i18n.global.t('settings.log.saved'))
  } catch (e) {
    logger.error('[Settings] save_log_settings failed:', e)
    toast.error(i18n.global.t('settings.log.saveFailed'))
  } finally {
    logSaving.value = false
  }
}

// ==================== 字体大小档位 ====================
// 档位间可无级滑动，点击下方标签跳到对应档位；值以 px 存储（12 = 正常）
const fontSizeLevels = [
  { value: MIN_FONT_SIZE, key: 'settings.appearance.fontSmall' },
  { value: NORMAL_FONT_SIZE, key: 'settings.appearance.fontNormal' },
  { value: 14, key: 'settings.appearance.fontLarge' },
  { value: MAX_FONT_SIZE, key: 'settings.appearance.fontXl' },
]

/** 当前值最接近的档位（用于高亮标签） */
const fontSizeLevelValue = computed(() => {
  const size = settingsStore.settings.ui.font_size || NORMAL_FONT_SIZE
  return fontSizeLevels.reduce((a, b) =>
    Math.abs(b.value - size) < Math.abs(a.value - size) ? b : a,
  ).value
})

/** 当前档位文案（小 / 正常 / 大 / 超大） */
const fontSizeLevelLabel = computed(() => {
  const level = fontSizeLevels.find((l) => l.value === fontSizeLevelValue.value)
  return level ? t(level.key) : ''
})

const environmentOptions = computed(() => [
  { value: 'windows', label: i18n.global.t('desktop.form.windowsNative') },
  { value: 'wsl2', label: 'WSL2' },
  { value: 'linux', label: i18n.global.t('desktop.form.linuxNative') },
])

// 仅展示当前宿主平台可用的执行环境；未识别平台（macOS 等）下落到 windows 与老数据兼容
const { availableValues } = useAvailableEnvironments()
const availableEnvironmentOptions = computed(() =>
  environmentOptions.value.filter((opt) => availableValues.value.includes(opt.value as any)),
)

const themeOptions = computed(() => [
  { value: 'light', label: i18n.global.t('settings.appearance.lightMode') },
  { value: 'dark', label: i18n.global.t('settings.appearance.darkMode') },
  { value: 'system', label: i18n.global.t('settings.appearance.followSystem') },
])

// 主题色板：调色台选项（色板值 + 展示色块，色块取色板自身色值以便预览切换后效果）
const paletteOptions = computed(() => [
  {
    value: 'warm',
    label: i18n.global.t('settings.appearance.paletteWarm'),
    swatches: { page: '#F5F4F0', card: '#FDFCFA', primary: '#1D1A14' },
  },
  {
    value: 'cool',
    label: i18n.global.t('settings.appearance.paletteCool'),
    swatches: { page: '#F3F5F7', card: '#FBFCFD', primary: '#2563EB' },
  },
  {
    value: 'forest',
    label: i18n.global.t('settings.appearance.paletteForest'),
    swatches: { page: '#F6F5EF', card: '#FDFCF7', primary: '#3E6B4F' },
  },
  {
    value: 'ocean',
    label: i18n.global.t('settings.appearance.paletteOcean'),
    swatches: { page: '#F2F7F9', card: '#FAFCFD', primary: '#0E7490' },
  },
  {
    value: 'sunset',
    label: i18n.global.t('settings.appearance.paletteSunset'),
    swatches: { page: '#FBF5EF', card: '#FEFAF5', primary: '#D9532A' },
  },
  {
    value: 'violet',
    label: i18n.global.t('settings.appearance.paletteViolet'),
    swatches: { page: '#F7F5FB', card: '#FCFBFE', primary: '#6D4FC6' },
  },
])

const languageOptions = [
  { value: 'zh-CN', label: '中文' },
  { value: 'en', label: 'English' },
]

// 直接读写 store，主题切换由 useTheme 全局监听即时生效；
// setter 同时立即持久化——防抖 watch 有 500ms 窗口，切页/退出时会丢失
const themeValue = computed({
  get: () => settingsStore.settings.ui.theme,
  set: (value: string) => {
    settingsStore.settings.ui.theme = value
    void settingsStore.saveSettings({
      ui: { ...settingsStore.settings.ui, theme: value },
    })
  },
})

// 色板切换由 useTheme 监听 data-palette 即时生效；同样立即持久化
// （否则切到设备页等触发 loadSettings 的页面时被后端旧值覆盖回退）
const paletteValue = computed({
  get: () => settingsStore.settings.ui.theme_palette || 'warm',
  set: (value: string) => {
    settingsStore.settings.ui.theme_palette = value
    void settingsStore.saveSettings({
      ui: { ...settingsStore.settings.ui, theme_palette: value },
    })
  },
})

const defaultEnvironment = computed({
  get: () => {
    const stored = settingsStore.settings.session.default_environment || 'windows'
    // 老用户存储的值（如 'wsl2'）在 Linux 平台上无效时，返回平台默认环境
    return availableValues.value.includes(stored as any) ? stored : (availableValues.value[0] ?? 'windows')
  },
  set: (value: string) => {
    settingsStore.settings.session.default_environment = value
  },
})

const preventSleep = computed({
  get: () => settingsStore.settings.network.prevent_sleep ?? true,
  set: (value: boolean) => {
    settingsStore.settings.network.prevent_sleep = value
  },
})

// 全局动画总开关：直接读写 store，由 deep watch 防抖持久化（无需即时保存）
const animationsEnabled = computed({
  get: () => settingsStore.settings.ui.animations_enabled ?? true,
  set: (value: boolean) => {
    settingsStore.settings.ui.animations_enabled = value
  },
})

const currentLanguage = computed({
  get: () => settingsStore.settings.ui.language || 'zh-CN',
  set: (value: string) => i18nStore.setLanguage(value),
})

// 语言切换过渡：先淡出当前内容，再在不可见时换语言，最后淡入新内容，
// 避免新旧文案重叠造成的闪烁；总时长由「动画效果」开关与 0.4s 时长控制。
const FADE_MS = 400
const langFading = ref(false)
let langFadeTimer: ReturnType<typeof setTimeout> | null = null

function switchLanguage(value: string) {
  if (currentLanguage.value === value) return
  if (langFadeTimer) clearTimeout(langFadeTimer)
  if (!animationsEnabled.value) {
    void i18nStore.setLanguage(value)
    return
  }
  langFading.value = true
  langFadeTimer = setTimeout(() => {
    void i18nStore.setLanguage(value)
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        langFading.value = false
      })
    })
  }, FADE_MS)
}

async function loadQrTokenTtl() {
  qrTokenTtl.value = await qrApi.getQrTokenTtl()
}

async function saveQrTokenTtl() {
  const val = Math.max(60, Math.min(3600, qrTokenTtl.value))
  qrTokenTtl.value = val
  await qrApi.setQrTokenTtl(val)
}

async function loadPairingCodeTtl() {
  pairingCodeTtl.value = await getPairingCodeTtl()
}

async function savePairingCodeTtl() {
  const val = Math.max(60, Math.min(3600, pairingCodeTtl.value))
  pairingCodeTtl.value = val
  await setPairingCodeTtl(val)
}

// 防抖保存逻辑：设置变更 500ms 后统一持久化；组件卸载时立即 flush，
// 避免 500ms 窗口内切页导致变更丢失（theme_palette/theme 的 setter 已即时保存，
// 此处兜底字体/环境等其余字段）。
// 保存回写（settings.value 被 store 重新赋值）会触发本 watch——经
// store.isPersisted 比对内容后跳过，不会形成保存循环。
let saveTimeout: ReturnType<typeof setTimeout> | null = null

watch(
  () => settingsStore.settings,
  () => {
    if (settingsStore.isPersisted(settingsStore.settings)) return
    if (saveTimeout) clearTimeout(saveTimeout)
    saveTimeout = setTimeout(() => {
      void settingsStore.saveSettings(settingsStore.settings)
    }, 500)
  },
  { deep: true },
)

onBeforeUnmount(() => {
  // 立即 flush 未保存的变更（卸载后 watch 不再触发）
  if (saveTimeout) {
    clearTimeout(saveTimeout)
    saveTimeout = null
    if (!settingsStore.isPersisted(settingsStore.settings)) {
      void settingsStore.saveSettings(settingsStore.settings)
    }
  }
})

// ==================== 链路加密（issue 08） ====================

const {
  config: linkCryptoConfig,
  fingerprint: linkCryptoFingerprint,
  loadLinkCryptoConfig,
  saveLinkCryptoConfig,
} = useLinkCryptoConfig()

/** 通道子开关元数据：字段名与 Rust serde snake_case 一致（deny_unknown_fields） */
const linkCryptoChannels: Array<{
  field: keyof Omit<LinkCryptoCfg, 'enabled' | 'allow_plaintext_fallback'>
  labelKey: string
  descKey: string
}> = [
  {
    field: 'encrypt_http',
    labelKey: 'settings.linkCrypto.encryptHttp',
    descKey: 'settings.linkCrypto.encryptHttpDesc',
  },
  {
    field: 'encrypt_ws_terminal',
    labelKey: 'settings.linkCrypto.encryptWsTerminal',
    descKey: 'settings.linkCrypto.encryptWsTerminalDesc',
  },
  {
    field: 'encrypt_ws_event',
    labelKey: 'settings.linkCrypto.encryptWsEvent',
    descKey: 'settings.linkCrypto.encryptWsEventDesc',
  },
]

/**
 * 乐观更新 + 失败回滚：单次切换整体保存，落库失败恢复原值并提示，
 * 不产生「UI 已开/后端未生效」的半启用状态
 */
async function updateLinkCrypto(patch: Partial<LinkCryptoCfg>) {
  if (!linkCryptoConfig.value) return
  const prev = linkCryptoConfig.value
  const next = { ...prev, ...patch }
  linkCryptoConfig.value = next
  try {
    await saveLinkCryptoConfig(next)
  } catch {
    linkCryptoConfig.value = prev
    toast.error(t('settings.linkCrypto.saveFailed'))
  }
}

onMounted(async () => {
  await settingsStore.loadSettings()
  await loadQrTokenTtl()
  await loadPairingCodeTtl()
  await loadLogSettings()
  loadLinkCryptoConfig().catch(() => {
    /* 配置域加载失败不阻断设置页其余部分；指纹/开关展示占位符 */
  })
  try {
    appVersion.value = await getAppVersion()
  } catch {
    appVersion.value = '—'
  }
})

async function handleCheckUpdate() {
  const update = await checkForUpdate()
  if (!update && updateStatus.value === 'latest') {
    toast.info(i18n.global.t('settings.about.alreadyLatest'))
  } else if (!update && updateStatus.value === 'failed') {
    toast.error(i18n.global.t('settings.about.checkFailed'))
  }
}

async function handleInstallUpdate() {
  await downloadAndInstall()
}

/** 在系统浏览器中打开 GitHub 仓库 */
async function openGitHub() {
  try {
    await open('https://github.com/7ZAI/BedCode')
  } catch (e) {
    logger.error('Failed to open GitHub repo:', e)
  }
}

const downloadPercent = computed(() => {
  if (downloadProgress.value.contentLength === 0) return 0
  return Math.round(
    (downloadProgress.value.downloaded / downloadProgress.value.contentLength) * 100,
  )
})
</script>

<style scoped>
/* 语言切换：淡出 → 换文案 → 淡入（单元素不重挂载，无重叠闪烁） */
.lang-fade-content {
  transition-property: opacity;
  transition-timing-function: ease;
}
.lang-fade-content.lang-fading {
  opacity: 0;
}
</style>
