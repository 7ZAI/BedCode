# 01: 预重构：五模块骨架归位

**What to build:** 桌面端插件子系统按内核五模块（core-config / core-monitor / core-security / core-plugin-manager / core-bus）物理归位：安全三件套（fs 三层校验、授权审批、互调门）迁入安全模块，加载/注册/监听/运行时迁入插件管理模块，消息总线迁入总线模块，配置与监控模块先立骨架。内核根 facade 成为唯一组合点。纯搬迁不改任何行为，外部消费方（Tauri 命令层等）经 facade 再导出，import 路径不变。

**Blocked by:** None (can start immediately)

**Status:** resolved

- [x] 五个模块目录/入口建立，现有文件全部归位，无残留旧路径
- [x] 模块间只经 facade 或 trait 注入协作，无新增横向耦合
- [x] `cd bedcode-desktop/src-tauri && cargo test` 全绿（与搬迁前基线一致）
- [x] 外部消费方 import 不经模块内部路径（走 facade 再导出）
- [x] 无行为变化：不增删任何功能、日志、错误分支

## Comments

- 2026-09-13 完成：commit fd031193e。五模块归位（security/manager/bus + config/monitor 骨架），facade 再导出，外部 4 处消费方改走 facade；cargo test 608 单测 + 全部集成测试绿；code-map 同步。
