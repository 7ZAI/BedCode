/**
 * Agent Hub 插件 dev-shell 领域种子数据（SDK PluginDevMock 协议）
 *
 * 仅 dev-shell 浏览器演示消费（loader 按 pluginId 注册，mock 命令实现
 * 消费种子返回演示值）；真实宿主忽略该导出，无需条件编译。
 *
 * 种子全部为 wire 形状（与 guest emit 载荷同构，见 ./types.ts），五个子域
 * 对应 guest 五个领域模块：
 * - detection：探测终态（3 装 1 未装 + codex 双安装演示）
 * - install：测速/npmrc/更新检查终态 + 各家安装剧本（运行输出模拟）
 * - skills：规范库 3 条目覆盖 distributed/stale/none 三种分发状态
 * - providers：预设 + claude 只读视图 + 桥接冲突演示素材
 * - usage：看板聚合 + 会话列表 + 首条会话的日志详情（事件流 + 原始 JSONL）
 */
import type { AgentHubDevMock } from './devMockTypes'

const NOW = Date.now()
const DAY = 86_400_000
const HOME = '/home/dev'

/** 近 n 天 YYYY-MM-DD（由远及近） */
function dayStr(daysAgo: number): string {
  return new Date(NOW - daysAgo * DAY).toISOString().slice(0, 10)
}

const devMock: AgentHubDevMock = {
  // ==================== 探测域（概览/安装页共用） ====================
  detection: {
    authGranted: true,
    envStatus: 'ok',
    envError: null,
    env: {
      os: 'linux',
      node: 'v22.14.0',
      npm: '10.9.2',
      pnpm: '12.2.1',
      registry: 'https://registry.npmjs.org',
    },
    clis: {
      claude: {
        installed: true,
        version: '2.0.21',
        method: 'npm-global',
        paths: [`${HOME}/.nvm/versions/node/v22.14.0/bin/claude`],
        dual: false,
        status: 'ok',
        error: null,
      },
      codex: {
        installed: true,
        version: '0.42.0',
        method: 'npm-global',
        paths: [
          `${HOME}/.nvm/versions/node/v22.14.0/bin/codex`,
          '/usr/local/bin/codex',
        ],
        dual: true,
        status: 'ok',
        error: null,
      },
      opencode: {
        installed: true,
        version: '0.11.3',
        method: 'standalone',
        paths: [`${HOME}/.opencode/bin/opencode`],
        dual: false,
        status: 'ok',
        error: null,
      },
      pi: {
        installed: false,
        version: null,
        method: 'unknown',
        paths: [],
        dual: false,
        status: 'not-installed',
        error: null,
      },
    },
  },

  // ==================== 安装域（测速/npmrc/更新/安装剧本） ====================
  install: {
    active: null,
    last: {
      cli: 'opencode',
      action: 'update',
      command: 'npm install -g opencode-ai',
      ok: true,
      cancelled: false,
      exitCode: 0,
      timedOut: false,
      error: null,
      output: '$ npm install -g opencode-ai\n\nadded 1 package in 14s',
      finishedAt: NOW - 3_600_000,
    },
    updates: {
      claude: { latest: '2.1.0', outdated: true, checkedAt: NOW - 1_800_000, error: null },
      codex: { latest: '0.42.0', outdated: false, checkedAt: NOW - 1_800_000, error: null },
      opencode: { latest: '0.12.0', outdated: true, checkedAt: NOW - 1_800_000, error: null },
      pi: null,
    },
    mirror: {
      speed: {
        status: 'ok',
        sources: [
          { id: 'npmmirror', url: 'https://registry.npmmirror.com', ms: 42, reachable: true },
          { id: 'huawei', url: 'https://mirrors.huaweicloud.com/repository/npm/', ms: 120, reachable: true },
          { id: 'tencent', url: 'https://mirrors.cloud.tencent.com/npm/', ms: 210, reachable: true },
          { id: 'npmjs', url: 'https://registry.npmjs.org', ms: 386, reachable: true },
          { id: 'yarn', url: 'https://registry.yarnpkg.com', ms: 402, reachable: true },
        ],
        recommend: 'npmmirror',
        error: null,
        testedAt: NOW - 7_200_000,
      },
      npmrc: {
        backupExists: false,
        fileRegistry: 'https://registry.npmjs.org',
        appliedAt: null,
        restoredAt: null,
      },
      customSources: [],
    },
    runScripts: {
      claude: {
        command: 'npm install -g @anthropic-ai/claude-code',
        output: [
          '$ npm install -g @anthropic-ai/claude-code',
          '',
          'added 1 package in 21s',
          '',
          'claude 2.1.0 installed successfully',
        ],
      },
      codex: {
        command: 'npm install -g @openai/codex',
        output: ['$ npm install -g @openai/codex', '', 'added 1 package in 17s'],
      },
      opencode: {
        command: 'npm install -g opencode-ai',
        output: ['$ npm install -g opencode-ai', '', 'added 1 package in 14s'],
      },
      pi: {
        command: 'npm install -g @earendil-works/pi-coding-agent',
        output: [
          '$ npm install -g @earendil-works/pi-coding-agent',
          '',
          'added 1 package in 19s',
        ],
      },
    },
  },

  // ==================== Skills 域（规范库 + 分发状态三态） ====================
  skills: {
    status: 'ready',
    error: null,
    scannedAt: NOW - 300_000,
    libraryRoot: `${HOME}/.agents/skills`,
    importing: false,
    skills: [
      {
        dir: 'code-review',
        path: `${HOME}/.agents/skills/code-review`,
        name: 'Code Review',
        description: '按规范与 spec 两轴审查本次改动，输出分级意见',
        allowedTools: null,
        files: [
          { path: 'SKILL.md', hash: 'a1b2c3d4' },
          { path: 'checklist.md', hash: 'b2c3d4e5' },
        ],
        hash: 'c3d4e5f6',
        error: null,
        distribution: {
          claude: { status: 'distributed', missingFiles: [], staleFiles: [] },
          pi: { status: 'distributed', missingFiles: [], staleFiles: [] },
        },
      },
      {
        dir: 'commit-message',
        path: `${HOME}/.agents/skills/commit-message`,
        name: 'Commit Message',
        description: '按 conventional commits 规范生成提交信息',
        allowedTools: null,
        files: [
          { path: 'SKILL.md', hash: 'd4e5f6a7' },
          { path: 'reference.md', hash: 'e5f6a7b8' },
        ],
        hash: 'f6a7b8c9',
        error: null,
        distribution: {
          claude: { status: 'stale', missingFiles: ['reference.md'], staleFiles: ['SKILL.md'] },
          pi: { status: 'none', missingFiles: [], staleFiles: [] },
        },
      },
      {
        dir: 'tdd-driver',
        path: `${HOME}/.agents/skills/tdd-driver`,
        name: 'TDD Driver',
        description: '红-绿-重构节奏驱动实现，测试先行',
        allowedTools: null,
        files: [{ path: 'SKILL.md', hash: 'a7b8c9d0' }],
        hash: 'b8c9d0e1',
        error: null,
        distribution: {
          claude: { status: 'none', missingFiles: [], staleFiles: [] },
          pi: { status: 'none', missingFiles: [], staleFiles: [] },
        },
      },
    ],
    targets: {
      claude: { root: `${HOME}/.claude/skills`, exists: true },
      pi: { root: `${HOME}/.pi/agent/skills`, exists: true },
    },
    github: { last: null },
    import: { last: null },
    skillContents: {
      'code-review': `---
name: code-review
description: 按规范与 spec 两轴审查本次改动，输出分级意见
---

# Code Review

对当前分支的改动做双轴审查：

1. **Standards 轴**：对照仓库编码规范（命名、错误处理、日志红线）
2. **Spec 轴**：对照需求文档逐条核对交付范围

输出使用 [P0]-[P3] 分级，P0/P1 必须修复后才能合并。
`,
      'commit-message': `---
name: commit-message
description: 按 conventional commits 规范生成提交信息
---

# Commit Message

- 格式：<type>(<scope>): <subject>
- type 限定 feat/fix/docs/refactor/chore/test/perf
- 禁止 AI 协作者标记
`,
      'tdd-driver': `---
name: tdd-driver
description: 红-绿-重构节奏驱动实现，测试先行
---

# TDD Driver

1. 红：先写失败的测试
2. 绿：最小实现让测试通过
3. 重构：在测试保护下整理代码
`,
      'demo-skill': `---
name: demo-skill
description: 本地导入演示技能（dev-shell 种子）
---

# Demo Skill

这是 dev-shell 本地导入流程的演示技能内容。
`,
    },
    localImport: {
      path: `${HOME}/Downloads/demo-skill`,
      name: 'demo-skill',
    },
  },

  // ==================== 供应商域（预设 + claude 只读视图） ====================
  providers: {
    presets: [
      {
        id: 1,
        name: 'DeepSeek 官方',
        baseUrl: 'https://api.deepseek.com',
        apiStyle: 'openai',
        models: ['deepseek-chat', 'deepseek-reasoner'],
        keyMask: 'sk-…(36)',
        notes: null,
        createdAt: NOW - 20 * DAY,
        updatedAt: NOW - 5 * DAY,
      },
      {
        id: 2,
        name: '月之暗面 Kimi',
        baseUrl: 'https://api.moonshot.cn/anthropic',
        apiStyle: 'anthropic',
        models: ['kimi-k2-0905-preview'],
        keyMask: '—',
        notes: 'pi:kimi',
        createdAt: NOW - 12 * DAY,
        updatedAt: NOW - 2 * DAY,
      },
      {
        id: 3,
        name: '智谱 GLM',
        baseUrl: 'https://open.bigmodel.cn/api/paas/v4',
        apiStyle: 'openai',
        models: ['glm-4.6', 'glm-4.5-air'],
        keyMask: 'sk-…(28)',
        notes: 'opencode:glm',
        createdAt: NOW - 6 * DAY,
        updatedAt: NOW - 6 * DAY,
      },
    ],
    claude: {
      env: {
        baseUrl: 'https://api.moonshot.cn/anthropic',
        model: 'kimi-k2-0905-preview',
        authTokenMask: 'sk-9f***3ab',
      },
      bridge: {
        providerConfigSh: true,
        anthropicBridgeMjs: true,
      },
    },
    import: {
      last: {
        ok: true,
        created: ['智谱 GLM'],
        skipped: ['DeepSeek 官方'],
        keys: { '智谱 GLM': 'sk-****7c1d' },
        error: null,
        at: NOW - DAY,
      },
    },
    apply: {
      last: {
        ok: true,
        preset: '月之暗面 Kimi',
        target: 'claude',
        files: [`${HOME}/.claude/settings.json`, 'provider-config.sh', 'anthropic-bridge.mjs'],
        keyMode: 'source',
        keyLen: 52,
        error: null,
        at: NOW - 3_600_000,
      },
    },
    importDiscoveries: [
      {
        name: 'DeepSeek 官方',
        baseUrl: 'https://api.deepseek.com',
        apiStyle: 'openai',
        models: ['deepseek-chat'],
        notes: 'pi:deepseek',
      },
      {
        name: 'SiliconFlow 硅基流动',
        baseUrl: 'https://api.siliconflow.cn/v1',
        apiStyle: 'openai',
        models: ['Qwen/Qwen3-235B-A22B', 'deepseek-ai/DeepSeek-V3.1'],
        notes: 'opencode:siliconflow',
      },
    ],
    importKeys: {
      'DeepSeek 官方': 'sk-****a2e9',
      'SiliconFlow 硅基流动': 'sk-****d80c',
    },
    applyFiles: {
      claude: [`${HOME}/.claude/settings.json`, 'provider-config.sh', 'anthropic-bridge.mjs'],
      pi: [`${HOME}/.pi/agent/config.json`],
      opencode: [`${HOME}/.config/opencode/opencode.json`],
    },
    applyBridges: ['provider-config.sh', 'anthropic-bridge.mjs'],
  },

  // ==================== 使用统计域（看板 + 会话 + 日志详情） ====================
  usage: {
    state: {
      status: 'ok',
      error: null,
      syncedAt: NOW - 300_000,
      authGranted: true,
      home: HOME,
      adapters: {
        claude: { files: 142, parsed: 137, skipped: 5, sessions: 48, error: null },
        pi: { files: 36, parsed: 36, skipped: 0, sessions: 21, error: null },
        opencode: { files: 12, parsed: 9, skipped: 3, sessions: 4, error: null },
      },
      sources: [
        { name: 'claude', path: `${HOME}/.claude/projects`, builtin: true },
        { name: 'pi', path: `${HOME}/.pi/agent/sessions`, builtin: true },
        { name: 'opencode', path: `${HOME}/.opencode/sessions`, builtin: false },
      ],
    },
    stats: {
      total: {
        sessions: 8,
        tokens_in: 1_284_000,
        tokens_out: 96_400,
        tokens_cache_read: 5_620_000,
        tokens_cache_write: 218_000,
        tokens_reasoning: 41_200,
        duration_ms: 10_620_000,
        cost_total: 12.86,
      },
      byDay: [
        { day: dayStr(6), sessions: 1, tokens_in: 82_000, tokens_out: 6_400 },
        { day: dayStr(5), sessions: 2, tokens_in: 214_000, tokens_out: 15_800 },
        { day: dayStr(4), sessions: 0, tokens_in: 0, tokens_out: 0 },
        { day: dayStr(3), sessions: 1, tokens_in: 158_000, tokens_out: 12_300 },
        { day: dayStr(2), sessions: 2, tokens_in: 366_000, tokens_out: 28_700 },
        { day: dayStr(1), sessions: 1, tokens_in: 201_000, tokens_out: 17_200 },
        { day: dayStr(0), sessions: 1, tokens_in: 263_000, tokens_out: 16_000 },
      ],
      byCli: [
        {
          adapter: 'claude',
          sessions: 6,
          duration_ms: 8_460_000,
          tokens_in: 1_038_000,
          tokens_out: 78_200,
          tokens_cache_read: 4_910_000,
          tokens_cache_write: 186_000,
          cost_total: 11.02,
        },
        {
          adapter: 'pi',
          sessions: 2,
          duration_ms: 2_160_000,
          tokens_in: 246_000,
          tokens_out: 18_200,
          tokens_cache_read: 710_000,
          tokens_cache_write: 32_000,
          cost_total: 1.84,
        },
      ],
      byProject: [
        {
          project: `${HOME}/project/tauriProject/BedCode`,
          sessions: 5,
          duration_ms: 7_320_000,
          tokens_in: 902_000,
          tokens_out: 68_400,
          cost_total: 9.41,
        },
        {
          project: `${HOME}/work/bedcode-mobile`,
          sessions: 2,
          duration_ms: 2_400_000,
          tokens_in: 296_000,
          tokens_out: 21_600,
          cost_total: 2.66,
        },
        {
          project: null,
          sessions: 1,
          duration_ms: 900_000,
          tokens_in: 86_000,
          tokens_out: 6_400,
          cost_total: 0.79,
        },
      ],
      byModel: [
        { model: 'claude-sonnet-4-5', sessions: 5, messages: 132, tokens_in: 862_000, tokens_out: 64_800 },
        { model: 'claude-opus-4-1', sessions: 1, messages: 28, tokens_in: 176_000, tokens_out: 13_400 },
        { model: 'glm-4.6', sessions: 1, messages: 41, tokens_in: 152_000, tokens_out: 11_200 },
        { model: 'kimi-k2-0905-preview', sessions: 1, messages: 19, tokens_in: 94_000, tokens_out: 7_000 },
      ],
    },
    sessions: [
      {
        id: 1,
        adapter: 'claude',
        cli_session_id: 'f2a8c1e0-4b7d-4e6a-9c3f-8d12b5a7e901',
        project: `${HOME}/project/tauriProject/BedCode`,
        title: 'agent-hub 使用统计与会话日志收尾',
        started_at: NOW - 5_400_000,
        ended_at: NOW - 4_020_000,
        duration_ms: 1_380_000,
        model: 'claude-sonnet-4-5',
        tokens_in: 284_000,
        tokens_out: 21_400,
        tokens_cache_read: 1_262_000,
        tokens_cache_write: 46_000,
        tokens_reasoning: 8_900,
        cost_total: 3.12,
      },
      {
        id: 2,
        adapter: 'pi',
        cli_session_id: 'pi-20260912-a3f1',
        project: `${HOME}/project/tauriProject/BedCode`,
        title: '供应商预设反向导入对齐',
        started_at: NOW - 9_000_000,
        ended_at: NOW - 7_800_000,
        duration_ms: 1_200_000,
        model: 'glm-4.6',
        tokens_in: 152_000,
        tokens_out: 11_200,
        tokens_cache_read: 402_000,
        tokens_cache_write: 18_000,
        tokens_reasoning: 4_100,
        cost_total: 1.04,
      },
      {
        id: 3,
        adapter: 'claude',
        cli_session_id: 'b7e2d4f8-1a9c-4f3b-8e5d-6c0a9b2d7f34',
        project: `${HOME}/work/bedcode-mobile`,
        title: '终端输入栏贴合间距修复',
        started_at: NOW - 129_600_000,
        ended_at: NOW - 128_520_000,
        duration_ms: 1_080_000,
        model: 'claude-sonnet-4-5',
        tokens_in: 176_000,
        tokens_out: 13_100,
        tokens_cache_read: 806_000,
        tokens_cache_write: 31_000,
        tokens_reasoning: 6_200,
        cost_total: 1.87,
      },
      {
        id: 4,
        adapter: 'claude',
        cli_session_id: 'c9d3e5a7-2b8f-4c1e-9a6b-0d4f8e3c5a12',
        project: `${HOME}/work/bedcode-mobile`,
        title: 'terminal_link 静默吞错点补日志',
        started_at: NOW - 138_600_000,
        ended_at: NOW - 137_700_000,
        duration_ms: 900_000,
        model: 'claude-opus-4-1',
        tokens_in: 121_000,
        tokens_out: 9_800,
        tokens_cache_read: 604_000,
        tokens_cache_write: 24_000,
        tokens_reasoning: 5_400,
        cost_total: 1.42,
      },
      {
        id: 5,
        adapter: 'pi',
        cli_session_id: 'pi-20260910-e5b7',
        project: null,
        title: null,
        started_at: NOW - 172_800_000,
        ended_at: NOW - 171_900_000,
        duration_ms: 900_000,
        model: 'kimi-k2-0905-preview',
        tokens_in: 94_000,
        tokens_out: 7_000,
        tokens_cache_read: 308_000,
        tokens_cache_write: 14_000,
        tokens_reasoning: 2_800,
        cost_total: 0.8,
      },
      {
        id: 6,
        adapter: 'claude',
        cli_session_id: 'a1b7c9e3-5d2f-4a8b-b6c0-3e7f9a1d5c28',
        project: `${HOME}/project/tauriProject/BedCode`,
        title: '插件并发闸门脉冲失败补 warn 日志',
        started_at: NOW - 216_000_000,
        ended_at: NOW - 215_100_000,
        duration_ms: 900_000,
        model: 'claude-sonnet-4-5',
        tokens_in: 88_000,
        tokens_out: 6_900,
        tokens_cache_read: 412_000,
        tokens_cache_write: 16_000,
        tokens_reasoning: 3_100,
        cost_total: 0.94,
      },
      {
        id: 7,
        adapter: 'claude',
        cli_session_id: 'e3f5a7c9-7e1b-4d3a-8f5c-9b2d6e4a1f06',
        project: `${HOME}/project/tauriProject/BedCode`,
        title: '供应商统一管理预设 CRUD',
        started_at: NOW - 432_000_000,
        ended_at: NOW - 430_560_000,
        duration_ms: 1_440_000,
        model: 'claude-sonnet-4-5',
        tokens_in: 262_000,
        tokens_out: 19_800,
        tokens_cache_read: 1_114_000,
        tokens_cache_write: 42_000,
        tokens_reasoning: 7_600,
        cost_total: 2.79,
      },
      {
        id: 8,
        adapter: 'claude',
        cli_session_id: 'd6b8f0a2-8c3e-4b7d-9e1a-5c8b2f6d0e43',
        project: `${HOME}/project/tauriProject/BedCode`,
        title: 'Skills 规范库扫描与分发比对',
        started_at: NOW - 518_400_000,
        ended_at: NOW - 517_140_000,
        duration_ms: 1_260_000,
        model: 'claude-sonnet-4-5',
        tokens_in: 102_000,
        tokens_out: 7_200,
        tokens_cache_read: 512_000,
        tokens_cache_write: 21_000,
        tokens_reasoning: 3_400,
        cost_total: 1.06,
      },
      {
        id: 9,
        adapter: 'opencode',
        cli_session_id: 'occ-20260910-77b2',
        project: '/tmp/demo-scratch',
        title: 'opencode 日志目录演示（自定义来源）',
        started_at: NOW - 604_800_000,
        ended_at: NOW - 603_400_000,
        duration_ms: 1_400_000,
        model: 'deepseek-v3.2',
        tokens_in: 96_000,
        tokens_out: 7_400,
        tokens_cache_read: 0,
        tokens_cache_write: 0,
        tokens_reasoning: 0,
        cost_total: 0.62,
      },
    ],
    sessionDetails: {
      1: {
        session: {
          id: 1,
          adapter: 'claude',
          cli_session_id: 'f2a8c1e0-4b7d-4e6a-9c3f-8d12b5a7e901',
          project: `${HOME}/project/tauriProject/BedCode`,
          title: 'agent-hub 使用统计与会话日志收尾',
          started_at: NOW - 5_400_000,
          ended_at: NOW - 4_020_000,
          duration_ms: 1_380_000,
          model: 'claude-sonnet-4-5',
          tokens_in: 284_000,
          tokens_out: 21_400,
          tokens_cache_read: 1_262_000,
          tokens_cache_write: 46_000,
          tokens_reasoning: 8_900,
          cost_total: 3.12,
          source_path: `${HOME}/.claude/projects/tauriProject-BedCode/f2a8c1e0-4b7d-4e6a-9c3f-8d12b5a7e901.jsonl`,
        },
        events: [
          {
            ts: NOW - 5_400_000,
            role: 'user',
            text: 'agent-hub 的使用统计 tab 里，「加载更多」按钮在只有一页数据时也渲染出来了，看下分页边界。',
            model: null,
            tokens: null,
          },
          {
            ts: NOW - 5_394_000,
            role: 'assistant',
            text: '我先看 useUsage 的分页游标与 sessionsTotal 的更新路径，再对照 StatsTab 的 hasMore 判定。',
            model: 'claude-sonnet-4-5',
            tokens: { input: 18_400, output: 1_260, cacheRead: 96_000, cacheWrite: 4_200, reasoning: 620 },
          },
          {
            ts: NOW - 5_392_000,
            role: 'tool',
            text: 'Read bedcode-desktop/plugins/agent-hub/src/composables/useUsage.ts (L112-130)',
            model: null,
            tokens: null,
          },
          {
            ts: NOW - 5_388_000,
            role: 'assistant',
            text: '问题在 loadMoreSessions：空页返回后 loadedOffset 与 sessionsTotal 相等，但初始渲染时 sessions.length === total 也会展示按钮。修复：hasMore 改为比较 sessionsTotal 与已加载数。',
            model: 'claude-sonnet-4-5',
            tokens: { input: 22_100, output: 1_840, cacheRead: 128_000, cacheWrite: 5_600, reasoning: 940 },
          },
          {
            ts: NOW - 5_380_000,
            role: 'user',
            text: '好，顺手把空态文案的 i18n key 也补齐 en。',
            model: null,
            tokens: null,
          },
          {
            ts: NOW - 5_378_000,
            role: 'assistant',
            text: '已同步 zh-CN 与 en 两份文案，vitest 通过，8 个用例全绿。',
            model: 'claude-sonnet-4-5',
            tokens: { input: 19_800, output: 1_120, cacheRead: 112_000, cacheWrite: 3_800, reasoning: 480 },
          },
        ],
        eventsTruncated: false,
        raw: [
          '{"type":"user","message":{"role":"user","content":"agent-hub 的使用统计 tab 里，「加载更多」按钮在只有一页数据时也渲染出来了，看下分页边界。"},"timestamp":"' + new Date(NOW - 5_400_000).toISOString() + '"}',
          '{"type":"assistant","message":{"model":"claude-sonnet-4-5","content":[{"type":"text","text":"我先看 useUsage 的分页游标与 sessionsTotal 的更新路径，再对照 StatsTab 的 hasMore 判定。"}],"usage":{"input_tokens":18400,"output_tokens":1260,"cache_read_input_tokens":96000,"cache_creation_input_tokens":4200}},"timestamp":"' + new Date(NOW - 5_394_000).toISOString() + '"}',
          '{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"...useUsage.ts L112-130..."}]},"timestamp":"' + new Date(NOW - 5_392_000).toISOString() + '"}',
          '{"type":"assistant","message":{"model":"claude-sonnet-4-5","content":[{"type":"text","text":"问题在 loadMoreSessions：空页返回后 loadedOffset 与 sessionsTotal 相等。"}],"usage":{"input_tokens":22100,"output_tokens":1840,"cache_read_input_tokens":128000}},"timestamp":"' + new Date(NOW - 5_388_000).toISOString() + '"}',
        ],
        rawTruncated: false,
        skippedLines: 0,
      },
    },
  },
}

export default devMock
