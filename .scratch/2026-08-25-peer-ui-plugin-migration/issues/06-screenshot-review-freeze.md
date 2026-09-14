# 06 — dev-shell 截图评审与 UI 冻结（阶段二收口）

**What to build:** 对 01–05 产出的全部插件页面做一轮系统性视觉评审并修正到位：在两端插件工程目录分别用对应 dev-shell 启动浏览器环境，Chrome headless 对每个页面的关键状态逐张截图，交视觉评审（frontend-styles 自查清单 + design-taste 量化基线），按结论迭代样式与交互直至达标。本票完成即冻结前端 UI，后续票不再改样式。

**Blocked by:** 01, 02, 03, 04, 05

**Status:** resolved（2026-08-25，评审记录与豁免清单见 Comments）

- [x] 截图矩阵覆盖：桌面（设备面板三态 / 首连弹窗 / 状态栏待确认项 / 设置覆盖层含可信对端分区）+ 移动（浏览页各态 / 设备 sheet 三态 / 确认对话框 / 自动互信 toast / 设置二级页分区）
- [x] 每张截图经视觉评审：token-bound 取值、无原生控件外观、z-index 层级、过渡属性特定性、移动端 44px 触控与 safe area、深浅色两套主题
- [x] 评审发现的样式/交互问题全部修正或明确豁免（记录理由），无遗留未决项
- [x] 截图产物仅作评审过程件，不入库（.scratch/.gitignore 已覆盖 *.png / *.mjs）
- [x] 冻结声明：本票合入后 UI 层不再接受非缺陷性改动

## Comments

### 评审执行记录（2026-08-25）

- 截图矩阵 22 张存 `.scratch/peer-ui-plugin-migration/shots/`（桌面 12：consent 具名/无名、peer-panel 五态、settings+trusted、revoke 两步、浅色浏览；移动 12：browse 离线/数据、auto-trust toast、consent 具名/无名/排队、device sheet 多态、settings 二级页、trusted 分区、revoke 确认）。
- 评审方式：vision subagent 两批（桌面面板/设置 + 移动全量）完成；桌面弹窗批因上游限流由主 agent 直接读图评审（四张均已逐项过清单）。frontend-styles 清单 + design-taste 基线均过：无 Neon/纯黑/占位名，颜色单主色系，空/错态覆盖完整，弹窗/sheet 间距对齐规整。

### 评审发现并已修复（全部经浏览器复核 + 两端测试绿）

| 级别 | 问题 | 修复 |
|------|------|------|
| P0 | desktop SettingsPanel `defineEmits<{…}>` 缺调用括号，宏不展开运行时 ReferenceError，半初始化组件毒化 Transition 更新路径（locateNonHydratedAsyncRoot 崩溃）→ 设置覆盖层永不出现 | 补 `()>`；Transition 恢复正常 |
| P0 | mobile devMock 导出扁平 PeerDevMock，SDK 协议要求 `{peer}` 包装 → loader 读不到种子，设备/consent/trusted 全空 | devMock 按 SDK 协议包装 |
| P0 | mobile 设置步进器数值框空白（timeoutInput 仅 focus/commit 同步） | 挂载即同步 + watch 设置变化 |
| P0 | desktop `.ft-mini-btn` 双定义（队列图标 22×22 覆盖设备面板文字按钮 →「设为当断开」文字重叠） | 文字按钮拆分 `.ft-text-btn`（含 primary/ghost 变体），图标按钮保留 `.ft-mini-btn`；两侧均加 flex-shrink:0 |
| P1 | desktop dev-shell mock 种子在模块顶层读取，早于 registerDevMock，恒 undefined | initPeerState() 延迟初始化（与移动端同构） |
| P1 | desktop loader 在 activate() 之后才注册 mock 命令，插件首拉设置/设备落空 | mock 注册移到 activate() 之前 |
| P1 | desktop mock list-remote 旧 peerId/entries 契约、settings.roots 字符串形状，与插件 roots/dirId、{id,name} DTO 不匹配 | mock 对齐现行契约；顺修 roots 路径 `\b\D` 转义丢失 |
| P1 | desktop dev-shell 状态栏项 icon（SVG path d）被当文本渲染成路径串铺满底栏 | isSvgIcon 判断 + `<svg>` 渲染（与侧边栏同规则） |
| P1 | 状态点语义：在线未连接与已连接同色、不可达终态仍显示「在线」 | 三态点（实心绿=已连接 / 0.45 绿=在线 / 灰=离线·不可达终态），两端同构 |
| P1 | mobile DialogHost 不渲染 `variant:'danger'`（插件已传，撤销确认无危险样式） | danger 渲染红标题 + 红确认钮（对比 token） |
| P1 | mobile consent 弹窗文案 `\n` 不换行成文字墙（无名兜底安全指令被淹没） | DialogHost message 补 whitespace-pre-line |
| P1 | desktop 设置覆盖层底部提示被容器底缘裁切 | .ft-settings-body 补 padding-bottom |
| P2 | HomeNAS 禁用连接按钮视觉不够弱 | disabled 态 opacity .4 + grayscale |
| P2 | mobile mock 缺 list-remote handler（浏览恒空态） | 补 roots/dirId 契约 handler |

### 明确豁免（记录理由）

- **mobile toast 渲染在手机框顶缘之外**：dev-shell 伪象——toast `fixed top-14` Teleport 到 body，真机 body=全屏视口，与宿主 PluginDialogHost 同形状；不修。
- **mobile 设备 sheet 全宽渲染**：`Teleport to body` 为既有模式（TaskQueueSheet 同款），真机全屏布局正确；评审按全宽=真机布局执行。
- **consent 弹窗指纹无独立视觉强化**：`dialogs.showConfirm` 契约为纯文本 message，富文本/mono 片段需扩宿主对话框契约，留待真实需求驱动；换行分层已缓解。
- **consent 倒计时为静态提示**：ticket 04 已知遗留（真机联调项），非样式缺陷。
- **设置覆盖层「双返回」**：页面级「← 返回」（退出插件面板）与覆盖层「返回」（关闭设置）语义不同，保留。
- **空态行动按钮权重差异**（重新检测对端=primary vs 刷新=secondary）：离线需强引导、空目录轻操作，有意区分。
- **桌面指纹与名称同行**：窄面板有 ellipsis 保护，meta 行已含地址；维持现布局。
- **行内错误文案无警示图标**：纯色红已足够辨识，加图标属后续打磨。
- **「直接收接」错序报告**：经查 i18n 为「直接接收」，视觉误报，豁免。
- **vue-tsc 插件 tsconfig 3 处既有类型错**（FileTransferView notice/handleDownload、TaskPanel TaskStateName）：阶段一迁移遗留的类型债，非本票样式范畴，转 ticket 09 收口核对。

### 验证

- desktop vitest 38 绿 / mobile vitest 46 绿（file-transfer 插件目录）
- 插件 tsconfig vue-tsc：本票改动文件零新增错误（既有 3 处见豁免）
- 修正仅落在插件前端源码与两端 dev-shell（`bedcode-{desktop,mobile}/plugins/file-transfer/src`、`packages/plugin-sdk-{desktop,mobile}/dev-shell`），未触 Rust/宿主/gen-android

### 🔒 UI 冻结声明

**本票合入后，两端 file-transfer 插件前端 UI 层冻结：后续票（07 WASM 代理 / 08 宿主删除 / 09 收尾）不再接受非缺陷性样式与交互改动；缺陷修复须与本票评审基线（本页截图矩阵）保持视觉一致。**
