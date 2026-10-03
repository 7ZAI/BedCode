/**
 * 展示层格式化纯函数单测（票据 06）
 *
 * 关键约束：数字抽查与源数据一致——缩写不得产生「假零」（0.04k 显示 0k
 * 会与源数据矛盾），进位方向向上；null 不估算显示 —。
 */
import { describe, expect, it } from 'vitest'
import {
  abbreviateProject,
  axisTicks,
  cacheHitRate,
  COLLAPSE_THRESHOLD_CHARS,
  COLLAPSE_THRESHOLD_TOOL_CHARS,
  formatCost,
  formatDuration,
  formatEventTime,
  formatPercent,
  formatSessionTime,
  formatTokens,
  looksTruncated,
  metricValue,
  niceMax,
  splitToolText,
  totalTokens,
  GUEST_TEXT_CAPS,
  TRUNCATION_MIN_CAP,
} from '../utils/format'

describe('formatTokens', () => {
  it('千以下原样', () => {
    expect(formatTokens(0)).toBe('0')
    expect(formatTokens(42)).toBe('42')
    expect(formatTokens(999)).toBe('999')
  })

  it('k / M 分级一位小数', () => {
    expect(formatTokens(1000)).toBe('1.0k')
    expect(formatTokens(8_400_000)).toBe('8.4M')
    expect(formatTokens(1_050_000)).toBe('1.1M')
  })

  it('非零小量不产生假零', () => {
    // 40 tokens → 0.04k → 向上进位为 0.1k（显示 0k 与源数据矛盾）
    expect(formatTokens(40)).toBe('40') // 千以下原样
    expect(formatTokens(1004)).toBe('1.1k')
    expect(formatTokens(10_000_400)).toBe('10M') // ≥10M 取整（0.1% 级误差非假零）
  })

  it('空值与非法值显示 —', () => {
    expect(formatTokens(null)).toBe('—')
    expect(formatTokens(undefined)).toBe('—')
    expect(formatTokens(Number.NaN)).toBe('—')
    expect(formatTokens(-5)).toBe('0')
  })
})

describe('formatDuration', () => {
  it('分段缩写', () => {
    expect(formatDuration(500)).toBe('500ms')
    expect(formatDuration(42_000)).toBe('42s')
    expect(formatDuration(90_000)).toBe('1.5min')
    expect(formatDuration(96_000)).toBe('1.6min') // 向上取整到 0.1
    expect(formatDuration(21.4 * 3600 * 1000)).toBe('21.4h')
    expect(formatDuration(30 * 24 * 3600 * 1000)).toBe('30d')
  })

  it('空值显示 —', () => {
    expect(formatDuration(null)).toBe('—')
    expect(formatDuration(undefined)).toBe('—')
  })
})

describe('formatSessionTime / formatEventTime', () => {
  it('本地时区标签（用固定本地时间构造验证格式）', () => {
    const d = new Date(2026, 8, 12, 21, 47, 5) // 本地 2026-09-12 21:47:05
    expect(formatSessionTime(d.getTime())).toBe('09-12 21:47')
    expect(formatEventTime(d.getTime())).toBe('21:47:05')
  })

  it('空值：列表 — / 事件空串', () => {
    expect(formatSessionTime(null)).toBe('—')
    expect(formatEventTime(null)).toBe('')
  })
})

describe('abbreviateProject', () => {
  it('家目录折叠为 ~', () => {
    expect(abbreviateProject('/home/u/proj', '/home/u')).toBe('~/proj')
    expect(abbreviateProject('/home/u', '/home/u')).toBe('~')
  })

  it('非家目录与空值原样', () => {
    expect(abbreviateProject('/var/data', '/home/u')).toBe('/var/data')
    expect(abbreviateProject(null, '/home/u')).toBe('—')
  })
})

describe('formatCost', () => {
  it('有则存两位小数，null 不估算', () => {
    expect(formatCost(12.970714)).toBe('$12.97')
    expect(formatCost(0)).toBe('$0.00')
    expect(formatCost(null)).toBe('—')
  })
})

// ==================== 看板指标换算（票据 06 改版） ====================

describe('totalTokens（token 总量口径）', () => {
  it('输入 + 输出 + 缓存读 + 缓存写', () => {
    expect(totalTokens({ tokens_in: 10, tokens_out: 20, tokens_cache_read: 30, tokens_cache_write: 40 })).toBe(100)
  })

  it('反例守门：推理是输出的子集，不得再加一次（claude thinking_tokens 属于 output_tokens）', () => {
    // 若实现把 reasoning 也加进来，这里会得到 150 → 红
    expect(totalTokens({ tokens_in: 10, tokens_out: 20, tokens_cache_read: 30, tokens_cache_write: 40, tokens_reasoning: 50 })).toBe(100)
  })

  it('边界：缺字段 / null / 全 0 都归 0，不出 NaN', () => {
    expect(totalTokens({})).toBe(0)
    expect(totalTokens(null)).toBe(0)
    expect(totalTokens(undefined)).toBe(0)
    expect(totalTokens({ tokens_in: 0, tokens_out: 0, tokens_cache_read: 0, tokens_cache_write: 0 })).toBe(0)
  })
})

describe('metricValue（指标取值唯一口径）', () => {
  const row = {
    tokens_in: 1,
    tokens_out: 2,
    tokens_cache_read: 3,
    tokens_cache_write: 4,
    tokens_reasoning: 5,
    sessions: 6,
    duration_ms: 7_000,
    cost_total: 1.25,
  }

  it('逐指标取对应字段', () => {
    expect(metricValue(row, 'tokens_in')).toBe(1)
    expect(metricValue(row, 'tokens_out')).toBe(2)
    expect(metricValue(row, 'tokens_cache_read')).toBe(3)
    expect(metricValue(row, 'tokens_cache_write')).toBe(4)
    expect(metricValue(row, 'tokens_reasoning')).toBe(5)
    expect(metricValue(row, 'sessions')).toBe(6)
    expect(metricValue(row, 'duration_ms')).toBe(7_000)
    expect(metricValue(row, 'cost_total')).toBe(1.25)
  })

  it("'tokens' = 四桶之和（与 totalTokens 同一口径）", () => {
    expect(metricValue(row, 'tokens')).toBe(10)
  })

  it('边界：字段缺失归 0；成本为 null（未上报）时也是 0 而不是 NaN', () => {
    expect(metricValue({}, 'tokens_in')).toBe(0)
    expect(metricValue({}, 'tokens')).toBe(0)
    expect(metricValue({ cost_total: null }, 'cost_total')).toBe(0)
  })

  it("'tokens' 优先取节奏格预置的总量（该格无分桶）", () => {
    expect(metricValue({ tokens: 42, tokens_in: 1, tokens_out: 1 }, 'tokens')).toBe(42)
  })
})

describe('cacheHitRate（缓存命中率）', () => {
  it('缓存读 / (输入 + 缓存读 + 缓存写)', () => {
    expect(cacheHitRate({ tokens_in: 0, tokens_cache_read: 75, tokens_cache_write: 25 })).toBe(0.75)
    expect(cacheHitRate({ tokens_in: 100, tokens_cache_read: 300, tokens_cache_write: 0 })).toBe(0.75)
  })

  it('边界：分母为 0 → null（不是 0%——「没输入过」不等于「缓存没起作用」）', () => {
    expect(cacheHitRate({ tokens_in: 0, tokens_cache_read: 0, tokens_cache_write: 0 })).toBeNull()
    expect(cacheHitRate(null)).toBeNull()
  })

  it('无缓存命中时是 0 而不是 null（有分母、确实没命中）', () => {
    expect(cacheHitRate({ tokens_in: 500 })).toBe(0)
  })
})

describe('formatPercent', () => {
  it('0–1 小数转百分比', () => {
    expect(formatPercent(0.5)).toBe('50%')
    expect(formatPercent(0.256, 1)).toBe('25.6%')
    expect(formatPercent(0)).toBe('0%')
  })

  it('null / 非有限值不编造百分比', () => {
    expect(formatPercent(null)).toBe('—')
    expect(formatPercent(Number.NaN)).toBe('—')
  })
})

describe('niceMax（坐标轴上界）', () => {
  it('取 1 / 2 / 2.5 / 5 × 10^k 中不小于 max 的最小值', () => {
    expect(niceMax(1)).toBe(1)
    expect(niceMax(1.2)).toBe(2)
    expect(niceMax(0.3)).toBe(0.5)
    expect(niceMax(7)).toBe(10)
    expect(niceMax(120)).toBe(200)
  })

  it('边界：非正 / 非有限 → 1（避免除零与空路径），且永远 ≥ max', () => {
    expect(niceMax(0)).toBe(1)
    expect(niceMax(-5)).toBe(1)
    expect(niceMax(Number.NaN)).toBe(1)
    for (const m of [0.04, 3, 9, 11, 250, 999, 1_000, 12_345]) {
      expect(niceMax(m)).toBeGreaterThanOrEqual(m)
    }
  })
})

describe('axisTicks（刻度序列）', () => {
  it('自上而下：上界 → 0，等分 count+1 档', () => {
    expect(axisTicks(100, 3)).toEqual([100, 200 / 3, 100 / 3, 0])
    expect(axisTicks(10, 1)).toEqual([10, 0])
  })

  it('边界：上界 0 时不再全为 0 造成「刻度重叠」（0/3 = 0）', () => {
    // 0 上界下刻度全等，前端会画出 4 条重合线；此处只锁住「不会算出负数/NaN」
    for (const t of axisTicks(0, 3)) expect(Number.isFinite(t)).toBe(true)
  })
})

// ==================== 聊天视图：工具卡切片 / 折叠阈值 / 截断提示（README 票 B3 / B4） ====================
//
// 行为契约（每条对应 format.ts 的一个分支）：
// - C-T01 claude `tool_use · 名称`：类型与名称都在卡头，无卡身
// - C-T02 claude/codex `tool_result · 输出`：第二段起全是卡身（输出不再被切成「名称」）
// - C-T03 codex/opencode `tool · 名称 (状态) · 卡身`：头两段归卡头，余下归卡身
// - C-T04 pi `名称 (error) · 卡身`：无类型前缀，首段即卡头；`(error)` 置失败位
// - C-T05 未知形态（无 ` · ` 分隔）：整条落卡身，不凭空造头
// - C-T06 卡身里的 ` · ` 不被继续切碎（输出原文完整保留）
// - C-T07 失败标记只看**尾部** `(error)` 且大小写不敏感；正文中间出现不算
// - C-X01 超上限 + 以省略号收尾 → 疑似截断；C-X02 恰好等于上限 → 不报（边界）
// - C-X03 超上限但无省略号（自然长文本）→ 不报（反例：不能只看长度）
// - C-X04 按角色取上限：tool 400 / 其余 2000（与 guest 常量对齐）

describe('splitToolText（工具卡头 / 卡身切片）', () => {
  it('C-T01 claude tool_use：类型 + 名称在卡头，无卡身', () => {
    expect(splitToolText('tool_use · Bash')).toEqual({ head: 'tool_use · Bash', body: '', isError: false })
  })

  it('C-T02 claude/codex tool_result：输出整段落卡身，不被当成名称', () => {
    const r = splitToolText('tool_result · hello world')
    expect(r.head).toBe('tool_result')
    expect(r.body).toBe('hello world')
    expect(r.isError).toBe(false)
  })

  it('C-T02b claude 配对后的 tool_result 带名称：名称归卡头、输出归卡身', () => {
    const r = splitToolText('tool_result · Bash · total 0')
    expect(r.head).toBe('tool_result · Bash')
    expect(r.body).toBe('total 0')
  })

  it('C-T01b claude tool_use 带参数摘要：参数归卡身（不被丢掉）', () => {
    const r = splitToolText('tool_use · Bash · {"command":"ls -l"}')
    expect(r.head).toBe('tool_use · Bash')
    expect(r.body).toBe('{"command":"ls -l"}')
  })

  it('C-T03 codex/opencode：头两段归卡头（类型 · 名称 (状态)），余下归卡身', () => {
    const r = splitToolText('tool · read (completed) · ok')
    expect(r.head).toBe('tool · read (completed)')
    expect(r.body).toBe('ok')
  })

  it('C-T04 pi：无类型前缀，首段即卡头；尾部 (error) 置失败位', () => {
    expect(splitToolText('bash (error) · command not found')).toEqual({
      head: 'bash (error)',
      body: 'command not found',
      isError: true,
    })
    expect(splitToolText('bash · hi')).toEqual({ head: 'bash', body: 'hi', isError: false })
  })

  it('C-T05 无分隔符的未知形态整条落卡身（不编头）', () => {
    expect(splitToolText('just some text')).toEqual({ head: '', body: 'just some text', isError: false })
  })

  it('C-T06 卡身里的分隔符不再切碎（输出原文完整，一段不少）', () => {
    const r = splitToolText('tool_result · a · b · c')
    // 有第三段时按「带名称」形态解读：a 进卡头，b · c 进卡身，join 回去等于原文
    expect(r.head).toBe('tool_result · a')
    expect(r.body).toBe('b · c')
    expect(`${r.head} · ${r.body}`).toBe('tool_result · a · b · c')
    const named = splitToolText('tool · read (completed) · a · b')
    expect(named.head).toBe('tool · read (completed)')
    expect(named.body).toBe('a · b')
  })

  it('C-T07 失败标记只认尾部 (error)，大小写不敏感；正文中间出现不算', () => {
    expect(splitToolText('tool · grep (ERROR) · a').isError).toBe(true)
    expect(splitToolText('tool · grep · pattern (error) here').isError).toBe(false)
    expect(splitToolText('grep (error)').isError).toBe(true)
  })
})

describe('looksTruncated（解析层截断提示）', () => {
  it('C-X01 末字符是省略号且末段够长 → 疑似截断（guest truncate_text 的真实形态）', () => {
    expect(looksTruncated('x'.repeat(GUEST_TEXT_CAPS.toolOutput) + '…')).toBe(true)
    expect(looksTruncated('x'.repeat(GUEST_TEXT_CAPS.message) + '…')).toBe(true)
  })

  it('C-X02 边界：短句以省略号自然收尾不报（「等等…」不算截断）', () => {
    expect(looksTruncated('等等…')).toBe(false)
    // 排除阀取「最小上限」：末段 ≤ 120 视为自然收尾
    expect(looksTruncated('x'.repeat(TRUNCATION_MIN_CAP - 1) + '…')).toBe(false)
    // 恰好等于最小上限的**真截断**仍要报（guest 截断后末段 = cap + 省略号 = 121）
    expect(looksTruncated('x'.repeat(TRUNCATION_MIN_CAP) + '…')).toBe(true)
  })

  it('C-X03 反例：够长但没有省略号 → 不报（自然长文本不能误报）', () => {
    expect(looksTruncated('x'.repeat(GUEST_TEXT_CAPS.message + 1))).toBe(false)
  })

  it('C-X05 卡内嵌套上限：参数摘要 120 上限的行也能被识别（整条远不到 400）', () => {
    // guest `TOOL_ARGS_CAP = 120`：`tool_use · 名称 · 参数…` 整条才 ~140 字
    const args = 'x'.repeat(GUEST_TEXT_CAPS.toolArgs) + '…'
    expect(args.length).toBeLessThan(GUEST_TEXT_CAPS.toolOutput)
    expect(looksTruncated(`tool_use · Bash · ${args}`)).toBe(true)
    // 末段够长但整条没到任何上限时，靠的是末段判定而不是整条长度
    expect(looksTruncated(`tool_result · ok`)).toBe(false)
  })

  it('C-X06 多段工具行只判末段：前面的段含省略号不算', () => {
    expect(looksTruncated(`tool · 名称 … · ${'x'.repeat(TRUNCATION_MIN_CAP + 1)}`)).toBe(false)
  })

  // 非工具行（user / assistant / system 正文）里出现 ` · ` 是常态，
  // 只看末段会把「正文被截到 2000」的整条真截断漏报（提示与原文入口一起消失）
  it('C-X07 非工具行：正文被截断（末段短但整条长）也要报', () => {
    const body = `${'超长正文。'.repeat(400)} · 收尾`
    expect(body.length).toBeGreaterThan(GUEST_TEXT_CAPS.message)
    expect(body.split(' · ').at(-1)!.length).toBeLessThan(TRUNCATION_MIN_CAP)
    expect(looksTruncated(`${body}…`)).toBe(true)
  })

  it('C-X08 非工具行：排除阀按整条长度（非工具行不豁免）', () => {
    expect(looksTruncated('配置 · 超时')).toBe(false)
    // 整条 = 排除阀 120（与单段同一条边界）不报；越一行（121）才报
    expect(looksTruncated('x'.repeat(TRUNCATION_MIN_CAP - 1) + '…')).toBe(false)
    expect(looksTruncated('x'.repeat(TRUNCATION_MIN_CAP) + '…')).toBe(true)
  })

  it('C-X04 上限镜像与 guest 常量对齐（漂移即可见）', () => {
    expect(GUEST_TEXT_CAPS.message).toBe(2000) // 四适配器消息正文
    expect(GUEST_TEXT_CAPS.toolOutput).toBe(1000) // CODEX/OPENCODE_TOOL_TEXT_CAP + claude/pi 字面量
    expect(GUEST_TEXT_CAPS.toolArgs).toBe(120) // claude TOOL_ARGS_CAP
    expect(GUEST_TEXT_CAPS.title).toBe(120)
    expect(TRUNCATION_MIN_CAP).toBe(120) // 最小上限 = 排除阀
  })
})

describe('折叠阈值（工具卡比助手正文更早收起）', () => {
  it('工具卡阈值低于助手正文阈值，且不高于工具输出上限的一半', () => {
    expect(COLLAPSE_THRESHOLD_TOOL_CHARS).toBeLessThan(COLLAPSE_THRESHOLD_CHARS)
    expect(COLLAPSE_THRESHOLD_TOOL_CHARS).toBeLessThanOrEqual(GUEST_TEXT_CAPS.toolOutput / 2)
    // 阈值下调后仍保持“短工具直接铺开、长工具收起点开”的分界
    expect(COLLAPSE_THRESHOLD_TOOL_CHARS).toBeGreaterThanOrEqual(300)
  })
})

describe('工具卡内的非文本块占位（guest token → 本地化文案）', () => {
  /** 假 i18n：记录调用并回显 key + 参数（断言落在「取了哪个 key / 带了什么参数」） */
  const fakeT = (key: string, params?: Record<string, unknown>) =>
    `${key}:${params ? JSON.stringify(params) : ''}`

  /** 走组件真正使用的入口（splitToolText 内部做占位本地化） */
  const body = (text: string, t = fakeT) => splitToolText(text, t).body

  it('带 kind 的 token → 取带参数的 key（块类型名不翻译，原样作参数）', () => {
    expect(body('tool_result · Read · [non-text:image]')).toBe('hub.lg.nonTextBlock:{"kind":"image"}')
  })

  it('无 kind 的退化 token → 取无参数 key（guest 校验不通过时的形态）', () => {
    expect(body('tool_result · Read · [non-text]')).toBe('hub.lg.nonTextBlockUnknown:')
  })

  it('多块内容按序替换，其余字符不动', () => {
    expect(body('tool_result · Read · 第一段\n[non-text:image]\n第三段')).toBe(
      '第一段\nhub.lg.nonTextBlock:{"kind":"image"}\n第三段',
    )
  })

  it('无 token 时原样返回（快速路径不碰正文）', () => {
    expect(body('tool_result · Bash · plain output')).toBe('plain output')
  })

  it('形态不符的「半截 token」不替换（宁可漏报，不可篡改真实输出）', () => {
    for (const raw of ['[non-text:图像', 'non-text:image]', '[non-text:<script>]', '[non-text :x]', '[non-text:a b]']) {
      expect(body(`tool_result · Bash · ${raw}`)).toBe(raw)
    }
  })

  it('不传 t 时不本地化（纯文本工具零影响；组件接线另有组件层用例锁）', () => {
    expect(splitToolText('tool_result · Read · [non-text:image]').body).toBe('[non-text:image]')
  })
})
