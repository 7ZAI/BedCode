# 12 — 切换 + 删除旧管线（contract）

**What to build:** expand–contract 的收口票：新对等链路通过功能对齐验收后，同一版本内删除旧宿主中心链路——终端 WS 上的文件服务控制面消息族、intent 协调层、旧 announce 机制及其宿主命令与 host functions 一并移除；file-transfer 插件仅保留对等链路路径。验证升级兼容：既有传输历史与本机配置在新版保留；双端真机全链路回归（发现/首连/互信迁移/推送/扇出/续传/浏览拉取/历史）。词汇表与 spec 中被删除机制的引用同步清理。

**Blocked by:** 09, 10, 11

**Status:** ready-for-agent

- [ ] 代码库中不再存在旧链路符号（编译级消失，非注释弃用）
- [ ] 全部相关测试清理或迁移后 `cargo test` 与两端 `test:run` 绿
- [ ] 真机回归清单（spec Further Notes 场景）逐项通过：含手机↔手机、桌面↔桌面、已配对自动互信
- [ ] 升级安装验证：历史与配置保留
- [ ] CONTEXT.md / ADR 引用一致性检查通过
