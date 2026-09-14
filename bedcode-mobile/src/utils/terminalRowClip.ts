/**
 * 终端行尾背景盒裁切（DOM 渲染器 CJK advance 累计漂移的止血补丁）
 *
 * 机制（真机 CDP 实测，opencode TUI 必现）：
 * xterm 6 DOM 渲染器按流式排版画行——行内字符依浏览器真实 advance 依次排布，
 * 以统一 letter-spacing（约 -1.8px）压回网格宽。CJK 回退字体的 advance 与
 * 2 格宽存在偏差，一行内逐字符累积漂移 → 行尾带背景色的填充 span（TUI 应用
 * 用「背景色 + 空格」补面板宽度）被推出行右边界 7~22px。行级 overflow 已被
 * terminal.css 放开（保护满行末字墨迹不被裁半），背景色盒随之溢出，画进行尾
 * 余量区（主题底色）形成「色块入侵」，且随 TUI 重排行内容变化而漂移。
 *
 * 本补丁：MutationObserver 监听行内容重建 + ResizeObserver 监听网格宽度变化，
 * 把「溢出行界且内容为纯空白」的 span 用 clip-path 在行界处裁掉——空白无墨迹，
 * 裁切零损失；含文字的 span 保持溢出（满行末字墨迹保护语义不变）。
 *
 * 性能（真机实测教训）：单次全量扫描 ≈10ms（154 span，含强制布局），120Hz
 * 帧预算仅 8.3ms——滚动/大回放期间每帧全量扫描会挤爆帧预算（BLASTBufferQueue
 * 缓冲耗尽 → 主线程饥饿 → 滚动/呈现冻结）。因此扫描必须节流：
 * - 脏行增量：只扫本批 mutation 涉及的行（<1ms），不做全量
 * - 64ms 时间节流：连续输出期间最多 ~15 次/秒
 * - 静默补扫：最后一次变更 250ms 后全量扫一遍，兜底收敛
 * canvas 渲染器逐格绝对定位、无累计漂移，可用后本补丁自然多余，可整体移除。
 */

/** 亚像素舍入噪声容差：溢出超过该值才裁切，避免 clip-path 逐帧抖动 */
const OVERFLOW_EPSILON_PX = 1

/** 扫描最小间隔（ms）：全量扫描 ≈10ms/次，120Hz 帧预算 8.3ms，必须节流 */
const SCAN_MIN_INTERVAL_MS = 64

/** 静默补扫延迟（ms）：最后一次行变更后多久做一次全量兜底扫描 */
const QUIESCE_SCAN_DELAY_MS = 250

/** 裁切单个行容器内溢出行界的纯空白背景 span（见模块头注释）。幂等 */
function clipRow(row: HTMLElement, rowRight: number): number {
  let changed = 0
  for (const span of row.querySelectorAll('span')) {
    const rect = span.getBoundingClientRect()
    if (rect.width <= 0) continue
    const overflow = rect.right - rowRight
    if (overflow <= OVERFLOW_EPSILON_PX) {
      // 行内容重排后漂移可能消失：清除历史裁切，避免残留错误裁切框
      if (span.style.clipPath) {
        span.style.clipPath = ''
        changed++
      }
      continue
    }
    // 仅裁纯空白 span（背景填充盒，无墨迹）；含文字 span 保留溢出
    if (/\S/.test(span.textContent ?? '')) continue
    const next = `inset(0 ${overflow.toFixed(1)}px 0 0)`
    if (span.style.clipPath !== next) {
      span.style.clipPath = next
      changed++
    }
  }
  return changed
}

/**
 * 扫描并裁切行容器内所有行溢出行界的纯空白背景 span。
 * 供静默兜底扫描与单测直接调用；幂等——已按当前溢出量裁切 / 无需裁切的
 * span 不会重复写入样式。
 *
 * @param rowsEl xterm 的 .xterm-rows 元素（其直接子元素为行 div）
 * @returns 本次实际新写入/清除的裁切数（调试用）
 */
export function scanRowBackgroundOverflow(rowsEl: HTMLElement): number {
  let changed = 0
  for (const row of rowsEl.children) {
    if (!(row instanceof HTMLElement)) continue
    changed += clipRow(row, row.getBoundingClientRect().right)
  }
  return changed
}

/** 行尾背景裁切器句柄：dispose 解除全部监听 */
export interface RowBackgroundClipper {
  dispose(): void
}

/**
 * 挂接行尾背景裁切器：监听 .xterm-rows 的行重建（MutationObserver）与
 * 网格尺寸变化（ResizeObserver），须在 term.open() 之后调用。
 *
 * 调度策略（性能约束见模块头注释）：
 * - 变更只标记脏行，64ms 节流增量扫描（连续输出期间成本 ~15% 单核以内）
 * - 最后一次变更 250ms 后做一次全量兜底扫描（静默收敛）
 * - 观察器含 attributes/characterData（xterm 可能复用节点只改样式/文本）；
 *   扫描幂等（无变化不写样式），不会自激循环
 */
export function attachRowBackgroundClipper(container: HTMLElement): RowBackgroundClipper {
  const rowsEl = container.querySelector<HTMLElement>('.xterm-rows')
  if (!rowsEl) {
    return { dispose() {} }
  }

  let disposed = false
  let scanTimer: ReturnType<typeof setTimeout> | null = null
  let quiesceTimer: ReturnType<typeof setTimeout> | null = null
  /** 全量扫描挂起（网格尺寸变化/兜底触发，优先于脏行增量） */
  let fullPending = false
  /** 待扫描脏行（增量路径） */
  const dirtyRows = new Set<HTMLElement>()

  function runScan() {
    scanTimer = null
    if (disposed) return
    if (fullPending) {
      fullPending = false
      dirtyRows.clear()
      scanRowBackgroundOverflow(rowsEl)
    } else {
      for (const row of dirtyRows) {
        if (row.isConnected) {
          clipRow(row, row.getBoundingClientRect().right)
        }
      }
      dirtyRows.clear()
    }
  }

  function scheduleScan(full: boolean) {
    if (full) {
      fullPending = true
    }
    if (scanTimer !== null) return
    scanTimer = setTimeout(runScan, SCAN_MIN_INTERVAL_MS)
  }

  /** 从 mutation 记录提取涉及的行为脏行；定位不到行时退化为全量 */
  function markDirtyRows(records: MutationRecord[]): boolean {
    let all = false
    for (const rec of records) {
      const target = rec.target
      const row = target instanceof HTMLElement && target.parentElement === rowsEl
        ? target
        : (target.parentElement?.parentElement === rowsEl ? target.parentElement : null)
      if (row) {
        dirtyRows.add(row)
      } else {
        all = true
      }
    }
    return all
  }

  const mo = new MutationObserver((records) => {
    if (disposed) return
    const all = markDirtyRows(records)
    scheduleScan(all)
    // 静默补扫：变更停止 250ms 后全量扫一遍收敛
    if (quiesceTimer) clearTimeout(quiesceTimer)
    quiesceTimer = setTimeout(() => scheduleScan(true), QUIESCE_SCAN_DELAY_MS)
  })
  mo.observe(rowsEl, {
    childList: true,
    subtree: true,
    characterData: true,
    attributes: true,
    attributeFilter: ['style', 'class'],
  })
  const ro = new ResizeObserver(() => {
    if (!disposed) scheduleScan(true)
  })
  ro.observe(rowsEl)

  // 挂接时立即全量扫一遍：历史回放/重进会话时行内容可能已就位
  scanRowBackgroundOverflow(rowsEl)

  return {
    dispose() {
      disposed = true
      if (scanTimer) {
        clearTimeout(scanTimer)
        scanTimer = null
      }
      if (quiesceTimer) {
        clearTimeout(quiesceTimer)
        quiesceTimer = null
      }
      mo.disconnect()
      ro.disconnect()
    },
  }
}
