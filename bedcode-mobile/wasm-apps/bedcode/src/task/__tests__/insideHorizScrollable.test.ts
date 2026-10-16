/**
 * 任务页横滑区「横向滚动容器」判定 行为契约测试
 *
 * 背景（实测缺陷）：纵向滚动容器的 overflow-x 会被 CSS 规范计算为 auto，
 * 任务列表里无空格长串（日志片段 / URL / 错误码）横向溢出 6px（chromium 实测）
 * 即被旧实现判为「可横向滚动」→ 任务页 zone 内横滑整轮 skip → 左右滑动失效。
 *
 * 判据（新）：纵向滚动容器（scrollHeight > clientHeight）排除；其余容器仅当
 * 横向溢出 ≥24px（实体横排内容）才算横向滚动区；2px 容差抗 DPR 亚像素舍入。
 *
 * | 契约 | 场景 | 预期 |
 * |---|---|---|
 * | C-H1 | 纵向滚动容器 + 6px 横溢出（任务列表在滚） | 不判横向滚动 |
 * | C-H2 | 短列表（不纵滚） + 6px 横溢出 | 不判横向滚动 |
 * | C-H3 | 短列表 + 2px 亚像素溢出 | 不判横向滚动 |
 * | C-H4 | 真横向滚动容器（溢出 ≥24px，内容横排） | 判横向滚动 |
 * | C-H5 | 溢出恰好 24px（阈值边界） | 判横向滚动 |
 * | C-H6 | target 无父级 / 空 target | 不抛且返回 false |
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { insideHorizontallyScrollable } from '../swipe-zone'

/** 构造带可编程布局度量的元素：流式写入页面，产出一棵 target → root 的祖先链 */
function buildChain(): { target: HTMLElement; root: HTMLElement } {
  const chain: HTMLElement[] = []
  const root = document.createElement('div')
  root.className = 'att-tab-panels'
  let parent: HTMLElement = root
  for (let i = 0; i < 3; i++) {
    const el = document.createElement('div')
    el.className = `level-${i}`
    parent.appendChild(el)
    chain.push(el)
    parent = el
  }
  document.body.appendChild(root)
  return { target: chain[chain.length - 1], root }
}

function cleanup(root: HTMLElement): void {
  root.remove()
}

/**
 * 覆写元素布局度量（happy-dom 不参与真实排版，全部数值由用例注入）
 * @param el 目标元素
 * @param sw scrollWidth / cw clientWidth / sh scrollHeight / ch clientHeight
 * @param styles computed style 覆写（仅列需要非默认的键）
 */
function mockMetrics(
  el: HTMLElement,
  m: { sw: number; cw: number; sh: number; ch: number },
  styles: { overflowX?: string; overflowY?: string } = {},
): void {
  Object.defineProperty(el, 'scrollWidth', { configurable: true, value: m.sw })
  Object.defineProperty(el, 'clientWidth', { configurable: true, value: m.cw })
  Object.defineProperty(el, 'scrollHeight', { configurable: true, value: m.sh })
  Object.defineProperty(el, 'clientHeight', { configurable: true, value: m.ch })
  if (styles.overflowX) el.style.overflowX = styles.overflowX
  if (styles.overflowY) el.style.overflowY = styles.overflowY
}

describe('insideHorizontallyScrollable 判定契约', () => {
  let root: HTMLElement
  let target: HTMLElement
  /** 检查链上的元素（函数从 target.parentElement 起向上查，target 自身不参与） */
  let checkEl: HTMLElement

  beforeEach(() => {
    const chain = buildChain()
    root = chain.root
    target = chain.target
    checkEl = target.parentElement as HTMLElement
  })
  afterEach(() => cleanup(root))

  it('C-H1 纵向滚动容器 + 6px 横向溢出（任务列表在滚）→ 不判横向滚动', () => {
    // 列表滚动容器：内容高出可视区（scrollHeight > clientHeight），且长串使横溢出 6px
    mockMetrics(checkEl, { sw: 340, cw: 334, sh: 1200, ch: 600 }, { overflowY: 'auto' })
    expect(insideHorizontallyScrollable(target, root)).toBe(false)
  })

  it('C-H2 短列表（不纵滚）+ 6px 横向溢出 → 不判横向滚动', () => {
    // 内容不足一屏：scrollHeight == clientHeight；overflow-y: auto → overflow-x 计算为 auto
    mockMetrics(checkEl, { sw: 340, cw: 334, sh: 400, ch: 400 }, { overflowY: 'auto' })
    expect(insideHorizontallyScrollable(target, root)).toBe(false)
  })

  it('C-H3 短列表 + 2px 亚像素溢出 → 不判横向滚动', () => {
    mockMetrics(checkEl, { sw: 336, cw: 334, sh: 400, ch: 400 }, { overflowY: 'auto' })
    expect(insideHorizontallyScrollable(target, root)).toBe(false)
  })

  it('C-H4 真横向滚动容器（溢出 60px，内容横排）→ 判横向滚动', () => {
    // 横向卡片行：内容不纵滚、横向实体溢出
    mockMetrics(checkEl, { sw: 394, cw: 334, sh: 120, ch: 120 }, { overflowX: 'auto' })
    expect(insideHorizontallyScrollable(target, root)).toBe(true)
  })

  it('C-H5 溢出 25px（临界：刚过 24px 容差）→ 判横向滚动', () => {
    mockMetrics(checkEl, { sw: 359, cw: 334, sh: 120, ch: 120 }, { overflowX: 'auto' })
    expect(insideHorizontallyScrollable(target, root)).toBe(true)
  })

  it('C-H6 空 target / 无父级 → 不抛且返回 false', () => {
    expect(insideHorizontallyScrollable(null, root)).toBe(false)
    mockMetrics(target, { sw: 394, cw: 334, sh: 120, ch: 120 }, { overflowX: 'auto' })
    // root 本身就是 target 的父级场景：链在 root 处停止
    const direct = {} as HTMLElement
    Object.defineProperty(direct, 'parentElement', { configurable: true, value: null })
    expect(insideHorizontallyScrollable(direct, root)).toBe(false)
  })

  it('C-H7 纵向滚动容器排除后继续向上查：上层真横向滚动容器仍判中', () => {
    // 触摸起点在纵滚列表内部，但其祖先（横向卡片轨道）是真正横滚区
    mockMetrics(target, { sw: 340, cw: 334, sh: 1200, ch: 600 }, { overflowY: 'auto' })
    const ancestor = target.parentElement!.parentElement as HTMLElement
    mockMetrics(ancestor, { sw: 600, cw: 334, sh: 100, ch: 100 }, { overflowX: 'auto' })
    expect(insideHorizontallyScrollable(target, root)).toBe(true)
  })
})