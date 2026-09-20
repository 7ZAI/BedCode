# 更新日志

本文件记录本项目所有值得关注的变更。

格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，
版本遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

> 本文档为中文版本；英文版见 [`CHANGELOG.md`](./CHANGELOG.md)（GitHub Release 流程读取该文件）。

## [未发布]

> 仅桌面端（路线图阶段 2 + 阶段 3 会话部分合并为一个批次执行）。**移动端代码零改动、
> 版本号不动**——范围豁免与移动端受损清单见「文档」节。

### 功能

#### 终端会话中心插件 — 设备 / 会话 / 任务合并为单一内置插件（桌面端）
- 新内置插件 `com.bedcode.session`（Application 形态、`rust-ts`、wasip3 组件）承接原先散在 内核、`com.bedcode.devices`、`com.bedcode.auto-task` 三处的产品域：配对与信任与首连确认编排、会话配置 CRUD 与生命周期编排、Agent 任务域（队列状态机、定时任务、agent hook 安装）
- `com.bedcode.devices` 与 `com.bedcode.auto-task` 退役；其模块、侧边栏视图、终端工具栏按钮、任务弹窗、文案表与随包 hook 脚本并入合并插件，按域重组而非逐文件平移
- 贡献式 UI「界面维持、贡献方换人」（spec D6）：四个侧边栏目录（设备配对 100 / 连接历史 101 / 终端会话 200 / Agent任务 210）、经新扩展点 `ui.registerSettingsSection` 贡献的设置分组、终端工具栏按钮；宿主内置入口在插件 `Activated` 时让位，未激活 / error / 停用时由宿主兜底壳接管
- 内核去业务化落地：会话结构体的四个任务字段摘除，换为不透明注解槽（`session-id -> map<string,string>`，内核只搬运透传、绝不解释键名）；线协议形状（`taskStatus` 等）不变，老客户端零改动
- 升级后任务历史一条不丢：宿主侧一次性、best-effort、幂等迁移，把六张任务表从退役插件的私有库搬进合并插件私有库，按列名交集拷贝并落账本戳，重启不重复插入

### 基础建设

#### 插件 ABI 桌面端 16 → 19（既有 interface 的函数级追加；移动端仍 11）
- v18 `host-auth` 认证记录面：`trusted-devices-list` / `trusted-device-revoke` / `connection-history-list` / `auth-setting-set` 返回内核原始记录，排序 / 过滤 / 派生视图归插件
- v19 `host-session` 会话语义面：配置 CRUD（新权限位 `session:config`）、`create-with-spec`（插件算好 launch spec，宿主只做 shell 包装 / WSL 转换 / 尺寸缺省 / ID 预生成）、`restart` / `remove` / `rename` / `resize`（裁决规则在插件、正统端登记在内核）、`annotate`、`connections-list`；另有 `host-platform.wsl-distros`
- 未新开任何通道：三域全部落在既有 20 组 `host-*` 原语内；输出订阅/ack 原语被显式否决（逐帧输出不进 WASM）
- `manifest-gen` 命令面口径收紧：manifest 已声明 `contributes.commands` 即视为人工裁剪过的用户可见面，生成器只报告「臂 / 声明」差集而不覆写（该插件的 release 构建现已幂等）

### 改进

#### 桌面端
- 旧 HTTP 前缀 `com.bedcode.auto-task/*` 由宿主显式别名表在旧插件缺席时应答；「保留 vs 切断」的成本对比与裁决记在票面而非留为隐含
- 插件私有库表名按域前缀统一（`task_*` / `session_*`），配可逆改名账本与回滚路径，并加源码扫描护栏（漏改一处 SQL 即编译期红）

### 安全

#### 桌面端
- `fs_auth` 内置受信任插件白名单种子由 `com.bedcode.auto-task` 改指 `com.bedcode.session`（写用户项目 agent 集成的就是合并插件；不改指会把它静默降级成逐目录弹窗授权）
- 认证分层成文且单点仲裁：配对码 / QR 的编排在插件，签发、验签执行点、密钥托管（host-auth secret-store）与 `pairings` / `connection_history` 表留宿主；插件未激活时宿主桥接回退到宿主实现并 `warn` 留痕，这是设计内降级路径，不算旁路
- 凭据仍只记长度不落明文；插件权限清单收口为 15 项且每位都能追到真实消费点（spec 表格里两位查无调用点的位刻意不声明）

### 测试与质量

#### 桌面端
- 端到端等价回归**零改动断言**通过：`pty_session_chain`、`ws_session_route`、`ws_auth_rules`、`http_auth_biometric`、`server_integration`、`link_crypto_http`、`broadcast_shutdown`、`build_manifest_smoke`（S2 接缝文件与本批次前基线零 diff）
- 故障半径行为测试补齐：`Error` 态贡献面整组摘除、再激活整组恢复、内置入口让位与摘除共用同一判据、设置分组回落纯内置形态、配对桥接降级到宿主服务
- 迁入的任务 UI 首次获得前端测试面（旧插件本来为零）：弹窗按需取数、入队/清空/开关的命令契约、历史视图加载与筛选一致性，外加两条同源护栏——前端调用的每条命令必有 Rust 分派臂、`t()` 用到的每个文案 key 必在两语言表内

### 文档

- ADR 0022 v8：会话语义下沉批次（v18/v19 函数表、注解槽、设置分组扩展点、15 项权限清单、双端偏离表加本批次号）
- 路线图更新：阶段 2 标已落地并记形态改判、阶段 3 标部分落地（会话已做、终端刻意未动）、补记「合并执行」决策与主动打破渐进原则的理由与代价表，移动端受影响清单 M1–M5 从单个 spec 目录挂进路线图
- AGENTS.md §7 修 ABI 计数与宿主能力清单条目、§8 认证语义措辞按插宿主分层改写；`docs/knowledge/plugin-http-endpoint-trust.md` 记旧前缀判定；桌面 code-map 与命令文档改指
- 明确不在范围：移动端适配、`com.bedcode.terminal`、终端窗口与输出管线进插件

## [2.1.1] - 2026-09-18

> 功能 / Features · 基础建设 / Platform & Infrastructure · 改进 / Improvements · 修复 / Fixes · 安全 / Security · 测试 / Tests & Quality · 文档 / Documentation

### 功能

#### 终端输出管线 — TB v3 字节流与环形缓存（双端）
- 桌面端 PTY 输出重写为字节连续管线（TB v3）：bytes 块队列 / v3 帧 / 字节游标与双速传播；慢消费者从环形缓存按订阅者游标拉取，背压不再阻塞生产者；`session_output` 链路调试统计（产出/ack 节流打点）
- 桌面端：一次性历史接口 `GET /api/sessions/{id}/history`；前端终端输出流适配 v3 字节游标（WS + Channel 双路径）
- 移动端：终端输出链路迁入 Rust 后端（`terminal_link` + 前端接线）；移除 TB v2 帧与旧 `ws_event` 通道终端残留死代码
- 移动端：段2 背压改为 ack 驱动补投，输出帧改为页面级 Channel
- 桌面端：删除 WS 环回终端链路（`local_token` / 环回 WS / 旧输出流）

#### 桌面端插件管理 — zip 加载与卸载
- 插件详情页对**所有来源**的插件都提供卸载（内置随包 / 文件扫描 / zip 安装）；卸载要求插件处于**未启用**状态（运行中按钮禁用并提示先停用），且清空插件全部数据：安装目录（取自 `extension_path`，含同目录私有 `plugin.db`）、键值存储、持久化文件系统授权（`fs_granted_paths` / `preauth_paths`）、持久化审批记录（`__system__` 空间的 `plugin_approvals` 条目：批准权限集 + 内容哈希钉扎）、持久化激活状态、数据库缓存连接与运行时限频记录。内置插件位于随包资源目录：只读安装下删除会如实报错，下次构建/更新后随包副本会重新出现
- 支持从本地 zip 包加载插件（解压到用户插件目录，来源标记 `user-installed`）
- 插件列表布局：加载插件按钮（主题色）移到「未启用」分区标题右侧，无未启用插件时依然可达；刷新按钮移到工具栏最右
- 卸载完整性：卸载时撤销持久化审批记录、按加载插件入口定位；清理插件孤儿残留目录，修复卸载后重装被磁盘查重卡死
- `fs_auth` 授权粒度精确化为三态（目录 / 文件 / 父目录）

#### 移动端 HTTP — Rust 统一代理与 fail-closed Egress 策略
- 移动端 HTTP 全部收束到单个 Rust 代理，配 fail-closed 三层 Egress 策略；前端 HTTP（`useHttpApi` / UpdateChecker / LinkEncryption）全部经此代理，移除 `@tauri-apps/plugin-http` JS 依赖
- Egress 授权弹窗 + 设置页授权查看 / 撤销
- 跳转重校验防 SSRF（移动端 Egress 与桌面端插件 HTTP 双端落地）
- SDK：`link-crypto` 增加 HTTP 密钥派生；SDK manifest `preauthUrls` 声明

#### file-transfer 插件 — 显式暂停/续传与并发控制
- 宿主并发闸门 + 显式暂停/续传（双端同构）；插件 `paused` 语义 + 前端暂停/继续/全部继续与并发设置
- 显式暂停/续传 wire 协议与数据面门控（双端 + `peer-net`）
- 桌面接收队列：接收卡速率 / ETA、清空历史二次确认、面板合计速率
- 移动端任务卡进度条活跃态用主题色（暂停/排队不再灰色误导）
- 双端暂停/恢复/取消状态同步（wire + 数据面门控 + persist 单事务）

#### 移动端终端体验
- 终端体验优化：扫码整合 / 新手引导 / 键盘避让 / 输入条 / 主题 / 帮助文档
- 字体档位跨度加大 + 会话数量限制
- TUI 鼠标上报嗅探支持多参数 DECSET 与真实上报开关
- TerminalView 退化为编排层，业务按域拆分（行尾静态裁切、网格写入收口）
- 移除 `peer_pick_folder` 命令（SAF 树 URI 统一文件夹选择）

#### 插件 SDK
- `host-peer` 传输控制三原语（pause / resume / resume-all）进入 ABI 契约；`plugin-component-test` fixture ABI 同步至 9
- `MarkdownEditor` 组件（marked 渲染 + raw HTML 转义 + 语法高亮）
- 两端 SDK 打包为 GitHub Release 附件（npm tarball / crate / 聚合 zip + SHA256SUMS）

### 基础建设

- 构建资源自适应包装器 `adaptive-run` + `build-profile`：采样 CPU 负载 / 可用内存 / swap 压力并注入编译并行度（`CARGO_BUILD_JOBS`、Gradle `workers.max`、`NODE_OPTIONS` 堆）；用法见 `docs/commands.md`
- CI 发布流水线为每个插件打 zip 分发包 + 双语 release body
- 移动端 dev 日志默认 verbose（logcat 含 Rust `debug!`）
- 移动端应用名统一为 **BedCode**；停用静态首屏动画，Android 开屏纯色化
- pi session 归档脚本（默认阈值 15 → 10 天）；文档跟踪策略：`docs/` 全分支正常跟踪，受保护路径缩减为受保护配置文件

### 改进

#### 桌面端
- 巨型前端组件拆分：TerminalPreview → 编排层 + 终端域 composable，SettingsView → 分组设置子组件，useDesktopCommands → 域命令模块（会话 / 设备 / 设置 / 事件）

#### 移动端
- dev 日志过滤非业务噪音，控制台与落盘同规则

### 修复

- **终端**：输出管理器注册时序（PTY 启动前注册 + 启动失败回滚）；订阅激活竞态；pty 链路历史/实时拼接竞态与重进游标语义；重复进入终端页作废上一代段2 推送通道；显示区与输入栏贴合间距；`terminal_link` 静默吞错点补日志；特殊按键字节非 UTF-8 时丢弃并告警
- **桌面端插件**：用户安装的 rust-ts 插件执行完整 guest 生命周期；插件并发闸门脉冲失败不再静默吞错（补 warn）；正式版禁用右键原生菜单；`tauri-build.js` DEB 重命名正则转义修正
- **file-transfer**：暂停/恢复/取消双端状态不同步；issue 16/17 缺陷修复（暂停恢复/速率计算 + 桌面前端静默失败）
- **移动端**：`terminalRowClip` 闭包内 `rowsEl` 非空断言（vue-tsc 收窄失效）；`http_auth_flow` 全局 token 用例加串行闸（消除默认并行 flake）；触摸滚动单元格高度兜底重算；running 状态广播不再复位存活会话缓冲

### 安全

- 双端跳转重校验（桌面插件 HTTP `redirect_decision`、移动端 Egress）封堵 SSRF 路径
- 移动端 HTTP 在 fail-closed 三层 Egress 策略下默认拒绝

### 测试与质量

- 桌面端 unit-test audit：32 张票据全落地（lib 基线 615 → 784）
- 移动端 mobile-test-audit：3 个 P0 lane 全落地 + P1 部分
- 新增测试：段2 通道补帧解析、file-transfer 清空历史二次确认弹窗（驱动 + 失败/取消反例）、任务卡活跃态主题色
- `unit-test-discipline` skill 接入 AGENTS.md 任务路由

### 文档

- `docs/commands.md`：自适应构建章节（`adaptive-run` 用法）+ 构建 profile 动态覆盖指引
- pty 输出管线 TB v3 架构文档（`docs/knowledge/pty-output-pipeline.md`）+ pty-byte-history 任务记录
- 移动端终端链路 code-map 增补（TB v3 标注）；mobile-ws-rust 方案 / 审查 / 遗留记录
- 架构图迁移至 `docs/diagrams`（archify 交付物 + README 链接）
- feature-branch 隔离 spec（task-scheduler / OCR / code-viewer）与 file-transfer 并发/暂停续传实施记录
- 移除过时的 implementation-plans 与 skills-course 学习文档

## [2.1.0] - 2026-09-10

> 功能 / Features · 基础建设 / Platform & Infrastructure · 改进 / Improvements · 修复 / Fixes · 安全 / Security · 测试 / Tests & Quality · 文档 / Documentation

### 功能

#### 对等网络 — P2P 直连链路（`packages/peer-net`）
- 新增双端共享的 Rust crate：节点身份、绑定公钥的自签证书、TLS 1.3 mTLS 直连、信任存储、专用 mDNS 发现、共享目录浏览与批级断点续传传输引擎
- 节点电源总线：集中的节点生命周期管理，双端完成插件生命周期接线与入站连接事件桥接
- 专用 `_bedcode-peer` mDNS 服务，带 TTL 在线缓存与可测试的注入缝
- TCP keepalive 活性探测、mDNS 周期性重广播/重查询实现可靠互发现、移除首帧超时
- 首连确认闸门、撤销并重确认流程、节点生命周期闸门消除 TOCTOU 竞态

#### 对等网络 — 加密核心（`packages/link-crypto`）
- 新增共享加密 crate：出站密钥 miss 观测、缓存容量护栏、GET 指标校正、query 路径归一（跨端）
- 移动端 pin 写入校验与指纹清理；HTTP GET/HEAD 空 body 协商；`X-BedCode-Crypto` 响应标记
- 生物凭证绑定/解绑迁移到 HTTP（脱离 WebSocket）

#### file-transfer 插件 — 对等化重写
- 旧的局域网文件服务整体退役，文件传输全链路改由对等栈接管
- V2 三段式主视图（移动端），业务逻辑由插件自持（Phase 3，双端）
- 设备发现刷新、端点记忆、历史快照、首连确认超时
- 双端共享目录多选；桌面端设置显示完整路径
- 端点清理/校验、设备单例、确认超时定点结算、根缓存重置
- 传输面板终态归档：进度 / 原因 / 打开所在文件夹 / 双端记账（两端各自记录同一次传输）
- 分区存储下的 SAF `content://` URI 中转复制兜底（解决 `EACCES`）

#### 认证与传输架构重做
- HTTP 生物认证与事件通道原语（桌面端）
- WebSocket 首消息 JWT 认证，以及专用的 `/ws/event` 事件通道
- 移动端常驻事件 WebSocket：认证后建连，断线自动自愈
- 移动端 HTTP 认证客户端；终端直连 WebSocket（`useTerminalSocket` + 状态机 store）

#### 终端输出管线
- 字节流 PTY 管线重写：基于序号的输出队列 + 快照订阅（废除字节偏移契约）
- 每会话终端路由，远端通道使用 TB v2 二进制帧；本地通道迁移到 TB v2 并支持快照重订阅
- 删除旧广播兼容通道与死代码
- 移动端终端写入管线让出主线程：128 KB 分块让出 + `flushing` 重入守卫
- `get_terminal_ws_info` 替代已移除的移动端输出链路；移动端支持 ack / yield / history 缓存

#### auto-task 插件
- 任务历史状态筛选由 chips 改为下拉 Select（默认「全部」）
- 各 Agent 的终端输入提交符统一为 `\r` Enter 字节

#### ai-chatbox 插件
- 供应商限流自动重试（双端）

### 基础建设

- 两端 `package.json` 与 Tauri 配置版本升级至 **2.1.0**；新增 `.deb` 构建支持
- CI：在 Windows / macOS 之外新增 Linux 构建目标；cargo 编译资源限制
- CI：新增双端 Rust + vitest 回归测试门禁（分层独立 job），合并到 `master` / `uat` 时阻断
- CI：`release.yml` working-directory 相对仓库根解析；SDK 构建步骤由 `pnpm filter` 语法改为 working-directory 模式
- CI：verify-latest.json 改用 draft release 资产查询；签名密钥步骤注入 `TAURI_SIGNING_PRIVATE_KEY`
- **E2E 基础设施（桌面端）**：WebdriverIO + `tauri-plugin-wdio`（仅 debug 隔离）+ 外部 `tauri-driver`；smoke 断言验证 execute/IPC 链路。移除遗留的 `@playwright/test` 依赖
- **插件 SDK 更名并发布**：`@binblink/plugin-sdk-*` → `@binblink/bedcode-plugin-sdk-*`（桌面 + 移动），v0.1.1 发布到 npm 与 crates.io；SDK 包文件精简（dev-shell 排除 `node_modules` 与构建产物），补充 `.npmignore`
- 共享 Rust crate 迁入 `packages/`（`peer-net`、`link-crypto`）
- 前端包管理器全仓从 npm 迁移到 pnpm，各项目各自保留独立 lockfile
- `dev-run` 进程组回收（向整组发信号，而非只杀父进程）+ plugin-watch 孙进程自清理
- `doc-tracking.sh` 行尾修复，恢复 pre-commit 保护逻辑；`PROTECTED_PATHS` 扩展覆盖两端 `docs` 子目录
- AI 工具配置入库；`.agents/skills/` 统一供 pi / OpenCode / Codex / Claude Code 共用
- 会话产物不入库（pi-lens 备份、compactions）
- Rust MSRV 1.94（wasmtime 47）；插件 WASM 以 `--release` 构建并保留 names section

### 改进

#### 桌面端
- 终端：xterm 残影收敛 — 背压滞回、Channel 传输、渲染器决策、有序写入队列
- Linux 终端渲染优化；WebKitGTK IME 防护重构为单一 attach 点
- 登录 PTY 以 `-lic` 启动，使其继承 Linux 用户 PATH
- Windows 熄屏 / 休眠唤醒后 WebView2 黑屏自愈
- 启动屏 footer 版本号改为读取单一真源；调整窗口尺寸与启动背景色
- 插件列表简介字号 12px → 11px

#### 移动端
- 设置页拆为 `views/settings/` 子视图：外观、认证、连接、通知、关于（主视图由约 1130 行降至约 400 行）
- Android 启动主题简化 — 移除系统开屏定制；开屏深浅色跟随 + 启动窗口背景统一
- SplashScreen 开屏页暂时下线（注释保留，可一行恢复）
- 手势滑动不再与导航栏切换动画互相打断
- 键盘收起时退出终端输入编辑态
- 通知后台语义收口；死代码清理；导航图标对齐

#### 日志与可观测性
- **前端**：引入 loglevel 作为统一前端日志框架，替代直接 `console` 调用；仅 dev 构建转发到 `frontend.*.log`（桌面端）与 logcat（移动端），release 自动剥离
- **桌面宿主**：HTTP 请求全路径日志、JSON span 链、启动 bootstrap 通道、存量日志字段化迁移
- **桌面插件（WASM）**：启用 `wasm_backtrace_max_frames(32)`，使 trap 携带 WASM 调用栈；trap 在宿主侧留日志；调试模式（`BEDCODE_PLUGIN_DEBUG=1`）以 DWARF 构建插件并支持行号解析、燃料预算放大 32 倍；通过 `BEDCODE_PLUGIN_LOG=id=level` 按插件设定级别
- **移动端**：release 日志级别收敛；dev 日志保留治理（14 天滚动清理）
- 宿主错误处理加固：自描述 `io` 错误包装、span 插桩、spawn 错误边界

### 修复

- **对等网络**：停机 busy-loop、keepalive 留痕、目录 `size` 契约跨端归一；节点生命周期 TOCTOU 闸门；连接态重发补充 `deviceName`
- **文件传输**：移动端共享目录多选、mDNS 单守护恢复互发现、连接感知；桌面端启用死锁改为「启用先行」并把预授权弹窗前置于 loading；插件动态 UI 严格跟随启用状态（生命周期对称拆解 + mDNS 多播锁去重入）
- **终端**：opencode TUI 滚动残影 — rAF 合并写入默认开启 + 滚动停止补刷；移动端 `touchmove` cancelable 守卫；zoom 移除后同步 zoom-compensation 注释
- **移动端**：`SafPickerPlugin` 构造参数恢复为精确的 `android.app.Activity`（JNI 签名查找返回 `null` 导致 NPE）；platform-tools 升级后 adb fd0 shim 自愈；插件授权弹窗不再与 loading 同现；真机发布包联调缺陷
- **桌面端**：VerifyCode 配对成功的 `Authenticated` 响应补充 `device_name`；开屏 caret 残留清理；全局通知去重
- **构建 / 开发**：`plugin-build` 的 `JSON.parse` / `execSync` 补充错误上下文；V2 合并后的 file-transfer 组件导入路径修正

### 安全

- **对等身份**：首启纯随机生成 Ed25519 节点身份并原子持久化，绑定进 rcgen 自签证书，通过 ring 校验 `CertificateVerify` — 伪造身份无法通过握手
- **信任存储**：首连确认闸门，支持撤销并重确认，使已信任的对端可被撤销后重新受审
- **传输**：WebSocket 首消息 JWT 认证 + 专用认证后的 `/ws/event` 通道；移动端生物凭证绑定/解绑由 WebSocket 迁移到 HTTP
- **沿用 2.0.0**：插件身份校验与权限审批、生物认证链路加固、为 Agent hooks 保留本地旁路的 JWT 网关

### 测试与质量

- **桌面端 Rust**：583 个测试；集成覆盖 HTTP 契约、WS 配对认证、PTY 会话链路、多客户端广播 + 优雅停机、契约 fixture 漂移对齐
- **移动端 Rust**：251 个测试；L1/L2 集成套件落地，并修复断线重连缺陷
- **peer-net**：102 个测试，含双节点 harness（发现注入、共享目录、传输会话）
- **前端**：桌面端 61 个文件 569 个测试；移动端 42 个文件 360 个测试（views / stores / composables / integration）
- **插件 SDK**：桌面端契约测试 5 → 85；移动端 2 → 79
- E2E smoke 套件，验证 WebdriverIO → tauri-driver → IPC 链路
- CI 回归门禁：任一层失败即阻断合并到 `master` / `uat`

### 文档

- README 按 2.1.0 重写：版本徽标（含 wasmtime 47）、平台补充 Linux、Claude Code → Pi/Opencode 文案、移除过时截图；`README_en.md` 同步
- 两端目录级代码地图（`bedcode-desktop/docs/code-map.md`、`bedcode-mobile/docs/code-map.md`）作为模块查找索引
- ai-chatbox / auto-task / file-transfer 双端插件架构图（Archify）
- 知识库：`pty-output-pipeline`、`mobile-terminal-optimization-reference`、`release-workflow`、`sdk-publish`、`github-actions-setup`、`build-process`、`feature-branch-isolation`
- ADR：移动端文件服务退役（对等栈接管）、代码查看器设计
- auto-task DAG 编排 spec；xterm 透明模式残影收敛 spec 与票据；desktop-e2e-webdriver spec 与票据；插件 WASM 日志 spec
- 特性分支隔离落地：桌面端定时调度插件 → `feature/task-scheduler`，移动端 OCR 插件 → `feature/ocr-plugin`，代码查看器设计 → `feature/code-viewer`

---

## [2.0.0] - 2026-08-16

### 新增

#### 插件系统 — WASM 平台
- 插件运行时迁移到 WASM Component Model（wasmtime）；移除 cdylib 动态加载，Component 成为唯一支持的形态
- ABI 演进 v2 → v6：类型化宿主 API、参数绑定 SQL、内存回收、out_ptr、签名校验、插件状态上报、InputSubmitted 观测扩展点
- 运行时加固：epoch 中断 + 资源限制、燃料看门狗、trap 自动重载恢复、AOT 缓存、wasmtime 47
- 安全：插件身份校验与权限审批（防冒充）
- 工具链：bedcode-plugin CLI（create / build / dev / validate / doctor / manifest）、Dev Shell 浏览器开发环境（双端，`--host` 供手机访问）、manifest-gen
- 生命周期：动态激活/停用并持久化状态、热重载、安装/卸载、loading 遮罩
- 能力：每插件独立 SQLite 数据库、插件间消息总线、host_notify、fs_auth 批量目录授权、文件服务挂载/传输、WSL 文件系统桥接
- SDK 内置共享 UI 组件库（Rust + TS，双端）

#### auto-task 插件
- 多 Agent 支持：Claude Code / pi / opencode / Codex（注册表驱动的 Agent 适配架构）
- 任务队列：调度、自动执行、自动应答、预设任务（一次性）、定时任务状态机、任务历史与统计、筛选与重试
- 移动工具箱：任务历史 / 定时任务面板
- TUI-agent 首次派发兜底（15s 宽限）与按 Agent 的终端 hooks

#### file-transfer 插件
- 局域网文件传输插件（WASM 核心 + 桌面/移动 UI + 打包分发）
- 双向传输：发送到手机（上行）、带策略审批的接收、异步批量审批、传输历史、断点重新入队、专用 downloads_dir
- Android SAF 流式传输：共享目录、带 pfd 强引用的 SAF 选择器、全盘访问授权引导

#### ai-chatbox 插件
- 纯 AI 对话重写（双端）：多供应商、流式 SSE 解析、thinking 模式、Shiki 高亮、代码渲染配置、JSONL 持久化

#### 移动端
- 插件系统启用：动态路由、插件管理页、工具箱入口、插件导航页签
- Android SAF 文件/目录选择器（startActivityForResult）
- 生物认证：密钥、认证设置页、质询-响应、设备身份持久化
- 终端：会话预载、游标式输出订阅、TUI 滚动兼容（SGR）、Agent CLI 命令预设、16 键默认快捷键
- 强调色色板（与桌面端同源）

#### 桌面端
- 终端 PTY 回放与历史播放（Rust 侧恢复窗口关闭期间丢失的输出）
- 字节流 PTY 输出管线：字节偏移契约、游标增量重订阅
- 四套主题色板（forest / ocean / sunset / violet）
- 生物质询-响应与连接历史
- SystemInfo 采集与设备名广播、通用加密工具模块

### 变更

- 插件后端完全 WASM 化（移除 cdylib）；Rust MSRV 提升至 1.94（wasmtime 47）
- Toast 迁移到 vue-sonner（双端）
- 桌面 UI 基于 Warm Workbench 设计重建；移动 UI 统一为分组卡片风格；字号 token 化
- 终端尺寸控制改为远端优先，移动端支持暂停/恢复订阅
- 输出管线迁移到本地 WS 单通道（桌面端）；游标式订阅取代 2MB 前端环形缓冲（移动端）
- 构建：rust-lld 链接器、thin LTO、版本升级脚本、安装包 release 后缀重命名、CI 以 wasm32 目标构建插件产物 + Windows 签名指纹注入
- Skills 统一至 `.agents/skills/`，供 pi / OpenCode / Codex / Claude Code 共享

### 修复

- 终端：输出连续性（游标增量重订阅解决重放风暴）、长时间运行页面崩溃、丢帧（异步插件回调、drain/reset）、重连后尺寸同步
- 文件传输：文件名冲突（409 + 拒绝原因）、通知风暴、任务竞态、Windows 路径分隔符、资源管理器定位、`.part` 残留清理
- 插件：多插件 PluginContext 污染导致 i18n 失效、WASM trap 恢复、燃料耗尽 trap、loader 句柄释放、WSL 子进程超时
- 移动端：心跳 blocking_write panic、Activity 重建后选择器失效（EBADF）、订阅泄漏、重连状态不一致
- 桌面端：设置保存循环（内容快照比对）、端口输入、删除后会话命名回退

### 安全

- 插件身份校验与权限审批（防冒充）
- 生物认证链路加固：IPC 序列化、DER 解析、绑定守卫自检
- 为 Agent hooks 保留本地旁路的 JWT 网关（token 从 hook 脚本中移除）

### 测试

- 前端 +175，桌面端 Rust +204，移动端 +116，SDK 契约测试（桌面 5→85，移动 2→79），file-transfer 宿主单元测试

---

## [1.1.0] - 2026-07-05

### 新增

#### 插件系统
- 支持 cdylib 动态加载的 Rust 插件 API crate
- 插件清单类型与权限系统
- PluginHost 与 API 桥接 Tauri 命令
- UI 插槽的扩展点注册表
- 插件加载器、存储与 `AppError::Plugin` 变体
- 完整的前端插件系统（PluginRegistry）
- 自动生成配置表单的 PluginConfigView 页面
- 带列表、开关、可展开详情的 PluginsView 页面
- usePluginManager composable
- PluginTerminalToolbar 与 PluginTitleBarItems 渲染组件
- registerTerminalToolbarItem 与 registerTitleBarItem 代理 API
- AI chatbox 插件重写为独立 cdylib 插件
- 资源目录插件加载与 API 安全
- 插件侧边栏 / 工具箱视图路由与导航
- 插件页面 i18n key

#### 移动端
- 面向性能的 Buffer-Only 终端架构
- mDNS 服务发现与广播
- 按会话的任务通知系统
- 自动执行任务引擎与终端集成
- WebSocket 心跳保活与重连改进
- 侧边栏 + 代码显示布局的 CodeExplorerView
- 支持行级着色的 Diff 渲染
- 带 diff 模式的 FileViewerModal
- 带类型徽标、状态与操作菜单的 PresetTaskCard 组件
- 基于 localStorage 持久化的 usePresetTasks composable
- 快捷键配置弹窗与终端输入栏无限轮播
- loading 遮罩与交互改进
- 快捷栏按钮颜色与快捷键面板一致
- 所有弹窗的平滑开合动画
- 集成带通配符 scope 权限的 tauri-plugin-http
- ForegroundService 中的 WakeLock

#### 桌面端
- Actix Web 服务器的高级网络配置
- 带配置迁移到 properties 格式的服务器管理页
- 用于设备识别的指纹跟踪
- 启动时端口可用性检查
- FileSidebar 标题栏中的 Git 分支切换器
- 电源管理功能
- Claude Code hooks 从全局配置迁移到项目级配置
- 带 session ID 绑定的全局化 hooks

#### 服务端 / 后端
- 在现有 WsServer 旁新增 Actix Web HTTP 服务器
- Actix Web HTTP 控制器、DTO 与中间件
- 用于终端 I/O 的 Actix WS actor
- WS 指标与配置端点
- HTTP + WS 双协议支持
- 文件内容 / diff 树 HTTP API
- 终端输出缓冲区以减少 WebSocket 消息数
- 当前行输入跟踪与插件事件响应

#### 国际化
- 带语言持久化的 vue-i18n 基础设施
- 全部视图、组件、composable 的 i18n + 错误码系统
- 带语言切换 UI 的 i18n 设置页
- 导航、布局与共享组件的 i18n
- 终端视图与输入栏的 i18n
- BottomSheet 与 PairingInput 组件的 i18n
- 桌面端 SessionManager、SessionsConfig 与组件文件的 i18n

#### 代码查看器
- useCodeHighlight 多主题支持
- 用于代码查看器设置的 useCodeViewerStore
- CodeViewerSettingsModal 组件
- 在 FileViewerModal 与 CodeExplorerView 中集成代码查看器设置

### 变更

- 插件重构为任务状态管理器，引入 KeyCombo 系统与自动审批模式
- 用进程内 Actix Web 取代 IPC 子进程
- 将 event/ 合并进 events/，修复 IPC 运行时
- 移除 desktop/ 与 shared/ 层级，Rust 模块按领域扁平化
- 移动端模块结构扁平化并迁移 Android 包名
- 移动端：移除 auto-executor、抽出 FileExplorer、新增浅色代码主题
- 移动端：TerminalView 重构并简化预设任务
- 桌面端：重组 Rust 模块、新增 mDNS、基于设计 token 重做 UI
- 桌面端：服务器重置默认值 + UI 打磨
- 移动端通知迁移
- 任务选择器重构

### 修复

- 移动端连接错误处理与状态一致性
- 路径分隔符归一为正斜杠
- 侧边栏动画改进
- 重连处理与特殊键修饰符
- 返回会话列表后移动端终端滑回问题
- 插件状态类型处理与表头
- PluginViewHost props 路由
- IPC reader 实现与 sysinfo 指标
- 按钮符号清理

---

## [1.0.0] - 2026-06-30

### 新增

#### 核心架构
- 多项目 monorepo：bedcode-desktop + bedcode-mobile 作为独立项目
- 桌面端与移动端之间的 WebSocket + HTTP 双协议通信
- 用于设备配对的 X25519 密钥交换
- 所有通信的 AES-GCM 加密
- 基于系统 keychain / secret service 的安全存储
- 带 60 秒过期时间的 6 位配对码认证

#### 桌面端
- 会话管理界面（创建、编辑、删除会话）
- 带二维码显示的设备配对界面
- 集成 xterm.js 的终端预览
- 带快捷操作的系统托盘
- 网络与外观配置的设置页
- 面向 Windows 与 WSL2 的 PTY（伪终端）管理
- 基于 SQLite 持久化的会话配置管理
- 面向移动端的 WebSocket 服务器
- mDNS 设备发现服务
- Tmux 会话集成

#### 移动端
- 设备发现与配对流程
- 支持增强 / 原始模式切换的终端输出显示
- 带特殊键（Tab、Ctrl+C、Esc 等）的输入栏
- 可自定义命令的快捷操作网格
- 带搜索功能的历史记录
- 带通知偏好的设置页

#### 后端（Rust）
- 基于 SQLite 的数据层（pairings、sessions、messages、quick actions）
- 基于 portable-pty 的 PTY 进程管理
- 带路径转换的 WSL2 支持
- WebSocket 消息协议
- ANSI 转义序列解析器
- Markdown 代码块提取器
- 带等待输入检测的输出解析器
- 带免打扰时段的通知服务

### 安全
- 所有 WebSocket 通信经 WSS 加密
- 配对码 60 秒后过期
- 连接时校验设备指纹

---

## [0.1.0] - 2026-04-30

### 新增

#### 核心功能
- 基于 Tauri 2.0 + Vue 3 + TypeScript 的初始项目结构
- 面向 Windows 与 WSL2 的 PTY（伪终端）管理
- 基于 SQLite 持久化的会话配置管理
- 面向移动端的 WebSocket 服务器
- mDNS 设备发现服务
- 6 位配对码认证

#### 桌面 UI
- 会话管理界面（创建、编辑、删除会话）
- 带二维码显示的设备配对界面
- 集成 xterm.js 的终端预览
- 带快捷操作的系统托盘
- 网络与外观配置的设置页

#### 移动 UI
- 设备发现与配对流程
- 支持增强 / 原始模式切换的终端输出显示
- 带特殊键（Tab、Ctrl+C、Esc 等）的输入栏
- 可自定义命令的快捷操作网格
- 带搜索功能的历史记录
- 带通知偏好的设置页

#### 后端（Rust）
- 基于 SQLite 的数据层（pairings、sessions、messages、quick actions）
- 基于 portable-pty 的 PTY 进程管理
- WSL2 支持（含路径转换）
- Tmux 会话集成
- WebSocket 消息协议
- ANSI 转义序列解析器
- Markdown 代码块提取器
- 带等待输入检测的输出解析器
- 带免打扰时段的通知服务

#### 安全
- 用于设备配对的 X25519 密钥交换
- 通信的 AES-GCM 加密
- 基于系统 keychain / secret service 的安全存储

### 变更
- 无（首次发布）

### 修复
- 无（首次发布）

### 安全
- 所有 WebSocket 通信经 WSS 加密
- 配对码 60 秒后过期
- 连接时校验设备指纹

---

## 版本历史

| 版本 | 日期 | 说明 |
|---------|------|-------------|
| 2.1.1 | 2026-09-18 | 终端输出管线 TB v3 + 环形缓存、插件 zip 加载/卸载、移动端 Rust HTTP 代理 + fail-closed Egress、file-transfer 暂停/续传、移动端终端体验、SDK host-peer 原语、自适应构建包装器 |
| 2.1.0 | 2026-09-10 | 对等网络（`packages/peer-net` + `link-crypto`）含 TLS 1.3 mTLS 与信任存储、file-transfer 对等化重写、JWT + 常驻事件 WebSocket、终端输出管线重写、统一前端日志、WASM trap 日志、E2E + CI 门禁、SDK 更名为 `@binblink/bedcode-plugin-sdk-*` |
| 2.0.0 | 2026-08-16 | WASM Component Model 插件平台、auto-task / file-transfer / ai-chatbox 插件、移动端插件系统、生物认证、UI 重做 |
| 1.1.0 | 2026-07-05 | 插件系统、移动端终端重构、国际化、Actix Web 服务器 |
| 1.0.0 | 2026-06-30 | 多项目 monorepo，桌面 + 移动稳定版发布 |
| 0.1.0 | 2026-04-30 | 含核心功能的首次发布 |
