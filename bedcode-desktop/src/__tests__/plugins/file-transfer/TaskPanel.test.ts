/**
 * TaskPanel 展示/交互契约（真机报障回归）
 *
 * 契约来源（用户报障）：
 * 1) 「桌面端任务卡片没有显示传输速度」→ 接收（下载）卡必须显示速率，
 *    且无速率采样（rateBps=0）时不得渲染 0/空速率（反例）；
 * 2) 「桌面端右上角关闭没有反应」→ 队列面板右上角关闭按钮必须 emit close；
 * 3) 「暂停后任务消失」的 UI 半边：paused 接收卡留在队列里且提供「继续」，
 *    点击带 sessionId 派发 resume（后端 store 侧配套把 paused 计入接收视图）。
 *
 * 纯展示组件测试：provide 最小 pluginContext（仅 i18n.t 被模板使用）。
 */

import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import TaskPanel from '../../../../wasm-apps/file-transfer/src/components/TaskPanel.vue'
import type { ReceivingTask } from '../../../../wasm-apps/file-transfer/src/types'

/** 上下文桩：t 直返 key（断言时用 key 定位元素，不依赖真实文案） */
function makeContext(): PluginContext {
  return {
    i18n: { t: (key: string) => key },
  } as unknown as PluginContext
}

function makeReceiving(overrides: Partial<ReceivingTask> = {}): ReceivingTask {
  return {
    sessionId: 'pull-1',
    batchId: 'pull-1',
    remotePath: 'movie.mp4',
    relPath: 'movie.mp4',
    size: 4 * 1024 * 1024,
    offset: 1024 * 1024,
    rateBps: 2048,
    state: 'running',
    reason: null,
    peerId: 'node-a',
    peerName: 'Pixel',
    createdAt: 1,
    updatedAt: 2,
    ...overrides,
  }
}

function mountPanel(receiving: ReceivingTask[]) {
  return mount(TaskPanel, {
    props: {
      tasks: [],
      totalSpeed: 0,
      receiving,
      history: [],
      peerNames: {},
      downloadDir: '/downloads',
    },
    global: { provide: { pluginContext: makeContext() } },
  })
}

/** 切到「正在接收」tab（tab 文案 = i18n key） */
async function switchToReceivingTab(wrapper: ReturnType<typeof mountPanel>): Promise<void> {
  const tab = wrapper
    .findAll('button.ft-tab')
    .find((b) => b.text() === 'transfer.queue.receiving')
  expect(tab, 'receiving tab must exist').toBeTruthy()
  await tab!.trigger('click')
}

describe('TaskPanel', () => {
  it('should show receive speed when rate sample is available', async () => {
    const wrapper = mountPanel([makeReceiving()])
    await switchToReceivingTab(wrapper)

    // formatBytes(2048) = '2.00 KB' → 速率行形如 '2.00 KB/s'
    expect(wrapper.text()).toContain('2.00 KB/s')
  })

  it('should hide receive speed when rate sample is zero', async () => {
    const wrapper = mountPanel([makeReceiving({ rateBps: 0 })])
    await switchToReceivingTab(wrapper)

    expect(wrapper.text()).not.toContain('/s')
  })

  it('should emit close when queue head close button clicked', async () => {
    const wrapper = mountPanel([])

    const closeBtn = wrapper.find('button[aria-label="transfer.queue.close"]')
    expect(closeBtn.exists()).toBe(true)
    await closeBtn.trigger('click')

    expect(wrapper.emitted('close')).toHaveLength(1)
  })

  it('should keep paused receive task in queue with resume wired to sessionId', async () => {
    const wrapper = mountPanel([makeReceiving({ state: 'paused', rateBps: 0 })])
    await switchToReceivingTab(wrapper)

    const resumeBtn = wrapper.find('button[title="transfer.task.resume"]')
    expect(resumeBtn.exists()).toBe(true)
    await resumeBtn.trigger('click')

    expect(wrapper.emitted('resume')).toEqual([['pull-1']])
  })
})
