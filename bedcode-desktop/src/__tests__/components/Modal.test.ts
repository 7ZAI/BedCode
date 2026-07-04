import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import { defineComponent, h } from 'vue'
import Modal from '@/components/Modal.vue'

// Helper to mount Modal with Teleport handling
function mountModal(options: Parameters<typeof mount<typeof Modal>>[1] = {}) {
  return mount(Modal, {
    ...options,
    attachTo: document.body,
    global: {
      ...options.global,
      // Disable Teleport for testing
      stubs: {
        teleport: false,
        ...options.global?.stubs,
      },
    },
  })
}

describe('Modal Component', () => {
  it('should not render when modelValue is false', () => {
    const wrapper = mountModal({
      props: {
        modelValue: false,
        title: 'Test Modal'
      },
      slots: {
        default: 'Modal Content'
      }
    })

    // When modelValue is false, the modal content should not be visible
    expect(wrapper.find('.fixed.inset-0').exists()).toBe(false)
  })

  it('should render when modelValue is true', async () => {
    document.body.innerHTML = '' // Clear body

    const wrapper = mountModal({
      props: {
        modelValue: true,
        title: 'Test Modal'
      },
      slots: {
        default: 'Modal Content'
      }
    })

    // Wait for teleport to render
    await wrapper.vm.$nextTick()

    // Check if modal is rendered in body (teleport target)
    const modalElement = document.querySelector('.fixed.inset-0.z-50')
    expect(modalElement).toBeTruthy()
    expect(modalElement?.textContent).toContain('Test Modal')
    expect(modalElement?.textContent).toContain('Modal Content')

    wrapper.unmount()
  })

  it('should emit update:modelValue when close button clicked', async () => {
    document.body.innerHTML = ''

    const wrapper = mountModal({
      props: {
        modelValue: true,
        title: 'Test Modal'
      },
      slots: {
        default: 'Modal Content'
      }
    })

    await wrapper.vm.$nextTick()

    // Find close button in the body (where teleport renders)
    const closeButton = document.querySelector('button.absolute.top-4.right-4') as HTMLButtonElement
    expect(closeButton).toBeTruthy()

    closeButton?.click()
    await wrapper.vm.$nextTick()

    expect(wrapper.emitted('update:modelValue')).toBeTruthy()
    expect(wrapper.emitted('update:modelValue')![0]).toEqual([false])

    wrapper.unmount()
  })

  it('should emit update:modelValue when overlay clicked', async () => {
    document.body.innerHTML = ''

    const wrapper = mountModal({
      props: {
        modelValue: true,
        title: 'Test Modal'
      },
      slots: {
        default: 'Modal Content'
      }
    })

    await wrapper.vm.$nextTick()

    // Find the outer container (overlay) - click on it directly
    const overlay = document.querySelector('.fixed.inset-0.z-50') as HTMLDivElement
    expect(overlay).toBeTruthy()

    // Simulate click on the outer container (not on the modal content)
    overlay?.click()
    await wrapper.vm.$nextTick()

    expect(wrapper.emitted('update:modelValue')).toBeTruthy()

    wrapper.unmount()
  })

  it('should not close when closeOnBackdrop is false', async () => {
    document.body.innerHTML = ''

    const wrapper = mountModal({
      props: {
        modelValue: true,
        title: 'Test Modal',
        closeOnBackdrop: false
      },
      slots: {
        default: 'Modal Content'
      }
    })

    await wrapper.vm.$nextTick()

    // Click on the outer container
    const overlay = document.querySelector('.fixed.inset-0.z-50') as HTMLDivElement
    overlay?.click()
    await wrapper.vm.$nextTick()

    // Should not emit close event
    expect(wrapper.emitted('update:modelValue')).toBeFalsy()

    wrapper.unmount()
  })

  it('should render footer slot', async () => {
    document.body.innerHTML = ''

    const wrapper = mountModal({
      props: {
        modelValue: true,
        title: 'Test Modal'
      },
      slots: {
        default: 'Modal Content',
        footer: '<button>Confirm</button>'
      }
    })

    await wrapper.vm.$nextTick()

    // Check if footer is rendered in body
    const modalContent = document.querySelector('.relative.bg-dark-800')
    expect(modalContent?.innerHTML).toContain('<button>Confirm</button>')

    wrapper.unmount()
  })

  it('should apply size class', async () => {
    document.body.innerHTML = ''

    const wrapper = mountModal({
      props: {
        modelValue: true,
        title: 'Test Modal',
        size: 'lg'
      }
    })

    await wrapper.vm.$nextTick()

    // Check if size class is applied
    const modalContent = document.querySelector('.relative.bg-dark-800')
    expect(modalContent?.classList.contains('max-w-lg')).toBe(true)

    wrapper.unmount()
  })
})
