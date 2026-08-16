/**
 * useOcr 业务逻辑单测（spec §6.2/§6.3）
 *
 * 覆盖：enginePhase 派生、识别流（成功/取消/防重入）、
 * 错误映射（模型缺失/权限拒绝/无相机/解码失败）、模型管理、
 * 共享结果写入与清空。
 */
import { beforeEach, describe, it, expect, vi } from 'vitest'
import type { PluginContext, OcrApi, OcrLine } from '@binblink/plugin-sdk-mobile'
import {
  useOcr,
  clearOcrResult,
  mapRecognizeError,
  LOW_CONFIDENCE_THRESHOLD,
  RESULT_ROUTE_ID,
} from '../composables/useOcr'

/** 构造带可注入 ocr API 的假 PluginContext（useOcr 仅消费 ocr + logger） */
function makeContext(overrides: Partial<OcrApi> = {}): { context: PluginContext; ocr: OcrApi } {
  const ocr: OcrApi = {
    recognize: vi.fn(async () => ({ engine: 'offline', durationMs: 50, lines: [] })),
    engineStatus: vi.fn(async () => ({
      available: true,
      modelsPresent: true,
      modelsBytes: 17_417_728,
      engineLoaded: true,
      supportedEngines: ['offline'],
    })),
    deleteModels: vi.fn(async () => ({ deleted: true, freedBytes: 17_417_728 })),
    restoreModels: vi.fn(async () => ({ restored: true })),
    pickImage: vi.fn(async () => ({ path: '/cache/ocr/a.rgba', width: 1000, height: 750 })),
    cameraCapture: vi.fn(async () => ({ path: '/cache/ocr/b.rgba', width: 1200, height: 900 })),
    ...overrides,
  }
  const context = {
    id: 'com.bedcode.ocr',
    ocr,
    logger: { info: vi.fn(), debug: vi.fn(), warn: vi.fn(), error: vi.fn() },
  } as unknown as PluginContext
  return { context, ocr }
}

function linesFixture(): OcrLine[] {
  return [
    { text: 'Hello', confidence: 0.98, bbox: { x: 0, y: 0, w: 10, h: 10 } },
    { text: '低置信', confidence: 0.3, bbox: { x: 0, y: 20, w: 10, h: 10 } },
  ]
}

describe('enginePhase 派生', () => {
  it('状态未知 → unknown', () => {
    const { context } = makeContext()
    const ocr = useOcr(context)
    expect(ocr.enginePhase.value).toBe('unknown')
  })

  it('available=false → unavailable（引擎不可用）', async () => {
    const { context, ocr } = makeContext({
      engineStatus: vi.fn(async () => ({
        available: false,
        modelsPresent: true,
        modelsBytes: 0,
        engineLoaded: false,
        supportedEngines: ['offline'],
      })),
    })
    const o = useOcr(context)
    await o.refreshEngineStatus()
    expect(o.enginePhase.value).toBe('unavailable')
  })

  it('模型缺失 → missing（引导恢复）', async () => {
    const { context, ocr } = makeContext({
      engineStatus: vi.fn(async () => ({
        available: true,
        modelsPresent: false,
        modelsBytes: 0,
        engineLoaded: false,
        supportedEngines: ['offline'],
      })),
    })
    const o = useOcr(context)
    await o.refreshEngineStatus()
    expect(o.enginePhase.value).toBe('missing')
  })

  it('模型在位但引擎未加载 → loading', async () => {
    const { context, ocr } = makeContext({
      engineStatus: vi.fn(async () => ({
        available: true,
        modelsPresent: true,
        modelsBytes: 1,
        engineLoaded: false,
        supportedEngines: ['offline'],
      })),
    })
    const o = useOcr(context)
    await o.refreshEngineStatus()
    expect(o.enginePhase.value).toBe('loading')
  })

  it('模型在位且引擎常驻 → ready', async () => {
    const { context } = makeContext()
    const o = useOcr(context)
    await o.refreshEngineStatus()
    expect(o.enginePhase.value).toBe('ready')
  })

  it('engineStatus 抛错 → 状态回落 unknown（不崩）', async () => {
    const { context, ocr } = makeContext({
      engineStatus: vi.fn(async () => {
        throw new Error('plugin not activated')
      }),
    })
    const o = useOcr(context)
    await o.refreshEngineStatus()
    expect(o.enginePhase.value).toBe('unknown')
  })
})

describe('识别流', () => {
  beforeEach(() => clearOcrResult())

  it('相册选图 → 识别成功：写入共享结果并返回 true', async () => {
    const { context, ocr } = makeContext({
      recognize: vi.fn(async () => ({ engine: 'offline', durationMs: 88, lines: linesFixture() })),
    })
    const o = useOcr(context)
    const ok = await o.recognizeFromAlbum()
    expect(ok).toBe(true)
    expect(ocr.pickImage).toHaveBeenCalledOnce()
    expect(o.lines.value).toHaveLength(2)
    expect(o.lines.value![1].confidence).toBeLessThan(LOW_CONFIDENCE_THRESHOLD)
    expect(o.durationMs.value).toBe(88)
    expect(o.source.value?.width).toBe(1000)
  })

  it('拍照 → 识别成功（同链路）', async () => {
    const { context, ocr } = makeContext({
      recognize: vi.fn(async () => ({ engine: 'offline', durationMs: 60, lines: linesFixture() })),
    })
    const o = useOcr(context)
    expect(await o.recognizeFromCamera()).toBe(true)
    expect(ocr.cameraCapture).toHaveBeenCalledOnce()
    expect(o.lines.value).toHaveLength(2)
  })

  it('用户取消（null）→ 静默返回 false，不写入结果', async () => {
    const { context, ocr } = makeContext({
      pickImage: vi.fn(async () => null),
      recognize: vi.fn(),
    })
    const o = useOcr(context)
    expect(await o.recognizeFromAlbum()).toBe(false)
    expect(ocr.recognize).not.toHaveBeenCalled()
    expect(o.lines.value).toBeNull()
  })

  it('防重入：识别中再次触发立即返回 false，源只调一次', async () => {
    const { context, ocr } = makeContext()
    let release: (v: unknown) => void = () => {}
    const gate = new Promise((resolve) => {
      release = resolve
    })
    const pickImage = vi.fn(async () => {
      await gate
      return { path: '/cache/ocr/a.rgba', width: 1, height: 1 }
    })
    const recognize = vi.fn(async () => ({ engine: 'offline', durationMs: 1, lines: [] }))
    const o = useOcr(makeContext({ pickImage, recognize }).context)

    const first = o.recognizeFromAlbum()
    const second = await o.recognizeFromAlbum()
    expect(second).toBe(false)
    expect(pickImage).toHaveBeenCalledTimes(1)
    release(null)
    expect(await first).toBe(true)
    expect(pickImage).toHaveBeenCalledTimes(1)
  })

  it('识别抛错 → 上抛给调用方且复位 recognizing（可重试）', async () => {
    const { context, ocr } = makeContext({
      recognize: vi.fn(async () => {
        throw new Error('plugin_ocr_recognize: models not extracted (restore via ...)')
      }),
    })
    const o = useOcr(context)
    await expect(o.recognizeFromAlbum()).rejects.toThrow(/models not extracted/)
    expect(o.recognizing.value).toBe(false)
    // 再次触发不再被防重入拦截
    ocr.recognize = vi.fn(async () => ({ engine: 'offline', durationMs: 1, lines: [] }))
    expect(await o.recognizeFromAlbum()).toBe(true)
  })
})

describe('错误映射（错误串 → i18n key）', () => {
  it('模型缺失 → 引导恢复 key', () => {
    expect(
      mapRecognizeError(new Error('plugin_ocr_recognize: models not extracted (restore via plugin_ocr_restore_models)'))
        .key,
    ).toBe('ocr.home.modelsMissing')
  })

  it('相机权限拒绝 → permissionDenied', () => {
    expect(mapRecognizeError(new Error('Camera permission denied; grant it in system settings')).key).toBe(
      'ocr.error.permissionDenied',
    )
  })

  it('无相机应用 → noCameraApp', () => {
    expect(mapRecognizeError(new Error('No camera app available: ActivityNotFoundException')).key).toBe(
      'ocr.error.noCameraApp',
    )
  })

  it('解码失败 → decodeFailed', () => {
    expect(mapRecognizeError(new Error('Failed to decode image: unsupported or corrupted image')).key).toBe(
      'ocr.error.decodeFailed',
    )
  })

  it('未知错误 → recognizeFailed 兜底', () => {
    expect(mapRecognizeError(new Error('boom')).key).toBe('ocr.error.recognizeFailed')
  })
})

describe('模型管理（设置区）', () => {
  it('删除模型：调用宿主 + 刷新引擎状态（modelsPresent=false → missing）', async () => {
    const status = vi.fn(async () => ({
      available: true,
      modelsPresent: false,
      modelsBytes: 0,
      engineLoaded: false,
      supportedEngines: ['offline'],
    }))
    const { context, ocr } = makeContext({
      engineStatus: status,
      deleteModels: vi.fn(async () => ({ deleted: true, freedBytes: 1 })),
    })
    const o = useOcr(context)
    await o.deleteModels()
    expect(ocr.deleteModels).toHaveBeenCalledOnce()
    expect(status).toHaveBeenCalled()
    expect(o.enginePhase.value).toBe('missing')
  })

  it('恢复模型：调用宿主 + 刷新引擎状态 → ready（入口解禁）', async () => {
    // 可变引擎状态：删除后 modelsPresent=false，恢复后回 true
    let present = true
    const status = vi.fn(async () => ({
      available: true,
      modelsPresent: present,
      modelsBytes: present ? 1 : 0,
      engineLoaded: present,
      supportedEngines: ['offline'],
    }))
    const { context, ocr } = makeContext({
      engineStatus: status,
      deleteModels: vi.fn(async () => {
        present = false
        return { deleted: true, freedBytes: 1 }
      }),
      restoreModels: vi.fn(async () => {
        present = true
        return { restored: true }
      }),
    })
    const o = useOcr(context)
    // 先置为缺失
    await o.deleteModels()
    expect(o.enginePhase.value).toBe('missing')
    await o.restoreModels()
    expect(ocr.restoreModels).toHaveBeenCalledOnce()
    expect(o.enginePhase.value).toBe('ready')
  })

  it('modelBusy 防重入：删除中再次触发不重复调用', async () => {
    const { context, ocr } = makeContext()
    let release: (v: unknown) => void = () => {}
    const gate = new Promise((resolve) => {
      release = resolve
    })
    ocr.deleteModels = vi.fn(async () => {
      await gate
      return { deleted: true, freedBytes: 1 }
    })
    const o = useOcr(context)
    const first = o.deleteModels()
    const second = o.deleteModels()
    release(null) // 先放行 gate，再等待两个调用收敛
    await Promise.all([first, second])
    expect(ocr.deleteModels).toHaveBeenCalledTimes(1)
  })
})

describe('共享结果', () => {
  it('clearOcrResult 清空模块级结果（结果页返回主页后无陈旧数据）', async () => {
    const { context } = makeContext()
    const o = useOcr(context)
    await o.recognizeFromAlbum()
    expect(o.lines.value).not.toBeNull()
    clearOcrResult()
    expect(o.lines.value).toBeNull()
    expect(o.source.value).toBeNull()
  })

  it('结果页路由 id 与 manifest contributes.routes 一致', () => {
    expect(RESULT_ROUTE_ID).toBe('result')
  })
})
