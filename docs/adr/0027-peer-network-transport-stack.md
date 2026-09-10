# 对等网络传输栈选型：对称化现有 HTTP+WS 而非引入新协议栈

## 背景

对等网络（任意 BedCode 桌面/移动节点在同一局域网内直连互联）需要选定传输栈。约束场景：两端都是自家 Rust App（Tauri），直连可达、无 NAT 穿透需求；数据面依赖已打磨三个版本的 Range 断点续传、上传 session 与批量批准语义；桌面端生产代码已运行 actix-web 4（HTTP+WS），v2.0.0 的移动端 actix 服务端曾完整走通双向上传下载（后被 v2.1「服务器归零」移除，git 可恢复）。

## 决定

采用**方案 A**：把现有 HTTP+WS 栈对称化为共享 crate `packages/peer-net`（`bedcode-peer-net`），两端 src-tauri 以 path 依赖引入。单 TCP 端口同时承载数据面（HTTP）与控制面（同端口 WS upgrade）。身份绑定：每节点首次启动生成 Ed25519 长效密钥对，rcgen 由此产出自签证书，连接时经自定义 rustls verifier 校验对方证书指纹即节点 ID，指纹在可信列表中才放行。实现种子取自 v2.0.0 移动端服务端代码的去 host 化改造，而非从零重写。

## Considered Options

- **QUIC（quinn）**：stream 多路复用与连接迁移在稳定局域网 WiFi 下收益边际；断点续传/上传会话/批量审批整套 HTTP 契约全部重写；JSON 帧协议需自造轮子且失去 curl 抓包调试能力；团队无 QUIC 生产经验。
- **libp2p**：mDNS 发现 + Noise 静态密钥 + yamux 与节点身份模型天然吻合，但其核心价值（NAT 穿透、DHT、gossip）在局域网直连场景全部用不上；依赖树巨大、API 历史变动频繁。「买整套工具箱只用一把螺丝刀」。
- **WebRTC DataChannel**：局域网可不依赖信令服务器，但存在自举悖论——SDP offer/answer 的交换通道（HTTP POST 到 mDNS 已发现的 IP）本身已是可用直连，WebRTC 反而重建第二条连接；DTLS 证书指纹校验工作量与方案 A 相同；webrtc-rs/libdatachannel 引入成本高。浏览器互操作无需求。

三者共同的否决理由：都会丢弃在 HTTP 契约上持续迭代的续传/会话/审批资产并重新踩坑。

## Consequences

- 不追求第三方互操作（无需与 LocalSend 等既有局域网传输工具互通），协议私有是接受的成本。
- 自签证书 verifier 为自维护组件，rustls 升级时需跟进 API 变化。
- 若未来开辟「不走自建中继的公网真 P2P」战场（NAT 打洞），届时另立 ADR 评估 QUIC/libp2p/WebRTC 作为并行通道，不影响本决定。
