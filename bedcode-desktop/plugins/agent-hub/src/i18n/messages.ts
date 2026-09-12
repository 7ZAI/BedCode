/**
 * Agent Hub 插件 i18n 消息类型（唯一 key 来源）
 *
 * zh-CN 与 en 两个语言文件都必须实现该类型：
 * 新增/遗漏 key 在编译期即报错，保证两个语言文件的 key 永远同步。
 * 所有 key 以 `hub.` 域命名，注册时经插件 ID 前缀隔离为
 * `com.bedcode.agent-hub.hub.*`。
 * 用 type 别名而非 interface：TS 给类型别名隐式索引签名，使 MessageSchema
 * 可直接赋给 `Record<string, unknown>`（registerMessages 入参）。
 */

export type MessageSchema = {
  // ==================== 侧边栏 ====================
  'hub.sidebar.title': string

  // ==================== 顶部 tabs（变体 B） ====================
  'hub.tab.overview': string
  'hub.tab.install': string
  'hub.tab.skills': string
  'hub.tab.providers': string
  'hub.tab.stats': string
  'hub.tab.logs': string

  // ==================== 分区占位（票据 03–07 交付） ====================
  'hub.placeholder.hint': string
  'hub.placeholder.install.title': string
  'hub.placeholder.skills.title': string
  'hub.placeholder.providers.title': string
  'hub.placeholder.stats.title': string
  'hub.placeholder.logs.title': string

  // ==================== 概览 · 环境条 ====================
  'hub.env.title': string
  'hub.env.node': string
  'hub.env.npm': string
  'hub.env.pnpm': string
  'hub.env.registry': string
  'hub.env.detect': string
  'hub.env.detecting': string
  'hub.env.none': string

  // ==================== 概览 · CLI 卡片 ====================
  'hub.card.installed': string
  'hub.card.notInstalled': string
  'hub.card.error': string
  'hub.card.detecting': string
  'hub.card.dual': string
  'hub.card.dualWarning': string
  'hub.card.versionUnknown': string
  'hub.card.idle': string
  'hub.card.method.npm-global': string
  'hub.card.method.native': string
  'hub.card.method.standalone': string
  'hub.card.method.unknown': string

  // ==================== 目录授权 ====================
  'hub.auth.banner': string
  'hub.auth.action': string

  // ==================== 安装与更新 · 测速/镜像（票据 03） ====================
  'hub.speed.title': string
  'hub.speed.action': string
  'hub.speed.testing': string
  'hub.speed.official': string
  'hub.speed.mirror': string
  'hub.speed.recommendMirror': string
  'hub.speed.recommendOfficial': string
  'hub.speed.current': string
  'hub.speed.fail': string
  'hub.speed.goto': string
  'hub.mirror.persist': string
  'hub.mirror.persistConfirm': string
  'hub.mirror.restore': string
  'hub.mirror.tmp': string

  // ==================== 安装与更新 · CLI 行/控制台（票据 03） ====================
  'hub.inst.title': string
  'hub.inst.check': string
  'hub.inst.checking': string
  'hub.inst.install': string
  'hub.inst.update': string
  'hub.inst.latest': string
  'hub.inst.running': string
  'hub.inst.manualHint': string
  'hub.inst.nodeGuide': string
  'hub.inst.copy': string
  'hub.inst.copied': string
  'hub.inst.done': string
  'hub.inst.failed': string
  'hub.inst.cancelled': string
  'hub.inst.cancel': string
  'hub.inst.outputEmpty': string
  'hub.inst.console': string

  // ==================== Skills 管理（票据 04） ====================
  'hub.skill.scan': string
  'hub.skill.scanning': string
  'hub.skill.library': string
  'hub.skill.targetSync': string
  'hub.skill.targetStale': string
  'hub.skill.targetIdle': string
  'hub.skill.targetNoConvention': string
  'hub.skill.scanError': string
  'hub.skill.empty': string
  'hub.skill.emptyHint': string
  'hub.skill.edit': string
  'hub.skill.distribute': string
  'hub.skill.redistribute': string
  'hub.skill.distributing': string
  'hub.skill.status.libraryOnly': string
  'hub.skill.status.synced': string
  'hub.skill.status.stale': string

  // ==================== Skills · GitHub 安装（票据 04） ====================
  'hub.skill.github.action': string
  'hub.skill.github.title': string
  'hub.skill.github.urlPlaceholder': string
  'hub.skill.github.hint': string
  'hub.skill.github.install': string
  'hub.skill.github.installing': string
  'hub.skill.github.done': string
  'hub.skill.github.skipped': string
  'hub.skill.github.exists': string
  'hub.skill.github.overwrite': string
  'hub.skill.github.unreachable': string
  'hub.skill.github.failed': string

  // ==================== Skills · 本地导入（票据 04） ====================
  'hub.skill.import.action': string
  'hub.skill.import.exists': string
  'hub.skill.import.overwrite': string
  'hub.skill.import.authDenied': string
  'hub.skill.import.done': string
  'hub.skill.import.failed': string

  // ==================== Skills · 编辑器（票据 04） ====================
  'hub.skill.editor.title': string
  'hub.skill.editor.back': string
  'hub.skill.editor.save': string
  'hub.skill.editor.clean': string
  'hub.skill.editor.saved': string
  'hub.skill.editor.previewTitle': string
  'hub.skill.editor.confirmSave': string
  'hub.skill.editor.continueEdit': string
  'hub.skill.editor.conflict': string
  'hub.skill.editor.overwriteMine': string
  'hub.skill.editor.reload': string

  // ==================== 供应商管理（票据 05） ====================
  'hub.pv.secure': string
  'hub.pv.new': string
  'hub.pv.import': string
  'hub.pv.importing': string
  'hub.pv.importDone': string
  'hub.pv.importSkipped': string
  'hub.pv.importNone': string
  'hub.pv.importFailed': string
  'hub.pv.keyMask': string
  'hub.pv.empty': string
  'hub.pv.emptyHint': string
  'hub.pv.source.imported': string
  'hub.pv.source.manual': string
  'hub.pv.style.openai': string
  'hub.pv.style.anthropic': string
  'hub.pv.style.gemini': string
  'hub.pv.style.custom': string
  'hub.pv.models': string
  'hub.pv.apply': string
  'hub.pv.edit': string
  'hub.pv.delete': string
  'hub.pv.deleteConfirm': string

  // ==================== 供应商 · claude 只读视图（票据 05） ====================
  'hub.pv.claude.title': string
  'hub.pv.claude.none': string
  'hub.pv.claude.bridge': string
  'hub.pv.claude.baseUrl': string
  'hub.pv.claude.model': string
  'hub.pv.claude.token': string

  // ==================== 供应商 · 编辑器（票据 05） ====================
  'hub.pv.editor.titleNew': string
  'hub.pv.editor.titleEdit': string
  'hub.pv.editor.template': string
  'hub.pv.editor.name': string
  'hub.pv.editor.baseUrl': string
  'hub.pv.editor.apiStyle': string
  'hub.pv.editor.models': string
  'hub.pv.editor.nameExists': string
  'hub.pv.editor.save': string
  'hub.pv.editor.cancel': string

  // ==================== 供应商 · 应用（票据 05） ====================
  'hub.pv.apply.title': string
  'hub.pv.apply.target': string
  'hub.pv.apply.targetName': string
  'hub.pv.apply.targetNameHint': string
  'hub.pv.apply.codexUnsupported': string
  'hub.pv.apply.keyMode': string
  'hub.pv.apply.keyInline': string
  'hub.pv.apply.keySource': string
  'hub.pv.apply.keySourceMask': string
  'hub.pv.apply.keyNone': string
  'hub.pv.apply.write': string
  'hub.pv.apply.writing': string
  'hub.pv.apply.restartHint': string
  'hub.pv.apply.conflict': string
  'hub.pv.apply.conflictConfirm': string
  'hub.pv.apply.failed': string
}
