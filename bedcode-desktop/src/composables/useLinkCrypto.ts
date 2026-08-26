/**
 * 链路加密配置（issue 08）
 *
 * 封装 issue 01 的三个 Tauri 命令：
 * - get_traffic_encryption_config / set_traffic_encryption_config（spec §6 配置域）
 * - get_link_crypto_fingerprint（本机 Kd 指纹，供人工核对）
 *
 * 字段名与 Rust LinkCryptoConfig 的 serde 序列化一致（snake_case；
 * deny_unknown_fields 拒绝 camelCase 拼写）。
 */
import { invoke } from '@tauri-apps/api/core'
import { ref } from 'vue'

/** 链路加密配置域（镜像 server/link_crypto.rs LinkCryptoConfig） */
export interface LinkCryptoConfig {
  /** 主开关：false 时过滤器不注册，全服务明文（与现状一致） */
  enabled: boolean
  /** HTTP REST 载荷加解密 */
  encrypt_http: boolean
  /** WS 终端通道帧加解密 */
  encrypt_ws_terminal: boolean
  /** WS 事件通道帧加解密 */
  encrypt_ws_event: boolean
  /** 服务端对未协商的老客户端放行明文；false 时非环回未协商请求一律拒绝 */
  allow_plaintext_fallback: boolean
}

/** 配置响应式快照；null = 尚未从后端加载 */
const config = ref<LinkCryptoConfig | null>(null)

/** 本机 Kd 指纹（SHA-256 前 16 hex）；null = 未加载/懒初始化失败 */
const fingerprint = ref<string | null>(null)

/** 加载中标记（防止 onMounted 与手动刷新竞态重复请求） */
let loading: Promise<void> | null = null

/**
 * 从后端拉取配置与指纹（幂等，可安全重复调用）
 *
 * 指纹懒初始化失败不阻断设置页：展示占位并由后端日志兜底。
 */
export async function loadLinkCryptoConfig(): Promise<void> {
  if (loading) return loading
  loading = (async () => {
    try {
      config.value = await invoke<LinkCryptoConfig>('get_traffic_encryption_config')
      fingerprint.value = await invoke<string>('get_link_crypto_fingerprint')
    } finally {
      loading = null
    }
  })()
  return loading
}

/**
 * 保存配置：先持久化成功再热更新快照（后端命令保证顺序），失败上抛由调用方提示
 */
export async function saveLinkCryptoConfig(next: LinkCryptoConfig): Promise<void> {
  await invoke('set_traffic_encryption_config', { config: next })
  config.value = next
}

/** 配置快照只读访问（未加载时 null） */
export function useLinkCryptoConfig() {
  return { config, fingerprint, loadLinkCryptoConfig, saveLinkCryptoConfig }
}
