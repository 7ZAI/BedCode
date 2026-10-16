# 票 02 — 发现引擎类型中枢：type_hub.rs + 注入缝 + engine browse 切换

Status: ready-for-agent
Type: task
Blocked by: 01

## 目标

新模块 `packages/bedcode-discovery-engine/src/type_hub.rs`（spec §2.1-§2.3 / §6 D1-D4）：

- **订阅表 fan-out**：`HashMap<service_type, Hub>`；订阅者 `Vec<(sub_id, flume::Sender<ServiceEvent>)>`；通道容量 64；`try_send`——`Full` → 丢弃事件 + 限频 debug；`Disconnected` → 惰性剔除
- **唯一 browse**：每活跃类型全进程恰好一条 `daemon.browse()`，只由中枢调用；新增订阅者不得再触发 browse
- **requery**：`request_type_requery(ty)` → 100ms 合并窗 → 重发 browse + receiver 换绑 + 缓存重放 → 广播全部订阅者；前缘吞并（窗内重复请求丢弃）；新订阅者加入 = 后缘补齐（最后一次加入后 ~100ms 执行一次 browse，保证每个新订阅者都拿到缓存重放——spec §2.1-⑤）
- **生命周期**：首个订阅建档（唯一 browse）；引用计数归零 → 立即 `stop_browse` + 移除（无 linger）；重建幂等
- **守卫退订**：订阅句柄 Drop → 移除发送端（通道关闭）；guard 丢失路径由 fan-out 的 `Disconnected` 惰性剔除兜底
- **注入缝**：daemon 交互抽微型 trait/闭包（生产 = `ServiceDaemon` 包装，测试 = fake），单测不触网（spec §2.3）
- **任务与容错**：中枢泵任务经 `DiscoveryPorts::spawn`；错误边界同 `spawn_with_error_boundary` 族；控制通道已关（任务退出）时下次 subscribe / requery 检测 → 以 drained 同路径重建（spec R7）
- **生命周期日志**（spec T5-③，提前落在此处，票 05 只核验）：`created` / `reused` / `join-rebrowse` / `requery` / `drained-stop`；结构化字段按 AGENTS §8
- **接口**（spec §2.3 逐字）：

  ```rust
  pub fn subscribe_type(service_type: &str) -> Result<MdnsBrowseSubscription, String>;
  pub fn request_type_requery(service_type: &str);
  ```

  `MdnsBrowseSubscription` 形态与票 01 的 `MdnsTypeSubscription` 对齐（私有字段 + `new` / `into_parts()`），供票 04 宿主 glue 转换；转换不得使 receiver / guard 生命周期脱钩

`packages/bedcode-discovery-engine/src/engine.rs` 切换（spec §2.3 / §3）：

- `browse()`（`:147` 一带）：删 `daemon().browse`，改 `subscribe_type`；`BrowserEntry` 增持订阅守卫
- `stop_browser_inner`（`:271`）：删 `daemon.stop_browse`，条目移除即守卫 Drop → 通道关闭 → 浏览任务 recv 断开退出；`mdns browse loop exited` 日志保留（退出形态与今日 stop_browse 断联一致）
- `publish_dir_event` / 属主仲裁 / `is_self_broadcast` / 句柄表 / 权限门全部不动
- 模块头注释 + `shared_daemon` 注释（「切勿在别处另建守护」段）补 listener 级不变量与中枢位置（spec §8 注释同步）
- `packages/bedcode-discovery-engine/Cargo.toml`：`+ flume = "0.12"`

## 单测（spec §7 矩阵 1-4、7，注入缝上做，零触网）

1. 同类型两次 subscribe：只发生一次 `daemon.browse`；两订阅者收到同一事件 —— 变异自检：退回「后到覆盖」实现必红
2. requery 换绑：旧 receiver 失效后，两订阅者仍收到换绑后事件与缓存重放
3. 合并窗：100ms 内两次 requery 只产生一次 browse；窗内加入 + 加入后缘补齐各 1 次
4. 守卫退订：drop 订阅句柄 → 通道关闭、订阅者移除；归零 → `stop_browse` 恰一次 + 中枢移除；重建幂等（重新 subscribe 恢复）
5. 惰性兜底：不经守卫直接 drop receiver → 下次 fan-out 剔除，不影响其他订阅者
7. 中枢重建：控制通道关闭（模拟任务退出）后再次 subscribe 能重建并 browse

## 门禁

- `cd packages/bedcode-discovery-engine && cargo test` 全绿（既有用例零回归）
- 变异自检（第 1 条 + 至少再一条）结果记入票面 Comments
