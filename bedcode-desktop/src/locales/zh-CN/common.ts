export default {
  common: {
    button: {
      cancel: '取消',
      stop: '停止',
    },
    status: {
      error: '错误',
      unknown: '未知',
      running: '运行中',
      stopped: '已停止',
      asking: '等待输入',
      starting: '启动中',
    },
    // 票 01（F 组）：common.errorCode.ipcTimeout 随 InvokeTimeoutError 退役删除
    // （{cmd} 泄漏命令名 + 无调用点；超时展示统一走 errors.host.invoke.timeout）
    misc: {
      terminalTitle: '终端 - {name}',
    },
  },
  // ==================== 错误信封（ADR 0030）====================
  // 顶层命名空间 errors.<code> ↔ 信封 code，零映射层；改码 = 破坏性变更。
  // 注册表 v0 基码（票 01）：兜底 / 超时 / 前端兜底；host.plugin.* 运行时域见票 03。
  // UI 呈现规则：toast / 页面只显示友好文案（模板 + params 插值），永不显示错误码 / request_id / 技术详情。
  errors: {
    retry: '重试',
    host: {
      internal: '操作未完成，请稍后重试',
      invoke: {
        timeout: '操作超时，请重试',
      },
      // 票 02 机制码（ADR 0030 注册表 v0）：插件管理面机制失败
      // + 票 03 运行时段（事件通道信封，ADR 0030 决定 7 / 11）——同一 kind 对用户是
      // 同一件事 → 同一文案；插值参数只允许应用显示名
      plugin: {
        'not-activated': '该应用未启用，无法执行此操作',
        'not-found': '应用不存在或已卸载',
        trap: '应用「{name}」运行异常，已尝试自动恢复',
        'recovery-failed': '应用「{name}」运行异常且未能自动恢复，请到应用中心处理',
        'self-check-failed': '应用「{plugin}」启动自检失败，请检查配置',
        degraded: '应用「{name}」降级运行，部分功能不可用',
      },
    },
    frontend: {
      internal: '操作未完成，请稍后重试',
    },
  },
}
