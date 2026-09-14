# 05 — 两端插件·可信对端管理分区

**What to build:** 用户可在 file-transfer 插件的设置界面查看全部可信对端并撤销任意一个：桌面端在设置覆盖层新增独立分区，移动端在设置二级页新增同功能分区；列表项含设备名、短指纹、加入时间；撤销前弹出后果说明确认框（撤销后对方重连需重新确认）。数据由 devMock 提供即可演示。

**Blocked by:** None — can start immediately

**Status:** resolved

- [x] 两端设置界面各新增「可信对端」分区：空态文案 + 列表（名称/短指纹/加入时间）
- [x] 撤销走两步：行内撤销按钮 → 后果说明确认框 → 确认后调撤销命令并从列表移除；取消无副作用
- [x] 列表加载失败如实呈现错误态而非空白
- [x] 移动端分区遵循 --mobile-* token 与触控规范；桌面端遵循 token-bound 与 --ui-scale 缩放（撤销按钮/提示行 scoped 样式自包含，修复跨组件 scoped 失效）
- [x] 撤销命令路由、列表加载有编排单测；i18n zh-CN / en 同步（命令名对齐已落地 Rust 代理契约：list-trusted / revoke-trusted）
- [x] devMock 可信对端种子可在两端 dev-shell 演示列表与撤销全流程（种子在插件工程 devMock 导出，dev-shell 只做通用接线）
- [x] 只改插件前端源码，不触碰任何 Rust 与宿主文件（lib.rs 先行桥接片段属 ticket 07 收口范围，本票未触碰）
