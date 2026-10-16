# 更新日志

本文件记录本项目所有值得关注的变更。

格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，
版本遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

> 本文档为中文版本；英文版见 [`CHANGELOG.md`](./CHANGELOG.md)（GitHub Release 流程读取该文件）。

> **工作流规则**：changelog 条目**只在发布版本前一次性写入**——按上次发布以来的提交记录
> 总结归纳；日常修改代码不逐次记录 changelog。

## [未发布]

> 自 **v2.1.1**（2026-09-18）以来的全部改动高度总结——共 621 个提交，按类别分组，一次性按
> 提交记录（`git log v2.1.1..HEAD`）归纳写入。

### 新功能

- **mDNS 类型中枢**（ADR 0054）：每个协议类型只保留一个 listener，进程内 fan-out；双端宿主改为订阅
  中枢，移动端旧链路浏览退役。
- **移动端宿主壳改造成 Android 式平台**（ADR 0046）：主屏、多任务卡片轨道、通知栏、壁纸（含自定义
  图片导入）、权限管理与应用详情；系统返回键改为原生单一监听，按「应用层 → 平台层」路由；设置持久化
  与自启动解耦（ADR 0053）。
- **插件 `platform` 字段**（`desktop` / `mobile` / `both`），装载期与安装期双端匹配门禁；桌面设备壳
  落地于 `/shell` 路由（主屏 + 卡片切换）。
- **agent-hub 应用线**：供应商应用工作流、`opencode` standalone 自更新（GitHub Releases 为版本源）、
  统计看板重做（会话去重 + 多维图表 + 排行）、日志多目录来源，以及文件批量授权——一个业务动作只弹
  一次窗，不再逐文件询问。
- **终端 / 任务 / 通知能力面**：字间距设置（双端同步，0–4 px）、输出可用唤醒、`host-task` 宿主执行器
  与任务队列、「重试中」状态与自动任务间会话轮换、带权限门的 `fs:pick` 文件选择器，以及移动端
  `host-notify`（通知 / 震动 / 声音）。
- **契约与加密面**：链路加密协商套件参数化、WASI preopen 限声明只读档且仅 worker 类别插件可用
  （ADR 0034）、打包链默认注入产物 wasm 摘要并在出包前复核。

### 重构

- **wasm-core 收敛为单一 crate、单一形态**（ADR 0045 / 0047 / 0049）：移动 fork crate 退役，一份机制
  核心 + 一份运行时壳服务双端。全部 `bindgen!` 与 `impl … Host` 绑定迁入两端宿主——内核只保留覆盖
  `wit/core.wit` 的共享 `core_binding`，端事实（平台标识 / 入站端口 / 选择器）改为宿主注入端口。
- **能力域抽为共享 crate**（ADR 0035 / 0040 / 0042 / 0043 / 0044 / 0048 / 0052）：mDNS 发现、WS 出站
  客户端、host-api 实现核、peer 网络、通知与 file-transfer 业务核全部落到 `packages/`；宿主只做端口
  装配与端差异。
- **契约体系重构**（ADR 0051 / 0052）：WIT 上包 semver，整数 ABI 退役；`core.wit` 收窄为机制必需契约，
  功能级交集归能力域分片，端扩展归宿主，版本收敛到单一真源。
- **宿主业务代码清零**（桌面）：会话 `create` / `restart`、HTTP 协议网关、WS 业务面、传输编排、
  host-task 域与认证 / 生物认证面全部下沉插件，并由 fail-closed 的显式注册认证中心裁决
  （ADR 0029–0034）。
- **业务下沉插件**：会话引擎、终端渲染器 / 写入管线 / resize / 滚动域，以及 file-transfer 双端单 wasm
  产物 + 共享业务核；消息总线改为单消费者 FIFO，发布序 = 入队序 = 投递序。
- **前端零资源访问红线**三层落地（ESLint 静态锁 + Tauri ACL 运行期 + CSP 引擎），全部 I/O 发起权收归
  Rust；权限授权面双端同构，单一真源为内核 `PermissionManager`。

### 修复

- **移动端终端输出链路**：背压水位改锚 xterm **解析**完成水位（而非已接收字节）、环容量 4 → 10 MiB、
  事件名改 Tauri 合法、重放按世代清屏——字节缺失、重复渲染与卡 loading 一并消除；PTY resize 补 requester，
  尺寸随网格生效。
- **可靠性**：被静默丢弃的 WS 心跳 `Ping` 不再导致规律断线循环；认证类关闭码致命化，配退避下限与同因
  熔断；链路加密协商 fail-visible，不再把密文喂给业务层；燃料看门狗覆盖三条可选导出回调，杜绝跳调用
  累积烧穿。
- **插件生命周期**：二次激活后的 stale context、白屏 / 过渡空屏、多窗口凭证域回收，以及移动端打开
  AI Chatbox 即 SIGILL 均已修复；guest trap 不再升级为宿主进程 SIGILL。
- **数据与权限隔离**：主库 SQL 隔离下沉到 SQLite 引擎层并取消 storage 默认授予；总线 topic 命名空间
  强制边界；互调回复道补身份门；凭据、密钥与密文一律不落日志。
- **file-transfer**：快照恢复窗口过滤 + 拒绝 `restored` 条目拨号，修好陈旧地址拨号；浏览加载改为以
  活跃对端就绪为闸门。
- **ai-chatbox**：代码块高亮从动态 wasm chunk 改为静态内联导入，消除移动端纯文本降级。
- **设备与连接面**：QR host 按当次网卡枚举校正（换网 / DHCP 死地址）；设备在线状态统一到一个事件名；
  端口修改保存即生效；QR canvas 切走切回不再空白。
- **前端与 UI**：`PluginState` 词表对齐线上 camelCase（打通应用中心启停链）；正式版专有 CSP nonce 故障
  修复；启动双闪与 dev 白屏消除；CJK 重叠与 TUI 右缘黑带去除；「命令 + Enter」现在真的发出命令文本。
- **两批系统性审核修复**（wasm-core M/H/R/S 批次与 terminal-session 批次 1–7），成批关闭正确性与生命周期
  缺陷。

### 优化

- **终端吞吐**：ack 背压配未确认窗口与双水位迟滞，整体收敛到插件侧；输出可用唤醒限频；输出通道与历史
  直读引擎游标环。
- **启动**：预渲染底色跟随系统主题、挂载前预应用主题，消除双闪；终端链路预热只建连、重放按世代清屏。
- **日志纪律**：宿主热路径日志风暴收敛（bus / ws / fs_auth）并配防回接锁；dev 日志两轮降噪。
- **agent-hub**：git 域与用量扫描并行化；检测更新时先重探测本地版本再比较最新版。
- **构建资源**：sccache 上限重测至 30 GiB，`adaptive-run` 包装器按资源自适应限流并发构建，leaf crate
  `Cargo.lock` 保持同步。

### 基层建设

- **wasmtime 47 → 48**，ADR 0019 双端分叉关闭（零代码适配）。
- **契约治理**：版本单一真源（`contracts/bedcode-plugin-package.rs`）+ WIT 漂移锁（`check:wit` / CI /
  husky）；能力域 crate 保持零 WIT、零桌面 SDK 依赖，由 `bedcode-headless-host-probe` 长期门禁。
- **跨端验证变成真的**：两进程台子接入 CI（桌面宿主独立进程 + 移动端 `two_proc` 场景），终端全流程
  用例 C-001–C-009 摘掉 `ignore`。
- **防回接锁体系**：域方向、绑定面、术语漂移、样式 token 与退役面逐一加锁，每条附变异自检。
- **CI 修复**：`finalize-release` 显式传入 `GITHUB_TOKEN`（Actions 不注入该变量，空值导致 401）；
  Android 发布改用 runner 预装 SDK。
- **文档收拢**：code-map 瘦身为目录级导航、`packages/` 索引、ADR 0042–0052；用户面术语由「插件」统一为
  「wasm 应用」。
