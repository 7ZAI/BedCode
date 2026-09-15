# 07 — i18n 与验收清单扫尾

**What to build:** 收尾验收：zh-CN / en 文案 key 全同步，spec §7 验收标准逐项打勾（含「不存在绕过隔离区的删除路径」code review 专检），三套测试全绿，插件达到可交付状态。

**Blocked by:** 06

**Status:** ready-for-agent

- [ ] 所有用户可见文案 i18n key 同步出现在 zh-CN 与 en
- [ ] spec §7 验收标准逐项核对打勾，「无绕过隔离区删除路径」作为 code review 专检执行
- [ ] 插件 `cargo test`、宿主 `cargo test --lib`、桌面端 `npm run test:run` 全绿
- [ ] vue-tsc 干净
