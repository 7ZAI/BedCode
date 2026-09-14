# 08: dev-shell 演示种子（devMock）与截图验证

**What to build:** 为 agent-hub 插件接入 dev-shell（plugin-sdk-desktop 浏览器调试壳）演示能力：种子数据归插件工程（入口导出 `devMock`，SDK PluginDevMock 协议），dev-shell 只做通用接线 mock（命令 handler 消费种子 + `plugin:agent-hub:*` 事件回流），浏览器中可见五分区「有数据」完整形态并截图验证；顺手修复截图暴露的缺陷。

**Blocked by:** 06

**Status:** resolved

## Answer

**交付**：

- `plugins/agent-hub/src/devMockTypes.ts` + `devMock.ts`：五子域种子（detection / install / skills / providers / usage），全部 wire 形状（与 guest emit 载荷同构）；扩展字段（安装剧本 runScripts、skillContents、反向导入发现、应用文件清单）为演示数据，guest 真实实现从磁盘/进程采集
- `plugins/agent-hub/src/index.ts`：导出 `devMock`（真实宿主忽略）
- `plugin-sdk-desktop/dev-shell/src/mock/agent-hub.ts`：通用接线 mock——探测逐 CLI 动画、安装 run 逐行输出回显 + 轮询、测速/换源/检查更新、Skills 扫描/保存（FNV-1a 重算 hash → 已分发转 stale）/分发/GitHub 安装/本地导入（exists 分支）、供应商 CRUD/反向导入/claude 桥接冲突、使用统计 syncing→ok / 会话分页 / 日志详情
- `loader.ts`：按 devMock 子域存在与否注入（与 file-transfer 同构，不写死插件清单）

**截图验证**（dev-shell @ :5193，六分区 + 关键交互流全走通）：概览（环境条 + 测速对比 + 四 CLI 卡含 codex 双安装警告）、安装与更新（测速卡/行状态机/执行输出）、Skills（三态分发徽标）、供应商（key 掩码 + claude 只读视图 + 桥接提示）、使用统计（水位 tag/汇总卡/每日条形图/CLI 汇总/会话明细）、会话日志（主从布局 + 事件流 + 无 source_path 兜底）；交互流：重新检测动画、Claude 更新 run（镜像参数拼接 + 输出回显 + 已完成）、应用预设桥接冲突确认（inline key 掩码回填只读视图）、技能编辑保存 → 落后 → 重新分发恢复。

**顺手修复的缺陷**（截图/类型验证暴露）：

1. `UsageSessionRow` 缺 `source_path` 可选字段——`read-usage-session` wire 返回该列，SessionLogsTab 头卡直接访问
2. `UsageStats.byProject[].project` 类型 `string` 收窄过严——guest `GROUP BY project` 可空，StatsTab 已有 null 分支 → 放宽为 `string | null`
3. 缺 `src/vite-env.d.ts`（其余插件均有）——补 `.vue` / `*.css?inline` shim，`tsc --noEmit` 归零

**验证证据**：

- `pnpm exec vitest run plugins/agent-hub packages/plugin-sdk-desktop`（bedcode-desktop）：13 文件 / 100 用例全绿
- `pnpm run build:frontend`（agent-hub）：dist/index.js 118.43 kB 构建通过
- `pnpm exec tsc --noEmit -p tsconfig.json`（agent-hub）：0 error
- `pnpm exec eslint .`（根，改动 6 文件）：0 error
- dev-shell 进程与端口已清理（:5193 无监听、无残留 vite 进程）
