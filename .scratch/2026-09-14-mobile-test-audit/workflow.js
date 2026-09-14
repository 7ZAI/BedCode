// BedCode 移动端单元测试分模块审核 workflow
// 硬件策略：可用内存 4.1GB，每个 pi 子 agent ~1GB RSS
// 采用并发 2、分 3 波（每波 runs.all([2 lane])），波间串行
// 避免 MEMORY.md 记录的并发 2 崩溃风险（当时有 gradle daemon + vite watch 常驻，当前无）

const lanes = [
  {
    key: 'fe-utils-config',
    label: '审核前端基础层（utils/config/plugin/services）',
    files: [
      'src/__tests__/utils/terminalResizePolicy.test.ts',
      'src/__tests__/utils/frontendLogger.test.ts',
      'src/__tests__/utils/terminalRowClip.test.ts',
      'src/__tests__/utils/terminalResizeDebouncer.test.ts',
      'src/__tests__/utils/terminalDimensions.test.ts',
      'src/__tests__/utils/terminalIdle.test.ts',
      'src/__tests__/config/terminalThemes.test.ts',
      'src/__tests__/config/terminalOnboardingSteps.test.ts',
      'src/__tests__/config/agentPresets.test.ts',
      'src/__tests__/plugin/dialogHost.test.ts',
      'src/__tests__/plugin/pluginIcon.test.ts',
      'src/services/linkCrypto.test.ts'
    ],
    notes: '基础工具与配置层。重点：纯函数边界覆盖、日志/加密副作用验证、i18n key 同步性、错误路径断言强度。'
  },
  {
    key: 'fe-stores-views',
    label: '审核前端状态/视图层（stores/components/views）',
    files: [
      'src/__tests__/stores/terminalBuffer.test.ts',
      'src/__tests__/stores/inputAssistant.test.ts',
      'src/__tests__/stores/codeViewer.test.ts',
      'src/__tests__/stores/settings.test.ts',
      'src/__tests__/components/EgressConsentDialog.test.ts',
      'src/__tests__/components/terminalInputBarBlur.test.ts',
      'src/__tests__/views/toolboxViewSync.test.ts',
      'src/__tests__/views/toolboxKeepAlive.test.ts',
      'src/__tests__/views/toolboxDeepChild.test.ts',
      'src/__tests__/views/pluginToggleConvergence.test.ts'
    ],
    notes: 'Pinia store + Vue 组件 + 视图。重点：状态迁移断言、副作用验证（Tauri invoke）、组件交互、快照替代行为断言检测。'
  },
  {
    key: 'fe-composables',
    label: '审核前端 composables（组合式函数）',
    files: [
      'src/__tests__/composables/useFileTree.test.ts',
      'src/__tests__/composables/writeCoalescer.test.ts',
      'src/__tests__/composables/useTuiCompat.test.ts',
      'src/__tests__/composables/useTerminalBuffer.test.ts',
      'src/__tests__/composables/useTerminalScroll.test.ts',
      'src/__tests__/composables/useNotification.test.ts',
      'src/__tests__/composables/useLinkEncryption.test.ts',
      'src/__tests__/composables/presetTaskState.test.ts',
      'src/__tests__/composables/connectionProbe.test.ts',
      'src/__tests__/composables/useViewportPanGuard.test.ts',
      'src/__tests__/composables/useAppStartup.test.ts'
    ],
    notes: '业务逻辑 composable 层，数量最多。重点：mock 边界（只 mock 跨进程/Tauri）、交互验证 vs 结果验证、异步/定时器测试的确定性、防抖/合并逻辑边界。'
  },
  {
    key: 'fe-integration-ft',
    label: '审核前端集成测试 + file-transfer 插件测试',
    files: [
      'src/__tests__/integration/connection-flow.test.ts',
      'src/__tests__/integration/session-flow.test.ts',
      'src/__tests__/integration/plugin-lifecycle-teardown.test.ts',
      'src/__tests__/integration/pairing-flow.test.ts',
      'src/__tests__/integration/terminal-flow.test.ts',
      'src/__tests__/integration/plugin-loader-gating.test.ts',
      'src/__tests__/integration/pluginReactivate.test.ts',
      'src/__tests__/integration/plugin-flow.test.ts',
      'src/__tests__/plugins/file-transfer/useConsent.test.ts',
      'src/__tests__/plugins/file-transfer/useTasks.test.ts',
      'src/__tests__/plugins/file-transfer/usePeerDevices.test.ts',
      'src/__tests__/plugins/file-transfer/useRemoteFs.test.ts',
      'src/__tests__/plugins/file-transfer/useTrustedPeers.test.ts',
      'src/__tests__/plugins/file-transfer/useSettings.test.ts',
      'src/__tests__/plugins/file-transfer/deriveDeviceRows.test.ts'
    ],
    notes: '集成测试 + file-transfer 插件前端。重点：多模块协作断言、生命周期清理、文件传输状态机、对等网络发现、mock 深度是否合理。'
  },
  {
    key: 'plugin-sdk',
    label: '审核 ai-chatbox 插件 + SDK 测试',
    files: [
      'plugins/ai-chatbox/src/__tests__/useAiChat.test.ts',
      'plugins/ai-chatbox/src/__tests__/adapters.test.ts',
      'plugins/ai-chatbox/src/__tests__/useAiConfig.test.ts',
      'plugins/ai-chatbox/src/__tests__/markdown.test.ts',
      'plugins/ai-chatbox/src/__tests__/usePluginConfig.test.ts',
      'plugins/ai-chatbox/src/__tests__/highlight.test.ts',
      'plugins/ai-chatbox/src/__tests__/sse.test.ts',
      'packages/plugin-sdk-mobile/__tests__/vite-plugin.test.ts',
      'packages/plugin-sdk-mobile/__tests__/runtime.test.ts',
      'packages/plugin-sdk-mobile/__tests__/types.test.ts'
    ],
    notes: 'ai-chatbox 插件（SSE/markdown/高亮/adapters）+ SDK 运行时/类型/Vite 插件。重点：SSE 流式边界、markdown 渲染安全、adapters 多供应商契约、SDK 类型契约验证强度。'
  },
  {
    key: 'rust-tests',
    label: '审核 Rust 集成测试 + 主要内联测试',
    files: [
      'src-tauri/tests/ws_protocol_integration.rs',
      'src-tauri/tests/http_auth_flow.rs',
      'src-tauri/tests/http_proxy_flow.rs',
      'src-tauri/tests/build_manifest_smoke.rs',
      'src-tauri/tests/common/mod.rs',
      'src-tauri/src/model/message.rs',
      'src-tauri/src/enums/special_key.rs',
      'src-tauri/src/plugin/saf_io.rs',
      'src-tauri/src/peer_transfer.rs',
      'src-tauri/src/terminal_link.rs',
      'src-tauri/src/egress.rs',
      'src-tauri/src/plugin/saf_path.rs',
      'src-tauri/src/connection/codec.rs',
      'src-tauri/src/plugin/wasm_host.rs',
      'src-tauri/src/plugin/wasm_runtime/component.rs',
      'src-tauri/src/router/registry.rs',
      'src-tauri/src/auth/http.rs',
      'src-tauri/src/peer_receive.rs',
      'src-tauri/src/file_service/saf_tree.rs',
      'src-tauri/src/commands/http_proxy.rs',
      'src-tauri/src/peer_net.rs',
      'src-tauri/src/commands/dev_logs.rs',
      'src-tauri/src/plugin/fs_auth.rs',
      'src-tauri/src/plugin/wasm_runtime.rs',
      'src-tauri/src/plugin/approval.rs',
      'src-tauri/src/plugin/wasm_runtime/host_impl/mdns.rs',
      'src-tauri/src/connection/heartbeat.rs',
      'src-tauri/src/router/router.rs',
      'src-tauri/src/system/info.rs',
      'src-tauri/src/plugin/validation.rs',
      'src-tauri/src/enums/session.rs',
      'src-tauri/src/connection/request.rs',
      'src-tauri/src/auth/manager.rs',
      'src-tauri/src/plugin/loader.rs',
      'src-tauri/src/plugin/manager.rs',
      'src-tauri/src/model/message.rs'
    ],
    notes: 'Rust 集成测试（WS/HTTP 协议流）+ 主要内联 #[test]。重点：错误类型断言（AppError vs 裸字符串）、异步测试确定性、mock server 清理、WASM 沙箱测试边界、权限/安全测试的正反例、幂等性测试。注意磁盘紧张，不要跑全量 cargo test，只静态审查代码。'
  }
];

const rubric = [
  '## 审核标准（对照 unit-test-discipline skill 硬性门禁 G1-G6）',
  '',
  '**G1 行为契约**：每个测试能否追溯到明确的需求/代码分支/契约？列出被测模块的 if/else/try-catch/边界/提前返回，是否有对应测试。',
  '',
  '**G2 正反例覆盖**：每条业务规则是否有正例 + 反例？权限拒绝/非法输入/边界越界是否覆盖？',
  '',
  '**G3 强断言**：每个测试至少一个强断言（返回值精确值/状态变化/异常类型/错误码/副作用/消息）。只断言非空、不抛错、toHaveBeenCalled 算弱断言。',
  '',
  '**G4 反模式检测**：',
  '- 无断言测试（只调用函数无 expect）',
  '- 恒真断言（expect(true).toBe(true) 等）',
  '- 只断言 mock 被调用（toHaveBeenCalled 而非返回值/状态）',
  '- 快照替代行为断言（toMatchSnapshot 过多）',
  '- 复制实现逻辑作为预期（测试里重写被测逻辑）',
  '- 只有 happy path（无反例/边界/异常）',
  '- 为绿改期望（测试期望与实现行为不一致但被改过）',
  '- 共享状态/顺序依赖（测试互相影响、依赖执行顺序）',
  '- 真实网络/真实时间/sleep',
  '- .skip/.only 遗留',
  '',
  '**G5 实际运行**：本次只做静态审查，不要求运行测试。但检查测试代码是否可独立运行（无外部依赖、无顺序耦合）。',
  '',
  '**G6 自检与变异分析**：对每个测试问"如果反转 if 条件/删除副作用/返回 null/抛异常被吞掉，测试会失败吗？"无法杀死变异的测试标记为弱测试。',
  '',
  '## 输出格式（精简 Markdown）',
  '',
  '### 1. 模块概览',
  '- 测试文件数 / 测试用例数 / 总行数',
  '- 覆盖的被测模块清单',
  '',
  '### 2. 问题清单（按严重级别）',
  '每个问题包含：文件:行号 | 严重级别(Blocker/Major/Minor/Nit) | 问题描述 | 违反的门禁(G1-G6) | 建议修复方向',
  '',
  '### 3. 评分卡（每项 0-100 分）',
  '- 需求/行为契约追溯性：',
  '- 正反例覆盖：',
  '- 边界+异常覆盖：',
  '- 断言强度：',
  '- 独立性+确定性：',
  '- 可读性+可维护性：',
  '- **总分**（加权平均，<80 分标记为需重写）：',
  '',
  '### 4. 高风险未覆盖清单',
  '- 该模块有测试但明显缺失的关键场景',
  '- 该模块完全没有测试但应该有的关键路径',
  '',
  '### 5. 改进优先级建议',
  '- P0（必须修）：列出具体文件:行号',
  '- P1（建议修）：列出具体文件:行号',
  '- P2（可选）：列出方向',
  '',
  '## 约束',
  '- 只读审查，不要修改任何文件',
  '- 不要运行 cargo test 或 vitest（磁盘紧张）',
  '- 报告写入 .scratch/mobile-test-audit/<lane-key>.md',
  '- 报告精简，每个问题 2-3 行，不要冗长论述',
  '- 必须引用具体文件:行号作为证据'
].join('\n');

const waves = [
  [lanes[0], lanes[1]],
  [lanes[2], lanes[3]],
  [lanes[4], lanes[5]]
];

const reports = [];

for (let i = 0; i < waves.length; i++) {
  const wave = waves[i];
  const items = [];
  for (let j = 0; j < wave.length; j++) {
    const l = wave[j];
    const task = [
      '# 审核任务：' + l.label,
      '',
      '工作目录: /home/binblink/project/tauriProject/BedCode/bedcode-mobile',
      '',
      '## 审核文件清单',
      '',
      l.files.map(f => '- ' + f).join('\n'),
      '',
      '## 模块特别注意',
      '',
      l.notes,
      '',
      rubric
    ].join('\n');
    items.push({
      key: 'audit-' + l.key,
      agent: 'reviewer',
      model: 'sensenova/sensenova-6.8-flash-lite',
      label: l.label,
      task: task,
      output: '.scratch/mobile-test-audit/' + l.key + '.md'
    });
  }
  const results = await runs.all(items);
  reports.push({
    wave: i + 1,
    results: results.map((r, idx) => ({
      key: wave[idx].key,
      ok: r.ok,
      error: r.error,
      hasOutput: !!r.output
    }))
  });
}

return reports;
