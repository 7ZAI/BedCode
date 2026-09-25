/**
 * 行级 LCS diff（保存前 diff 预览用，纯函数）
 *
 * 编辑器场景：旧文（磁盘基线）与新文（草稿）均为小文件（SKILL.md 通常 < 数百行），
 * O(nm) 动态规划足够；不做词级 diff 与语法感知（v1 只求可读的增删行呈现）。
 */

/** 单行 diff 条目：ctx=上下文 add=新增 del=删除 */
export interface DiffLine {
  type: 'ctx' | 'add' | 'del'
  text: string
}

/**
 * 计算两段文本的行级 diff（保留旧文行序，新增行紧跟其插入位置）
 *
 * 无尾换行的最后一行仍参与比对；输出不含行号（预览场景不需要）。
 */
export function diffLines(oldText: string, newText: string): DiffLine[] {
  const a = oldText.length ? oldText.split('\n') : []
  const b = newText.length ? newText.split('\n') : []
  const n = a.length
  const m = b.length

  // LCS 全量长度表（回溯需要任意 (i,j) 查询；千行级文件 ≈ MB 内，够用）
  const table: Uint32Array[] = [new Uint32Array(m + 1)]
  for (let i = 1; i <= n; i++) {
    const prev = table[i - 1]
    const curr = new Uint32Array(m + 1)
    for (let j = 1; j <= m; j++) {
      curr[j] = a[i - 1] === b[j - 1] ? prev[j - 1] + 1 : Math.max(prev[j], curr[j - 1])
    }
    table.push(curr)
  }

  // 回溯生成 diff（从尾部向前，最后 reverse）
  const out: DiffLine[] = []
  let i = n
  let j = m
  while (i > 0 || j > 0) {
    if (i > 0 && j > 0 && a[i - 1] === b[j - 1]) {
      out.push({ type: 'ctx', text: a[i - 1] })
      i--
      j--
    } else if (j > 0 && (i === 0 || table[i][j] === table[i][j - 1])) {
      out.push({ type: 'add', text: b[j - 1] })
      j--
    } else {
      out.push({ type: 'del', text: a[i - 1] })
      i--
    }
  }
  out.reverse()
  return out
}

/** diff 统计（徽标展示用） */
export function diffStats(lines: DiffLine[]): { added: number; removed: number } {
  let added = 0
  let removed = 0
  for (const l of lines) {
    if (l.type === 'add') added++
    else if (l.type === 'del') removed++
  }
  return { added, removed }
}
