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

  // ==================== 概览 · 环境条 ====================
  'hub.env.title': string
  'hub.env.os': string
  'hub.env.node': string
  'hub.env.npm': string
  'hub.env.pnpm': string
  'hub.env.registry': string
  'hub.env.detect': string
  'hub.env.detecting': string
  'hub.env.none': string

  // ==================== 概览 · CLI 卡片 ====================
  'hub.card.installed': string
  // 已装但未初始化（票 07：CLI 装了却零会话数据，如本机 codex）
  'hub.card.installedNoSessions': string
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
  // ==================== 概览 · CLI 卸载（概览卡片动作） ====================
  'hub.card.uninstall': string
  'hub.card.uninstallConfirm': string
  'hub.card.uninstallRunning': string
  'hub.card.uninstallHintDual': string
  'hub.card.uninstallHintMethod': string
  'hub.card.uninstallHintNode': string
  'hub.card.uninstallFailed': string

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
  'hub.speed.recommended': string
  'hub.speed.currentTag': string
  'hub.speed.select': string
  'hub.speed.addCustom': string
  'hub.speed.addCustomPlaceholder': string
  'hub.speed.removeCustom': string
  'hub.speed.source.npmmirror': string
  'hub.speed.source.npmjs': string
  'hub.speed.source.huawei': string
  'hub.speed.source.tencent': string
  'hub.speed.source.yarn': string
  'hub.speed.source.custom': string
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
  // ADR 0030 业务错误码（guest bail_with_code!/user_facing_string，完整 key =
  // com.bedcode.agent-hub.hub.inst.error.*）：用户可见拒绝的原因文案
  'hub.inst.error.busy': string
  'hub.inst.error.detectionPending': string
  'hub.inst.error.nodeMissing': string
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
  'hub.pv.editor.close': string
  'hub.pv.editor.required': string
  'hub.pv.editor.template': string
  'hub.pv.editor.name': string
  'hub.pv.editor.baseUrl': string
  'hub.pv.editor.apiStyle': string
  'hub.pv.editor.models': string
  'hub.pv.editor.nameExists': string
  'hub.pv.editor.save': string
  'hub.pv.editor.cancel': string
  'hub.pv.editor.key': string
  'hub.pv.editor.keyNew': string
  'hub.pv.editor.keyPlaceholder': string
  'hub.pv.editor.keyHint': string
  'hub.pv.editor.keyClear': string
  'hub.pv.editor.keyKeep': string

  // ==================== 供应商 · 应用（票据 05） ====================
  'hub.pv.apply.title': string
  'hub.pv.apply.target': string
  'hub.pv.apply.targetName': string
  'hub.pv.apply.targetNameHint': string
  'hub.pv.apply.codexUnsupported': string
  'hub.pv.apply.keyMode': string
  'hub.pv.apply.keyStored': string
  'hub.pv.apply.keyStoredHint': string
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
  // ==================== 使用统计看板（票据 06 + 改版） ====================
  'hub.st.syncedTag': string
  'hub.st.syncing': string
  'hub.st.scanning': string
  'hub.st.scanNow': string
  // ---------- 时间窗 ----------
  'hub.st.range': string
  'hub.st.range.0': string
  'hub.st.range.7': string
  'hub.st.range.30': string
  'hub.st.range.90': string
  // ---------- KPI 卡 ----------
  'hub.st.kpi.sessions': string
  'hub.st.kpi.sessionsSub': string
  'hub.st.kpi.tokens': string
  'hub.st.kpi.tokensSub': string
  'hub.st.kpi.cache': string
  'hub.st.kpi.cacheSub': string
  'hub.st.kpi.duration': string
  'hub.st.kpi.durationSub': string
  'hub.st.kpi.cost': string
  'hub.st.kpi.costSub': string
  'hub.st.kpi.costNone': string
  'hub.st.kpi.coverage': string
  'hub.st.kpi.coverageVal': string
  'hub.st.kpi.coverageSub': string
  'hub.st.kpi.range': string
  'hub.st.kpi.noRange': string
  // ---------- 指标（趋势 / 排行 / 占比共用） ----------
  'hub.st.metricLabel': string
  'hub.st.metric.tokens': string
  'hub.st.metric.tokens_in': string
  'hub.st.metric.tokens_out': string
  'hub.st.metric.tokens_cache_read': string
  'hub.st.metric.tokens_cache_write': string
  'hub.st.metric.tokens_reasoning': string
  'hub.st.metric.sessions': string
  'hub.st.metric.cost_total': string
  'hub.st.metric.duration_ms': string
  // ---------- 趋势图 ----------
  'hub.st.trend.title': string
  'hub.st.trend.mode.total': string
  'hub.st.trend.mode.stack': string
  'hub.st.trend.table': string
  'hub.st.trend.chart': string
  'hub.st.trend.empty': string
  'hub.st.trend.col.day': string
  // ---------- 节奏热力图（7×24） ----------
  'hub.st.heat.title': string
  'hub.st.heat.sub': string
  'hub.st.heat.empty': string
  'hub.st.heat.aria': string
  'hub.st.heat.peak': string
  'hub.st.heat.peakShort': string
  'hub.st.heat.noPeak': string
  'hub.st.heat.rowTotal': string
  'hub.st.heat.legendLow': string
  'hub.st.heat.dow.0': string
  'hub.st.heat.dow.1': string
  'hub.st.heat.dow.2': string
  'hub.st.heat.dow.3': string
  'hub.st.heat.dow.4': string
  'hub.st.heat.dow.5': string
  'hub.st.heat.dow.6': string
  // ---------- CLI 占比环 ----------
  'hub.st.donut.title': string
  'hub.st.donut.sub': string
  'hub.st.donut.empty': string
  // ---------- 排行 ----------
  'hub.st.bars.projects': string
  'hub.st.bars.models': string
  'hub.st.bars.empty': string
  'hub.st.bars.more': string
  'hub.st.modelMessages': string
  'hub.st.noProject': string
  'hub.st.empty': string
  'hub.st.emptyWindow': string
  // 数据清空（票 07：全量保留 + 手动清空）
  'hub.st.clearData': string
  'hub.st.clearDataAsk': string
  'hub.st.clearDataConfirm': string
  'hub.st.clearDataCancel': string
  'hub.st.clearDataDone': string
  'hub.st.clearDataFailed': string
  // 适配器降级（票 07：opencode SQLite 源的机器可读 code → 界面文案）
  'hub.st.degraded': string
  'hub.st.degraded.sqlite3-missing': string
  'hub.st.degraded.db-missing': string
  'hub.st.degraded.query-failed': string

  // ==================== 会话日志（改版：来源 / 查询 / 分页 / 详情） ====================
  'hub.lg.sources.title': string
  'hub.lg.sources.builtin': string
  'hub.lg.sources.custom': string
  'hub.lg.sources.scan': string
  'hub.lg.sources.add': string
  'hub.lg.sources.addDir': string
  'hub.lg.sources.builtinPath': string
  'hub.lg.sources.addName': string
  'hub.lg.sources.nameCustom': string
  'hub.lg.sources.namePick': string
  'hub.lg.sources.pathDuplicate': string
  'hub.lg.sources.addPath': string
  'hub.lg.sources.pick': string
  'hub.lg.sources.picking': string
  'hub.lg.sources.pickFailed': string
  'hub.lg.sources.confirm': string
  'hub.lg.sources.cancel': string
  'hub.lg.sources.removeSource': string
  'hub.lg.sources.removePath': string
  'hub.lg.sources.noScan': string
  'hub.lg.sources.sessions': string
  'hub.lg.sources.files': string
  'hub.lg.sources.kind.jsonl': string
  'hub.lg.sources.kind.sqlite': string
  'hub.lg.sources.addFailed': string
  // ADR 0030 业务错误码（guest bail_with_code!，完整 key =
  // com.bedcode.agent-hub.hub.lg.sources.error.*）：来源/目录增删被拒的原因文案
  'hub.lg.sources.error.nameTaken': string
  'hub.lg.sources.error.pathTaken': string
  'hub.lg.sources.error.sqliteReadonly': string
  'hub.lg.sources.error.builtinProtected': string
  'hub.lg.sources.error.lastPathProtected': string
  'hub.lg.sources.removeFailed': string
  'hub.lg.sources.addPathFailed': string
  'hub.lg.sources.removePathFailed': string
  'hub.lg.sources.scanTimeout': string
  'hub.lg.sources.scanUnreadable': string
  'hub.lg.sources.scanInterrupted': string
  'hub.lg.sources.scanFailed': string
  'hub.lg.filter.agent': string
  'hub.lg.filter.all': string
  'hub.lg.filter.keyword': string
  'hub.lg.filter.keywordPh': string
  'hub.lg.filter.from': string
  'hub.lg.filter.to': string
  'hub.lg.filter.fromPh': string
  'hub.lg.filter.toPh': string
  'hub.lg.filter.dpSelect': string
  'hub.lg.filter.dpCancel': string
  'hub.lg.filter.dpNow': string
  'hub.lg.filter.query': string
  'hub.lg.filter.reset': string
  'hub.lg.col.agent': string
  'hub.lg.col.session': string
  'hub.lg.col.project': string
  'hub.lg.col.started': string
  'hub.lg.col.duration': string
  'hub.lg.col.tokens': string
  'hub.lg.row.open': string
  /** 正在使用的项目会话徽标（扫描时按配置/最新会话标记） */
  'hub.lg.row.current': string
  /** 当前行 title 提示 */
  'hub.lg.row.currentTip': string
  'hub.lg.noMatch': string
  'hub.lg.pager.total': string
  'hub.lg.pager.page': string
  'hub.lg.pager.prev': string
  'hub.lg.pager.next': string
  'hub.lg.detail.back': string
  'hub.lg.detail.tabChat': string
  'hub.lg.detail.tabRaw': string
  'hub.lg.noSource': string
  'hub.lg.eventsTruncated': string
  'hub.lg.rawTruncated': string
  'hub.lg.noEvents': string
  'hub.lg.role.user': string
  'hub.lg.role.assistant': string
  'hub.lg.role.tool': string
  'hub.lg.role.system': string
}
