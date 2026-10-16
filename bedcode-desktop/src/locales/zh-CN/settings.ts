export default {
  settings: {
    title: '设置',
    // 票 14：`pairing` 分组整体退役——配对码 / QR 有效期改由 com.bedcode.terminal-session
    // 插件贡献的设置分组承载（`pairing.settings.*`），宿主不再保留该分组文案。
    linkCrypto: {
      title: '链路加密',
      master: '启用链路加密',
      masterDesc: '对局域网传输的报文端到端加密；默认关闭，开启前请先在移动端完成配对',
      encryptHttp: '加密 HTTP 载荷',
      encryptHttpDesc: 'REST 请求/响应体信封加密（终端输入回传等）',
      encryptWsTerminal: '加密终端通道',
      encryptWsTerminalDesc: 'WS 终端帧加解密（PTY 输出流）',
      encryptWsEvent: '加密事件通道',
      encryptWsEventDesc: 'WS 事件帧加解密（同步事件广播）',
      plaintextFallback: '允许明文回退',
      plaintextFallbackDesc: '未协商的老客户端放行明文；关闭后非本机未协商请求一律拒绝',
      fingerprint: '本机指纹',
      fingerprintDesc: '本机身份密钥指纹（SHA-256 前 16 位），请与移动端展示值人工比对',
      saveFailed: '链路加密配置保存失败',
    },
    system: {
      title: '系统设置',
      preventSleep: '防止系统休眠',
      preventSleepDesc: '服务器运行时阻止系统进入休眠状态（允许屏幕熄灭）',
      // 票 14：默认端口随「配对设置」分组退役迁入本组（文案与迁移前逐字一致）
      defaultPort: '默认端口',
      defaultPortDesc: '服务器启动时使用的端口，重启后生效',
    },
    log: {
      title: '日志设置',
      level: '日志级别',
      levelDesc: '切换 runtime 日志的落盘级别，即时生效，无需重启',
      levelDebug: '调试',
      levelInfo: '信息',
      levelWarn: '警告',
      levelError: '错误',
      levelApplied: '日志级别已切换',
      format: '日志格式',
      formatDesc: '文本便于直接阅读；JSON 供脚本按字段过滤分析（重启后生效）',
      formatText: '文本',
      formatJson: 'JSON',
      maxFiles: '保留文件数',
      maxFilesDesc: '按天轮转保留的日志文件数量，0 表示不限制',
      capacityMb: '容量上限（MB）',
      capacityMbDesc: '日志目录总大小上限，超出后自动删除最旧文件；0 表示不限制',
      persist: '持久化配置',
      persistDesc: '格式 / 保留数量 / 容量上限保存在配置文件，重启后生效',
      openDir: '打开日志目录',
      save: '保存配置',
      saved: '日志配置已保存',
      saveFailed: '日志配置保存失败',
    },
    // 授权策略与记录（2026-09-27 授权策略增强 · 票 01）：设置页二级入口「应用授权」
    // 与授权管理页文案。档位措辞与 spec §4.1 逐字一致；resource 分类名对应宿主
    // 读模型的 resource 值（fs / network）在界面上的说法
    authorization: {
      title: '应用授权',
      entry: '应用授权记录',
      entryDesc: '查看每个应用的文件与网络授权记录，管理免询问策略',
      back: '返回设置',
      refresh: '刷新',
      appCount: '{count} 个应用',
      empty: '暂无可管理的应用',
      emptyHint: '安装并启用应用后，这里会列出它们的授权情况',
      recordCount: '{count} 条记录',
      // 授权记录清单（票 02）：展开行里的文件目录记录与逐条管理动作。
      // ops 用「读 / 写 / 读写」与弹窗的「读取 / 写入 / 读写」保持同一口径；
      // source 的 user / always_allow / legacy / user_deny 与宿主记录来源一一对应
      records: {
        toggle: '展开授权记录',
        fsTitle: '文件目录记录',
        // 票 05：网络侧记录（归一化 origin，可带 path 前缀）
        networkTitle: '网络地址记录',
        empty: '该应用还没有文件授权记录',
        // 按资源分区的空态 key（`<resource>Empty`，票 05 起展开面板逐资源渲染）
        fsEmpty: '该应用还没有文件授权记录',
        networkEmpty: '该应用还没有网络授权记录',
        revoke: '取消授权',
        removeDeny: '移除拒绝',
        revoked: '已取消该目录的授权，后续访问会被直接拒绝',
        networkRevoked: '已取消该地址的授权，后续请求会被直接拒绝',
        denyRemoved: '已移除拒绝记录，该目录回到未授权状态',
        effect: {
          allow: '已授权',
          deny: '硬拒绝',
        },
        ops: {
          read: '读',
          write: '写',
          read_write: '读写',
        },
        source: {
          user: '用户确认',
          always_allow: '免询问自动放行',
          legacy: '旧版遗留',
          user_deny: '用户拒绝',
        },
      },
      // 四分区标题与空态（票 07 详情页 / 票 08 设置页共用，spec §9.2）：
      // 用户已授权 / 免询问自动放行（未经确认）/ 内置免询问（第一方）/ 硬拒绝
      sections: {
        userGranted: '用户已授权',
        autoAllowed: '免询问自动放行',
        firstParty: '内置免询问',
        denied: '硬拒绝',
        // 免询问自动放行记录必须带「未经确认」标记并与用户确认记录视觉区分（spec §9.4）
        unconfirmed: '未经确认',
        empty: '暂无授权记录',
        // 内置免询问项的两种形态（第一方清单：home 前缀 / 任意项目的具名段）
        firstPartyHome: '家目录',
        firstPartySegment: '项目目录',
        revokeHint: '撤销后该目录后续访问会被直接拒绝',
        // 该应用没有内置免询问项时的空态（与「有项」区分：空态是事实，不是缺数据）
        firstPartyEmpty: '该应用没有内置免询问项',
        firstPartyRevoked: '已取消该目录的免询问，后续访问会被直接拒绝',
      },
      strategy: {
        always_ask: '总是询问',
        default: '默认',
        always_allow: '始终允许',
      },
      // 策略控件（票 03 起，票 04 放开「始终允许」）：三档都可选；切到「始终允许」
      // 走二次确认，确认文案必须说清该档的语义边界（spec §4.3）——切档**不**一次性
      // 授予全部权限，只有实际访问到的目标才逐步累积进记录（可持续累积，故仍要提示）
      strategyControl: {
        title: '{resource}请求策略',
        saved: '策略已更新，下一次判定立即生效',
        hint: {
          // 「总是询问」跳过全部 allow 记录，所以已授权过的目标也照问——
          // 写成「未记录的目标都会询问」会让用户以为已授权的静默通过（票 06 修正）
          always_ask: '每次访问都询问（已授权过的目标也照问）',
          default: '记录命中即放行，未记录才询问',
          always_allow: '未记录的目标免询问放行，并自动记录（未经确认）',
        },
        confirmTitle: '改为「始终允许」？',
        // 语义边界（spec §4.3）必须出现在确认里：用户以为「一次点开全盘」与
        // 以为「什么都不会发生」都是错的认知
        confirmBody:
          '「{name}」的{resource}请求将不再询问：没记录过的目标会直接放行，并以「未经确认」记入授权记录。切档不会一次性授予全部权限，只有该应用实际访问到的目标才会逐步累积——持续访问会让这份清单持续变长，可随时在下方逐条取消授权。',
        confirmOk: '改为始终允许',
      },
      resource: {
        fs: '文件',
        network: '网络',
      },
    },
    ui: {
      title: '界面设置',
    },
    appearance: {
      theme: '主题',
      palette: '主题色板',
      paletteWarm: '暖调工作台',
      paletteCool: '冷灰调',
      paletteForest: '森野绿',
      paletteOcean: '海洋青',
      paletteSunset: '落日晚霞',
      paletteViolet: '星空紫',
      paletteDesc: '全局配色风格，切换即时生效',
      lightMode: '浅色模式',
      darkMode: '深色模式',
      followSystem: '跟随系统',
      language: '语言',
      fontSize: '字体大小',
      fontSmall: '小',
      fontNormal: '正常',
      fontLarge: '大',
      fontXl: '超大',
      animations: '动画效果',
      animationsDesc: '关闭后全局禁用页面切换与交互过渡动画',
    },
    about: {
      title: '关于',
      githubRepo: 'GitHub 仓库',
      checkUpdate: '检查更新',
      alreadyLatest: '已是最新版本',
      checkingUpdate: '正在检查更新...',
      checkFailed: '检查更新失败，请稍后重试',
      newVersionAvailable: '发现新版本',
      downloadingUpdate: '正在下载更新...',
      downloadComplete: '下载完成，正在安装...',
      installingUpdate: '正在安装更新...',
      downloadUpdate: '立即更新',
    },
  },
}
