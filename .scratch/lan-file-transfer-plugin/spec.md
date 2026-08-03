# BedCode 内网文件传输插件（桌面 + 移动）实现规格

> 本规格由 Wayfinder 地图的全部决策票据汇编而成（见 `map.md` 与各 `issues/`）。实现者无需再做重大决策；文中标注【开放问题】的条目在实现前按 fog 区说明处理即可。

## 1. 概述与目标

在内网 WiFi 环境下，BedCode 桌面端与移动端之间双向传输文件（音乐/电影/文档等），以**两个插件**（桌面一个、移动一个）实现，依托两端宿主新增的**通用受控文件服务**宿主能力。

**硬性要求**：
- 高性能：目标打满 WiFi 链路（WiFi 6 实测 50–110 MB/s 量级）；文件大小不设上限（几十 GB 级），全程流式传输，禁止整文件内存加载
- 断点续传：中断后从字节偏移继续，不整文件重来
- 双向互访：两端各自作为文件服务方；**禁止删除、重命名、移动、覆盖**；仅可访问使用者显式配置的允许目录
- 安全：复用宿主认证；传输明文（预留加密缝），界面明示仅限可信内网
- UI：现代审美，跟随各端系统风格与宿主主题

**核心约束**：插件业务代码不入侵宿主；宿主只提供通用能力（通用文件服务、加密缝），文件传输插件是该能力的第一个消费者。

## 2. 范围

**In scope**：两个文件传输插件 + 两端宿主的通用文件服务能力 + 复用现有配对连接/JWT 认证。

**Out of scope**（已决排除，详见 map.md）：对端发现（mDNS）、覆盖/重命名/移动/删除远端文件、独立认证或独立认证端口、限速/定时传输/传输历史长期留存、跨外网传输、访问审计日志、E2E 加密的实际实现（只留缝）、多设备互访。

**前提**：一切基于已建立的配对连接，无发现机制。

## 3. 总体架构

```
桌面插件(TS+WASM)                        移动插件(TS+WASM)
 ├─ UI（浏览/队列/设置）                  ├─ UI（工具箱入口/浏览/队列/设置）
 ├─ 任务状态机 + storage 持久化           ├─ 任务状态机 + storage 持久化
 └─ context.fileService.mount(...)       └─ context.fileService.mount(...)
        │ 配置+策略钩子（数据不经 WASM）          │
        ▼                                        ▼
桌面宿主：现有 actix server 挂子路由      移动宿主：新增 actix-web 轻量服务
 （自动经 JWT 中间件）                    （独立端口，随挂载启停，Bearer Token）
        └──────────── 数据面 HTTP/1.1 + Range ────────────┘
        └──────────── 控制面：现有 WebSocket（在线/任务状态推送）────┘
```

- **数据面**：HTTP/1.1 + Range（已决协议）。下载用标准 Range；上传用 upload session 模型。
- **控制面**：对端在线感知、服务端口/token 公告、任务状态推送走**现有 WebSocket**，不轮询、不开新连接。
- 两端插件后端均为 WASM（桌面 wasmtime / 移动端 wasm_runtime），SDK 接口同构。

## 4. 宿主通用文件服务能力（两端各自实现，不建共享 crate）

### 4.1 SDK 接口（两端同构）

```ts
// manifest 权限：fileservice（未声明则拒绝挂载）
const mount = await context.fileService.mount({
  mountPath: 'files',                     // 宿主暴露为 /plugins/{pluginId}/files/**
  roots: string[],                        // 允许目录根（绝对路径，来自插件 storage）
  operations: ('list' | 'download' | 'upload')[],
  onUploadRequest: (meta: { relativePath: string; size: number })
      => Promise<{ allow: boolean; reason?: string }>   // 策略钩子
})
await mount.updateRoots(newRoots)         // 目录变更即时生效
mount.dispose()                           // deactivate 时摘除
```

- 挂载随插件生命周期：`activate()` 挂载、`deactivate()`/停用/卸载立即摘除；宿主重启后插件重新激活即重挂。
- URL 命名空间按插件 id 隔离。

### 4.2 策略钩子

- MVP 仅 `onUploadRequest`（接口预留扩展）。
- 仅在**上传会话创建时调用一次**（相对路径 + 大小），拒绝发生在写任何字节前。
- 同步阻塞上传握手，**2 秒超时；超时/插件异常一律拒绝（fail-closed）**。
- 「同名即拒」由插件在钩子内实现（目标目录存在同名文件 → `{ allow: false, reason: 'duplicate-name' }`）。

### 4.3 目录沙箱（宿主强制）

1. 挂载时：root 必须存在、是目录、通过宿主 fs 授权，否则拒绝挂载；重复/嵌套 root 去重取最外层。
2. 请求时：路径规范化；`..` 一律拒绝；最终路径 `canonicalize` 后必须仍在某 root 前缀内（防 symlink 逃逸），否则 404。
3. 大小写按文件系统实际语义（canonicalize 后比较）。
4. root 失效（删除/移动/权限回收）：该 root 下线、其余正常；列举返回明确错误。
5. 隐藏/系统文件宿主不过滤；浏览列表过滤 `.bedcode-upload-*.part` / `*.part` 临时文件。

### 4.4 端点形状（HTTP/1.1）

| 端点（挂载点下相对路径） | 方法 | 说明 |
|---|---|---|
| `/list?path=…` | GET | 目录列举（名称/大小/mtime/类型），路径沙箱校验 |
| `/file?path=…` | GET | 下载，支持 `Range: bytes=N-`（206）；`HEAD` 返回 size+mtime（指纹） |
| `/upload` | POST | 创建 upload session：body 含目标相对路径+大小 → 钩子校验（同名即拒在此）→ 返回 sessionId 与已收偏移（0） |
| `/upload/{sessionId}` | PUT/GET | PUT 从服务端已收偏移 append；GET 查询 session 状态（已收字节数），用于续传握手 |
| `/upload/{sessionId}/complete` | POST | 完成：临时文件原子 rename 到目标名 |
| `/upload/{sessionId}` | DELETE | 取消：清理临时文件 |

- 下载侧可直接基于 actix-files `NamedFile` 的 Range 能力包装；上传侧宿主管理临时文件。
- 临时文件：目标目录内 `.bedcode-upload-{sessionId}.part`（同卷保证 rename 原子落位，避免跨卷双倍 IO）。
- session TTL：24 小时无活动宿主清理（宿主侧可配置）；宿主启动时扫描清理孤儿 `.part`。

### 4.5 认证

- **桌面端**：子路由挂在现有 actix server，自动经过现有 JWT 中间件，零额外认证开发。
- **移动端**：宿主新起 actix-web 服务（独立端口），只认宿主登记的 Bearer Token。Token 引导：移动服务启动时生成 token，经**已认证的现有 WS** 推送给桌面端存储携带；配对解除即失效。反向（移动调桌面）继续用现有 JWT。
- 生命周期：首个文件服务挂载时启动移动服务，最后一个摘除时关闭；端口经 WS 控制面公告给对端。

### 4.6 加密拦截预留缝（MVP 空实现）

两端宿主的文件服务传输层预留 **TransportCipher seam**：对上传/下载字节流的加/解密拦截接口。MVP 注入空实现（直通）；未来接入 E2E 加密（X25519 + AES-GCM）时不动传输主流程。

## 5. 插件设计（两端同构，业务全在插件）

### 5.1 清单

```jsonc
{
  "id": "com.bedcode.file-transfer",      // 已定：两端同 id（与现有内置插件惯例一致）
  "permissions": ["storage", "fileservice", "ui 扩展点权限(按端)"],
  "contributes": { /* 桌面：view/设置区；移动：ui:toolbox + 设置区 */ }
}
```

入口已定：桌面端在**侧边栏菜单新增一项**（桌面 SDK `SidebarPanelDescriptor`：id/title/icon/order/component，排在插件菜单区）；移动端工具箱入口用 `ui:toolbox`。
【已定】分发方式：**两端内置**（移动端 APK assets + 桌面端内置资源，与现有 `ai-chatbox`/`auto-task` 同级待遇，首启自动解压即用）；保留现有 zip 安装通道供后续独立更新。

### 5.2 插件职责

- 允许目录列表（用户配置）存插件 storage；设置区增删 → `updateRoots` 即时生效。
- 传输任务的状态机、队列调度（并发槽）、偏移持久化、UI 全部在插件。
- 控制面：经宿主现有 WS 通道收发对端在线、端口/token 公告、任务状态推送（宿主需暴露给插件的 WS 消息钩子按现有 events/message_bus 机制，缺口实现期补齐——属宿主基础功能完善，允许）。

## 6. 传输规格

- 协议：HTTP/1.1；下载 `Range: bytes={offset}-`；上传 session 模型（创建→append→complete）。
- 并发：多文件并发 = 并行连接，**默认 3，可配置，上限初拟 8**；MVP 不做文件内分片（基准不达标时的升级路径是"文件内分片并发"，仍在 HTTP/1.1 内，不切协议）。
- 吞吐实现要点：tokio 流式文件 IO，缓冲 256KB–1MB。
- 大文件：全程流式；进度/速率/剩余时间由插件计算。

## 7. 任务状态机与持久化

### 7.1 状态机

`queued` / `transferring` / `paused` / `resumable` / `completed` / `failed` / `rejected`（终态，reason=duplicate-name）/ `cancelled`

迁移：queued→transferring（槽位空出）；transferring→paused（用户）/ resumable（断线、对端下线，自动）/ completed / failed；创建被拒→rejected；paused|resumable→transferring（恢复）；除 completed 外→cancelled（清理临时文件与偏移）。

### 7.2 恢复触发

- 重连（App 未重启）：**自动续传**，UI 显示"已自动续传"，可取消。
- App 重启：paused/resumable 恢复到列表但**不自动传**，提供"全部继续"。

### 7.3 持久化（插件 storage，key `transfer-tasks`，JSON 数组）

字段：`id, direction(upload|download), peer{deviceId,name}, remotePath(相对挂载点), localPath, size, offset, uploadSessionId(仅上传), fingerprint{size,mtime}, state, reason, createdAt, updatedAt`

写入：传输中每 1s 节流；状态迁移立即写；宿主 shutdown 信号强制 flush。
保留：仅 paused/resumable 跨重启；completed/cancelled/rejected/failed 当次会话可见，重启清除。

### 7.4 续传握手与有效性

- 下载：先取远端指纹（HEAD size+mtime）比对；不匹配→failed（reason=remote-changed）可一键重新排队；匹配→Range 续传。
- 上传：查 session 已收字节；一致→append；**session 丢失→自动重建从头传**（用户无感知，与同名规则不冲突）；本地源指纹变化→failed。
- 下载的同名检查由发起方在本地做（目标路径已存在即拒绝排队）；完成 rename 竞态失败→rejected/duplicate-name，保留 `.part` 供用户决定。

## 8. 安全模型

- **信任模型**：配对 + 允许目录白名单，无第二层开关。停用插件 = 服务消失。
- **默认安全**：新装插件共享列表为空；对端看到「对方尚未设置共享目录」。
- **目录沙箱**：见 4.3（宿主强制，插件无法绕过）。
- **只放入与取出**：服务面无删除/改名/移动/覆盖端点；上传同名即拒（钩子）。
- **传输加密**：明文 + 设置区常驻告知文案；TransportCipher 缝预留（4.6）。
- **审计**：不做（MVP）。

## 9. UI 规格

原型（一次性，不进生产）：`prototypes/desktop-ui/index.html`（#A）、`prototypes/mobile-ui/index.html`（#T / #A）。

### 9.1 桌面端 —— Variant A 双栏工作台

- 顶栏：对端 pill（在线绿点+设备名）｜面包屑｜「下载所选 (N)」(主按钮)「发送到手机…」「刷新」「设置」
- 左栏：目录表格（复选框多选、类型图标、名称、大小、修改时间）
- 右栏（360px 常驻）：状态汇总 chips（N 传输中/排队/失败/同名被拒）+ 任务卡（方向+文件名+暂停/取消+进度条+已完成/总量·速率·剩余时间）
- 增强项（非硬要求）：右键菜单、拖拽发送
- 入口：侧边栏新增菜单项（已定）

### 9.2 移动端 —— 工具箱入口块 + Variant A 浏览为主

- 入口：工具箱长条块卡片——渐变圆角图标（⇄）+「文件传输」+ 副标题 + 右侧实时状态角标（"2 传输中"，WS 推送）；不占底部导航
- 页面：顶栏（返回+标题+对端名+在线点）→ 面包屑 → Material 文件列表（图标+名称+元信息+多选勾选）→ 多选时底部主按钮「⬇ 下载到手机（N 项 · 总大小）」→ 迷你传输条（常驻：当前任务+细进度+速率）→ 点击展开 bottom sheet 完整队列
- 同名被拒：队列紫色 chip + 发起时即时 Material 对话框（标题"无法上传"，单按钮"知道了"，`context.dialogs`）
- 通知：全部完成/失败经 `context.notifications`
- 后台行为（已定）：**尽最大努力存活**——利用 Android 允许的保活机制（如前台服务通知）维持传输；仍**接受中断**，中断后依赖断点续传兜底（任务自动转 resumable，重连/回到 App 恢复）

### 9.3 状态呈现（两端一致的四色体系）

传输中（蓝/绿）｜已暂停（琥珀，▶ 恢复）｜失败（红，原因文案 + 重新排队）｜同名被拒（紫，"目标目录已存在同名文件，无法上传"）｜排队（灰）

## 10. i18n（zh-CN 与 en 同步新增）

| key | zh-CN | en |
|---|---|---|
| `transfer.error.duplicateName` | 无法上传：目标目录已存在同名文件 | Upload failed: a file with the same name already exists in the target folder |
| `transfer.error.remoteChanged` | 远端文件已变化，无法续传，请重新传输 | The remote file has changed and can't be resumed. Please start over. |
| `transfer.error.dirUnavailable` | 该目录当前不可用 | This folder is currently unavailable |
| `transfer.settings.plainWarning` | 文件在本局域网内明文传输，请仅在受信任的 WiFi 网络中使用 | Files are transferred unencrypted on your local network. Only use this on trusted WiFi. |

其余界面文案按 `{domain}.{section}.{key}` 规范在实现时补齐（composable 中用 `i18n.global.t()`，禁止中文硬编码）。

## 11. 建议实现顺序

1. 桌面宿主：通用文件服务能力（挂载注册表 + 沙箱 + Range 下载 + upload session + 钩子协议 + TransportCipher 空实现）
2. 移动宿主：同款能力 + 独立服务启停 + Bearer Token 与 WS 引导 + WS 控制面消息
3. 两端 SDK：`context.fileService` API + `fileservice` 权限
4. 插件：任务状态机 + 持久化 + 续传握手（先桌面→移动下载打通全链路）
5. 插件 UI（按 9 节原型）+ 设置区 + i18n
6. 基准测试：按第 12 节《性能验收基准》执行；不达标则启用文件内分片升级路径

## 12. 收尾定案与性能验收基准

原开放问题已全部定案：

- **插件分发方式**：**内置**——移动端 APK assets + 桌面端内置资源，与现有内置插件（ai-chatbox/auto-task）同级待遇，首启自动解压即用；同时保留现有 zip 安装通道供后续独立更新。
- **桌面端入口**：侧边栏菜单新增菜单项（见 5.1/9.1）。
- **Android 后台存活**：尽最大努力存活 + 接受中断 + 断点续传兜底（见 9.2）。
- **插件 id**：两端同名 `com.bedcode.file-transfer`。

### 性能验收基准（v1）

方法依据：业界通用做法——iperf3 测链路理论上限作分母，应用层文件传输对比验收（NAS/SMB 评测同法；应用层实测通常为链路上限的 70–90%）。

**环境标定**（每次验收先做，结果写入报告，否则绝对数字无意义）：
1. `iperf3 -P 4 -t 30`（多流；Windows 上单流有已知偏差，必须多流或改用 nttcp）分别测桌面→移动、移动→桌面两方向 TCP 上限，记为 **T**
2. 大文件顺序读写测两端磁盘速度，记为 **D**（慢盘可能才是真瓶颈）
3. 记录 WiFi 规格（如 WiFi 6 5GHz）

**验收项**：

| # | 项目 | 通过标准 |
|---|---|---|
| 1 | 单大文件吞吐（10GB 级，双向各测） | ≥ **80% × min(T, D)**；报告实测 MB/s 与占比 |
| 2 | 多文件并发吞吐 | 默认并发数（3）混合队列，聚合吞吐 ≥ 75% × min(T, D) |
| 3 | 断点续传正确性 | 10GB 文件传至 30%/60%/90% 时分别强制中断（关 App/断 WiFi/锁屏），恢复后续传完成，**哈希与源文件一致** |
| 4 | 并发抢占与恢复 | 传输中改并发数、对端插件停用再启用，队列状态正确无僵尸任务 |
| 5 | 内存稳定性 | 传输全程宿主+插件内存增长 ≤ 100MB（证明流式、无整文件加载） |

不达标时的升级路径：文件内分片并发（仍在 HTTP/1.1 内，不切协议）。

## 13. 开放问题

**无** —— 全部决策已定案，规格可直接交付实现。
