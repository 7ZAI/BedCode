# 更新日志

本文件记录本项目所有值得关注的变更。

格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，
版本遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

> 本文档为中文版本；英文版见 [`CHANGELOG.md`](./CHANGELOG.md)（GitHub Release 流程读取该文件）。

> **工作流规则**：changelog 条目**只在发布版本前一次性写入**——按上次发布以来的提交记录
> 总结归纳；日常修改代码不逐次记录 changelog。

## [未发布]

> 自 **v2.1.1**（2026-09-18）以来的全部改动高度总结，一次性按提交记录（`git log v2.1.1..HEAD`）
> 归纳写入；原逐条详细记录已合并进本总结。

### 架构 —— wasm-core 单一 crate、单一形态、机制一份

- **单一运行时**：移动 fork crate 退役；`packages/bedcode-wasm-core` 以一份机制核心 + 一份运行时
  壳服务双端（ADR 0045 / 0047 / 0049）。全部 `bindgen!` / `impl … Host` 绑定迁入两端宿主——内核
  唯一绑定是覆盖 `wit/core.wit` 的共享 `core_binding`；WASI 装配为共享机制、按 `CoreConfig::wasi`
  开关；端事实（平台标识 / 入站服务器端口）与文件夹选择器改为宿主注入端口。
- **能力域共享 crate 化**（ADR 0035 / 0040 / 0042 / 0043 / 0048 / 0052）：共享 mDNS 引擎
  （`bedcode-discovery-engine`）、WS 出站引擎（`bedcode-ws-client-engine`）、共享 host-api 实现核
  （`bedcode-host-api-core`：storage / bus / database / log / events）、peer 引擎集中化
  （`bedcode-server-peer-net`）、通知契约（`bedcode-notify-engine`）。宿主只做端口装配，域逻辑不再
  住内核或各端副本。
- **契约体系重构**（ADR 0051 / 0052）：WIT 上包 semver（`bedcode:plugin@0.2.0`），整数 ABI 退役
  （桌面 v38 / 移动 v21 台账迁入本 changelog）；core.wit 收窄为机制必需契约，功能级交集归能力域
  分片，端扩展归宿主；版本唯一真源 `contracts/bedcode-plugin-package.rs` + WIT 漂移锁
  （`check:wit` / CI / husky）。
- **宿主业务代码清零**（桌面）：host-session `create` / `restart` 原语退役（ABI v21）、HTTP 协议
  网关与路由注册下沉（v29）、WS 业务面下沉（v28–v31）、传输编排下沉插件（v30/v31）、认证中心
  显式注册 + fail-closed 裁决（ADR 0031 / 0033）、生物认证面迁入认证中心（v34）、host-task 域
  （v20）、错误信封纪律（ADR 0030）、事件循环属主 + 实例级降级（ADR 0029）。

### 移动端 —— 宿主壳改造成 Android 式平台

- 旧四页宿主 UI 退役；新壳（`bedcode-mobile/src/shell/**`）提供主屏 / 多任务 / 通知栏 / 壁纸 /
  权限管理 / 应用详情（ADR 0046）；多轮修订：系统条退役、运行页胶囊、多任务卡片轨道、设置归属、
  自定义壁纸导入。
- 业务页面归 wasm 应用（`registerSurface`）；旧嵌入扩展点（`registerToolboxPage` /
  `registerNavTab` / `registerTerminalToolbarItem` / `registerTerminalView`）整面退役；
  dev-shell 改造成与宿主壳同构。
- 前端修复：插件状态按线上 camelCase 词表识别（桌面同步修复）、运行期 store 作用域隔离（重启即
  全新状态）、运行页同屏一页切页、swipe-zone 判定修复；终端应用改名 **BedCode** 并换上双端统一
  BedCode logo；wasmtime 47 → 48（ADR 0019 双端分叉关闭）。
- 权限授权面与桌面同构——单一真源 = 内核 `PermissionManager`（审批收窄与停用撤销不再被旁路）；
  宿主加固：插件下载补 egress 闸门、命令注册完整性、死代码与永不编译测试清理；新增 `host-notify`
  域（通知 / 震动 / 声音，ABI 18）。

### 桌面端

- 插件状态按线上 camelCase 读取——修好应用中心启停开关与整条状态判定链（漂移测试锁定）。
- 设备壳落地（`/shell` 路由——主屏 + 卡片切换，方案 C）；ACL schema 构建产物回归（移除
  exit / fullscreen-on-monitor 权限位）；Rust 编译 warning 清零；dev 插件前端 watch 延后到宿主
  首次构建完成（wasmtime 归档竞争）。

### 双端共享产物与工程

- **file-transfer 双端单 wasm 产物**（平台差异收敛为同 trait 双实现 + 运行期分支）+ 共享业务核
  （`bedcode-file-transfer-core`，ADR 0044）。
- 构建与工具链：sccache 上限重测（30 GiB）、`adaptive-run` 构建资源自适应包装器、leaf crate
  `Cargo.lock` 同步；文档体系收拢（两端 code-map 瘦身为目录级导航、`packages/` 索引、
  ADR 0042–0052）。
- 验证：两端宿主 cargo 套件、wasm-core（约 580 项 lib 测试）、移动端前端（约 1055 用例）与根
  ESLint 全绿；新增大量防回接 / 漂移锁并附变异自检。
