# 05: core-bus：二进制载荷与背压

**What to build:** 插件间消息总线支持两种载荷格式：JSON 文本（现状，行为零回归）与二进制字节列（零 JSON 编解码，可传非 UTF-8 与大载荷）。订阅方声明格式偏好，格式不匹配的投递被拒绝并 warn；每订阅者有界队列，满则丢弃 + warn（plugin_id/topic 结构化字段）+ 丢弃计数进监控模块。WIT 契约新增二进制承载形式，ABI bump，桌面/移动双端契约同步演进（ADR 0019，wasmtime 47 不动）。

**Blocked by:** 01（五模块骨架归位）、03（丢弃计数进监控）

**Status:** ready-for-agent

- [ ] JSON 路径现有测试全部保留且全绿（零回归）
- [ ] 二进制 roundtrip：非 UTF-8 字节、大载荷（MB 级）收发字节一致
- [ ] JSON 订阅者收不到二进制消息（拒绝 + warn 断言）
- [ ] 队列满丢弃：丢弃数正确且进监控指标
- [ ] WIT 双端文件同步、ABI bump 记录（两端 wit 文件 diff 语义一致）
- [ ] `cargo test` 全绿
