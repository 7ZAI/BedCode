# 01 — peer-net crate 骨架 + 节点身份

**What to build:** 建成共享 crate `bedcode-peer-net` 并被桌面端与移动端 src-tauri 以 path 依赖引入；每个 App 实例首次启动生成长效 Ed25519 节点身份——节点 ID 即公钥指纹、密钥跨重启稳定、重装即新身份；由该密钥经 rcgen 产出自签 TLS 证书。双节点测试 harness（进程内两实例、回环端口互指）随票建立，作为后续所有无头行为的主验证缝。

**Blocked by:** None — can start immediately.

**Status:** ready-for-agent

- [ ] 两端可编译并引入 crate；`cargo test` 通过
- [ ] harness 中两个节点各持身份：重启（重新加载持久化密钥）后节点 ID 不变
- [ ] 自签证书可生成且公钥与节点 ID 一致；证书绑定校验有单测
- [ ] 身份持久化遵循两端宿主现有的本地存储惯例
