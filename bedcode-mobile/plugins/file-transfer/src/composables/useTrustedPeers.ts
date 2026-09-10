/**
 * 可信对端管理编排 — 列表加载 + 撤销信任命令路由（spec 决策 8）
 *
 * 数据经插件命令面：`file-transfer.list-trusted`（宿主 TrustedPeerDto
 * camelCase 形状数组）与 `file-transfer.revoke-trusted`（撤销后对端重连
 * 将重新走首连确认）。加载失败以 errorKey 如实呈现（i18n key 而非硬编码文案），
 * 撤销失败保留条目并上报错误，由设置分区渲染重试/确认交互。
 *
 * 本文件只做编排；条目归一化与时间格式化为纯函数独立直测。测试以 mock
 * PluginContext（commands.execute 记录调用）驱动，见宿主测试套件
 * plugins/file-transfer 子目录。
 */
import { ref, type Ref } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'

/** 可信对端条目（宿主 TrustedPeerDto camelCase 契约形状） */
export interface TrustedPeer {
  nodeId: string
  /** 展示名：持久化名优先、在线缓存名次之；均缺为 null（UI 以短指纹兜底） */
  displayName: string | null
  /** 短指纹（前 8 位） */
  fingerprintShort: string
  /** 加入可信列表时刻（RFC3339，展示层经 formatTrustedDate 本地化） */
  addedAt: string
}

/** 列表加载状态（error 时 errorKey 指示具体文案） */
export type TrustedLoadState = 'idle' | 'loading' | 'ready' | 'error'

/**
 * 条目归一化（纯函数）：容忍异形响应，畸形条目逐项剔除而非整体作废；
 * 非数组输入一律回退空列表——调用方呈现空态而非报错。
 */
export function normalizeTrustedPeers(raw: unknown): TrustedPeer[] {
  if (!Array.isArray(raw)) return []
  const peers: TrustedPeer[] = []
  for (const entry of raw) {
    if (!entry || typeof entry !== 'object') continue
    const e = entry as Record<string, unknown>
    if (typeof e.nodeId !== 'string' || e.nodeId === '') continue
    peers.push({
      nodeId: e.nodeId,
      displayName: typeof e.displayName === 'string' && e.displayName !== '' ? e.displayName : null,
      fingerprintShort:
        typeof e.fingerprintShort === 'string' && e.fingerprintShort !== ''
          ? e.fingerprintShort
          : e.nodeId.slice(0, 8),
      addedAt: typeof e.addedAt === 'string' ? e.addedAt : '',
    })
  }
  return peers
}

/**
 * 加入时间格式化（纯函数）：跟随传入 locale 本地化；无效日期原样返回兜底展示。
 * locale 非法（Intl 不识别的 tag）时回退运行时默认 locale，不抛错。
 */
export function formatTrustedDate(dateStr: string, locale?: string): string {
  if (!dateStr) return ''
  const date = new Date(dateStr)
  if (Number.isNaN(date.getTime())) return dateStr
  const options: Intl.DateTimeFormatOptions = {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  }
  try {
    return new Intl.DateTimeFormat(locale || undefined, options).format(date)
  } catch {
    return new Intl.DateTimeFormat(undefined, options).format(date)
  }
}

export function useTrustedPeers(context: PluginContext) {
  // ==================== 状态 ====================

  /** 可信对端快照（refresh 全量替换；撤销成功后本地摘除即时反馈） */
  const peers = ref<TrustedPeer[]>([]) as Ref<TrustedPeer[]>
  /** 加载状态机（分区按 loading/ready/error 分支渲染） */
  const loadState = ref<TrustedLoadState>('idle') as Ref<TrustedLoadState>
  /**
   * 当前错误文案 i18n key（加载失败 loadFailed / 撤销失败 revokeFailed；
   * 成功路径清空。存 key 不存文案——composable 禁中文硬编码）
   */
  const errorKey = ref('') as Ref<string>
  /** 撤销在途节点集合（行内按钮禁用防重复提交） */
  const revokingIds = ref<ReadonlySet<string>>(new Set())

  // ==================== 对外操作 ====================

  /** 拉取可信对端列表：失败置 error 态并记录 i18n key，不吞异常状态 */
  async function refresh(): Promise<void> {
    loadState.value = 'loading'
    try {
      const raw = await context.commands.execute('file-transfer.list-trusted', {})
      peers.value = normalizeTrustedPeers(raw)
      loadState.value = 'ready'
      errorKey.value = ''
    } catch (e) {
      console.error('[File Transfer] list-trusted failed:', e)
      loadState.value = 'error'
      errorKey.value = 'transfer.trusted.loadFailed'
    }
  }

  /**
   * 撤销信任：命令成功后本地摘除该条目（对方重连将重新走首连确认）；
   * 失败保留条目并记录错误 key，由调用方 toast/提示。
   */
  async function revoke(nodeId: string): Promise<boolean> {
    revokingIds.value = new Set(revokingIds.value).add(nodeId)
    try {
      await context.commands.execute('file-transfer.revoke-trusted', { nodeId })
      peers.value = peers.value.filter((p) => p.nodeId !== nodeId)
      errorKey.value = ''
      return true
    } catch (e) {
      console.error('[File Transfer] revoke-trusted failed:', e)
      errorKey.value = 'transfer.trusted.revokeFailed'
      return false
    } finally {
      const next = new Set(revokingIds.value)
      next.delete(nodeId)
      revokingIds.value = next
    }
  }

  return {
    peers,
    loadState,
    errorKey,
    revokingIds,
    refresh,
    revoke,
  }
}
