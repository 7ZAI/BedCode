/**
 * 终端域 HTTP 面（票 15：随终端 UI 域迁入）
 *
 * 经 `mobileApi.httpRequest` 走宿主 JWT / 链路加密 / 错误归一化通道，
 * 插件不直连网络（前端零资源访问红线；与 task/api.ts 同一范式）。
 */

import { getMobileApi, type MobileHttpResult } from '@binblink/bedcode-plugin-sdk-mobile'

/** 渲染端来源（与桌面端 resize 协议同词表） */
export type RendererSource = { kind: 'desktop' } | { kind: 'mobile'; deviceName: string }

/** resize 裁决结果（服务端「正统渲染端」仲裁） */
export type ResizeOutcome =
  | { status: 'applied'; canonical: RendererSource }
  | { status: 'needsConfirmation'; currentCanonical: RendererSource }

/** 上报终端网格尺寸（POST /api/sessions/{id}/resize） */
export function httpResizeSession(
  sessionId: string,
  cols: number,
  rows: number,
  force = false,
): Promise<MobileHttpResult<ResizeOutcome>> {
  return getMobileApi().httpRequest<ResizeOutcome>(`/api/sessions/${sessionId}/resize`, {
    method: 'POST',
    body: { cols, rows, force },
  })
}

/** 发送终端输入（POST /api/sessions/{id}/input；TUI 兼容滚动等旁路场景） */
export function httpSendSessionInput(
  sessionId: string,
  data: string,
  specialKey?: string,
): Promise<MobileHttpResult> {
  return getMobileApi().httpRequest(`/api/sessions/${sessionId}/input`, {
    method: 'POST',
    body: { data, specialKey: specialKey || null },
  })
}
