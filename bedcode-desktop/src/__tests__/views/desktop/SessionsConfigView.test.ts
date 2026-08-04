import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import i18n from '@/locales'
import SessionsConfigView from '@/views/SessionsConfigView.vue'
import { useSessionStore } from '@/stores/session'

// Mock Tauri invoke（视图在 onMounted 中调用 get_startup_time）
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(100),
}))

// Mock useDesktopCommands（session store 的数据源）
vi.mock('@/composables/useDesktopCommands', () => ({
  listSessionConfigs: vi.fn(async () => []),
  listSessions: vi.fn(async () => []),
  createSessionNoStart: vi.fn(async () => 'session-1'),
  startExistingSession: vi.fn(async () => {}),
  killSession: vi.fn(async () => {}),
  deleteSession: vi.fn(async () => {}),
  restartSession: vi.fn(async () => 'session-1'),
  createSessionConfig: vi.fn(async () => ({})),
  deleteSessionConfig: vi.fn(async () => {}),
  updateSessionConfig: vi.fn(async () => {}),
}))

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  }),
}))

vi.mock('@/composables/useSessionWindows', () => ({
  useSessionWindows: () => ({
    closeTerminalWindow: vi.fn(),
  }),
}))

vi.mock('@/composables/useSessionStatusListener', () => ({
  useSessionStatusListener: () => ({
    startListening: vi.fn().mockResolvedValue(undefined),
    stopListening: vi.fn(),
  }),
}))

vi.mock('@/composables/useGlobalTerminal', () => ({
  initSessionCache: vi.fn(),
  destroySessionCache: vi.fn(),
  cleanupAllCaches: vi.fn(),
}))

// Mock 子组件
vi.mock('@/components/Button.vue', () => ({
  default: {
    template: '<button @click="$emit(\'click\')"><slot /><slot name="icon" /></button>',
    props: ['variant', 'size', 'loading', 'disabled'],
  },
}))

vi.mock('@/components/Modal.vue', () => ({
  default: {
    template: '<div v-if="modelValue" class="modal"><slot /><slot name="footer" /></div>',
    props: ['modelValue', 'title', 'size'],
  },
}))

vi.mock('@/components/SessionCard.vue', () => ({
  default: {
    template: [
      '<div class="session-card">',
      '<span class="config-name">{{ config.name }}</span>',
      '<button class="edit-btn" @click="$emit(\'edit\', config)">edit</button>',
      '<button class="delete-btn" @click="$emit(\'delete\', config.id)">delete</button>',
      '</div>',
    ].join(''),
    props: ['config', 'sessions'],
    emits: ['start', 'edit', 'delete', 'view-session', 'stop-session', 'restart-session', 'delete-session'],
  },
}))

vi.mock('@/components/SessionForm.vue', () => ({
  default: {
    template: '<form @submit.prevent="$emit(\'save\', { name: \'New\', environment: \'windows\', wslDistro: \'\', workingDir: \'\', command: \'claude\', autoStart: false })"><slot /></form>',
    props: ['config'],
    emits: ['save', 'cancel'],
  },
}))

vi.mock('@/components/Spinner.vue', () => ({
  default: {
    template: '<div class="spinner" />',
    props: ['size', 'color'],
  },
}))

function mountView() {
  const pinia = createPinia()
  setActivePinia(pinia)
  const wrapper = mount(SessionsConfigView, {
    global: {
      plugins: [pinia, i18n],
    },
  })
  return { wrapper, pinia }
}

describe('SessionsConfigView', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('should render header with title', async () => {
    const { wrapper } = mountView()
    await flushPromises()

    expect(wrapper.find('header').exists()).toBe(true)
    expect(wrapper.find('h2').text()).toContain('会话')
  })

  it('should show empty state when no configs', async () => {
    const { wrapper } = mountView()
    await flushPromises()

    expect(wrapper.text()).toContain('暂无会话配置')
  })

  it('should show create dialog when clicking new button', async () => {
    const { wrapper } = mountView()
    await flushPromises()

    const newSessionBtn = wrapper.findAll('button').find((b) => b.text().includes('新建配置'))
    expect(newSessionBtn).toBeDefined()

    await newSessionBtn!.trigger('click')
    await flushPromises()

    // Modal stub 渲染，说明对话框已打开
    expect(wrapper.find('.modal').exists()).toBe(true)
  })

  it('should render session cards when configs exist', async () => {
    const { wrapper, pinia } = mountView()
    await flushPromises()

    const sessionStore = useSessionStore(pinia)
    sessionStore.configs = [
      {
        id: '1',
        name: 'Test Session',
        environment: 'windows',
        wsl_distro: undefined,
        working_dir: 'C:\\test',
        command: 'claude',
        auto_start: false,
      },
    ]

    await flushPromises()
    wrapper.vm.$forceUpdate()
    await flushPromises()

    const cards = wrapper.findAll('.session-card')
    expect(cards).toHaveLength(1)
    expect(cards[0].text()).toContain('Test Session')
  })

  it('should call store loadConfigs on mount', async () => {
    const commands = await import('@/composables/useDesktopCommands')
    mountView()
    await flushPromises()

    expect(commands.listSessionConfigs).toHaveBeenCalled()
    expect(commands.listSessions).toHaveBeenCalled()
  })
})

describe('SessionsConfigView Integration', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('should handle config editing', async () => {
    const { wrapper, pinia } = mountView()
    await flushPromises()

    const sessionStore = useSessionStore(pinia)
    sessionStore.configs = [
      {
        id: '1',
        name: 'Test',
        environment: 'windows',
        wsl_distro: undefined,
        working_dir: 'C:\\test',
        command: 'claude',
        auto_start: false,
      },
    ]
    await flushPromises()
    wrapper.vm.$forceUpdate()
    await flushPromises()

    // 触发 SessionCard 的 edit 事件
    const editBtn = wrapper.find('.edit-btn')
    expect(editBtn.exists()).toBe(true)
    await editBtn.trigger('click')
    await flushPromises()

    // 编辑对话框应打开
    expect(wrapper.find('.modal').exists()).toBe(true)
  })

  it('should delete config through confirm dialog', async () => {
    const { wrapper, pinia } = mountView()
    await flushPromises()

    const sessionStore = useSessionStore(pinia)
    sessionStore.configs = [
      {
        id: '1',
        name: 'To Delete',
        environment: 'windows',
        wsl_distro: undefined,
        working_dir: '',
        command: 'claude',
        auto_start: false,
      },
    ]
    await flushPromises()
    wrapper.vm.$forceUpdate()
    await flushPromises()

    const deleteBtn = wrapper.find('.delete-btn')
    await deleteBtn.trigger('click')
    await flushPromises()

    // 删除确认对话框打开
    expect(wrapper.find('.modal').exists()).toBe(true)
  })
})
