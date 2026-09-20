# 04: session 插件 git 工作区能力下沉

**What to build:** 把当前僵尸形态的 git 业务（分支列表/状态/checkout 端点，桌面与移动端均无消费方）复活为 session 插件工作区 git 域的组件化能力：经 host-process（非交互进程原语）+ host-fs 实现，响应契约（含非 git 仓库判定语义）通过网关对外保持；宿主侧 git 业务模块与路由正式退役，将来加 UI 时直接在插件 API 面上接。

**Blocked by:** 01（网关地基与形状契约锁）

**Status:** ready-for-agent

- [ ] `/api/git/*` 响应契约保持（分支/status/checkout 语义、非 git 仓库判定、参数校验），无消费方不改变契约
- [ ] git 操作经插件 API 面可调用（插件侧契约测试锁语义）
- [ ] 宿主 git 业务模块与路由退役（contract），回滚仅剩 git revert
- [ ] 插件 httpEndpoints manifest 清单与插件分派表同源（契约用例锁定）
- [ ] cargo test / eslint 0 error；前端零改