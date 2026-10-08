/**
 * 终端新手引导步骤定义（数据与组件分离，便于 i18n 完整性单测）
 *
 * titleKey/descKey/tryHintKey 为 mobile.terminal.* 下的 i18n key（不含前缀）。
 * 新增步骤必须同步补充 zh-CN 与 en 两份语言文件——
 * terminalOnboardingSteps.test.ts 会对全部 key 做存在性断言。
 */

export type OnboardingCheckKind = 'quickBarScroll' | 'sawGone' | 'panelDotIndex'

export interface TerminalOnboardingStep {
  /** 聚光灯目标元素选择器（terminal 页面内） */
  targetSelector: string
  titleKey: string
  descKey: string
  /** 试试看提示（交互步骤） */
  tryHintKey?: string
  /**
   * 完成检测类型：
   * - quickBarScroll：快捷条 scrollLeft 相对步骤进入时发生变化
   * - sawGone：appearSelector 元素出现后再消失（用户完成并退出该动作）
   * - panelDotIndex：快捷键面板轮播切到第 2 页
   * 缺省 = 无自动检测，手动「下一步」推进
   */
  checkKind?: OnboardingCheckKind
  /** sawGone 检测的出现选择器（缺省用 targetSelector） */
  appearSelector?: string
}

export const TERMINAL_ONBOARDING_STEPS: TerminalOnboardingStep[] = [
  {
    targetSelector: '.quick-bar',
    titleKey: 'onboardingQuickBarTitle',
    descKey: 'onboardingQuickBarDesc',
    tryHintKey: 'onboardingTryQuickBar',
    checkKind: 'quickBarScroll',
  },
  {
    targetSelector: '.input-box',
    titleKey: 'onboardingCompletionTitle',
    descKey: 'onboardingCompletionDesc',
    tryHintKey: 'onboardingTryCompletion',
    checkKind: 'sawGone',
    appearSelector: '.completion-panel',
  },
  {
    targetSelector: '.terminal-scroll-container',
    titleKey: 'onboardingSelectionTitle',
    descKey: 'onboardingSelectionDesc',
    tryHintKey: 'onboardingTrySelection',
    checkKind: 'sawGone',
    appearSelector: '.terminal-scroll-container.selection-mode',
  },
  {
    targetSelector: '.terminal-input-bar .toggle-btn',
    titleKey: 'onboardingPanelOpenTitle',
    descKey: 'onboardingPanelOpenDesc',
    tryHintKey: 'onboardingTryPanelOpen',
    checkKind: 'sawGone',
    appearSelector: '.shortcuts-panel',
  },
  {
    targetSelector: '.carousel-dots',
    titleKey: 'onboardingPanelSwipeTitle',
    descKey: 'onboardingPanelSwipeDesc',
    tryHintKey: 'onboardingTryPanelSwipe',
    checkKind: 'panelDotIndex',
  },
  {
    targetSelector: '.carousel-container',
    titleKey: 'onboardingCustomTitle',
    descKey: 'onboardingCustomDesc',
  },
  {
    targetSelector: '.overflow-btn',
    titleKey: 'onboardingOverflowTitle',
    descKey: 'onboardingOverflowDesc',
    tryHintKey: 'onboardingTryOverflow',
    checkKind: 'sawGone',
    appearSelector: '.overflow-menu',
  },
  {
    targetSelector: '.folder-btn',
    titleKey: 'onboardingSidebarTitle',
    descKey: 'onboardingSidebarDesc',
    tryHintKey: 'onboardingTrySidebar',
    checkKind: 'sawGone',
    appearSelector: '.sidebar-overlay:not(.sidebar-hidden)',
  },
  {
    targetSelector: '.execute-btn',
    titleKey: 'onboardingSendTitle',
    descKey: 'onboardingSendDesc',
  },
]
