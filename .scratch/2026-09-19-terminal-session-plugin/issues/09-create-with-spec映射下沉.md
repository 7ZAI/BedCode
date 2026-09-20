# 09: create-with-spec——命名唯一化与 config→launch 映射下沉

**父规格:** `.scratch/2026-09-19-terminal-session-plugin/spec.md` D4 / D3 / D8-P2

**What to build:** 「一个配置如何变成一个可启动的会话」这条决策线搬到插件：重名如何改写、配置到启动参数如何映射（含发行版与 shell 分支）、两阶段启动如何编排。宿主只负责按插件算好的启动规格执行，PTY 引擎与 shell 包装仍在内核。用户从桌面或移动端启动会话的结果与今天一致。

**Blocked by:** 08（配置真源已在插件，映射才谈得上归属）

**Status:** done（2026-09-20）

## 实施记录（2026-09-20）

### 1. WIT / SDK / 宿主契约（`host-session.create-with-spec`，v19 内函数级追加，ABI 不 bump）

- WIT `bedcode.wit` host-session 追加 `create-with-spec(spec-json) -> result<string, string>`；
  spec-json 契约（camelCase）：`{name, command, args?, cwd, cols?, rows?, env?, environment?, configId?, start?}`——
  `environment` 与宿主 `ExecutionEnvironment` serde 同形（`Wsl2{distro}`/`Linux`/`Windows{shell}`），
  `start` 缺省 true（false = 两阶段第一阶段）。宿主只做执行：shell 包装 / WSL 转换 / 尺寸缺省
  （缺省或 0 → 120x40，与 DefaultConfigMapper 基准一致）/ ID 预生成（异步创建，理由同 `create`——事件回灌死锁）。
- SDK：`HostSession` trait 追加 `session_create_with_spec`；`WasmHost` 实现。
- 宿主：`component.rs` 接线 + `host_impl/session.rs::session_create_with_spec`（权限门 `session:write` 先于一切
  + `resolve_launch_spec` 纯函数：解析 / 仲裁（空 name/command/cwd、非法 environment 显性报错）/ 映射）。
- `SessionManager::create_session_from_spec`（执行端）：不再读配置表 / 不命名 / 不映射——
  `start=true` 复刻 `create_session_with_source_and_id`（输出注册早于启动 + 启动失败回滚注销 + 正统端归属启动端 +
  Created 事件），`start=false` 复刻 `create_session_no_start`（Starting / 不启动 / 不注册输出管理器 / 无 Created）。

### 2. 插件侧编排（`plugins/session/rust/src/launch.rs`，新域）

- `generate_unique_name`：复刻宿主 `DefaultNamingService`（同配置活跃会话 base/`base(N)` 最大编号 + 1；Stopped
  不计数；多配置同名互不干扰）——命名唯一化策略下沉。
- `resolve_environment` / `build_launch_spec`：复刻 `DefaultConfigMapper` 分支（wsl2 distro 缺省 Ubuntu / linux /
  windows PowerShell / 空命令兜底 / 非法环境取值显性报错）——config→launch 映射下沉。
- `create_via_host`（wasm 入口）：读配置真源（私有库）→ 会话列表 → 命名唯一化 → 构建 spec →
  `host-session.create-with-spec` 执行 → `{sessionId}`（预生成 id）。
- 互调 api 面新增 `session-create`（入参 `{configId, cols?, rows?, start?}`）；plugin.json permissions 追加
  `session:write`（create-with-spec 权限位）；四处 pin 同步：plugin.json / 插件 `declares_only_landed_domain_surface`
  （15 项 api）/ 前端 `plugin-contract.test.ts` / 宿主 `test_session_plugin_artifact_lifecycle`。

### 3. 宿主命令面改造（两条创建路径经插件编排，行为等价）

- 新桥接 `utils/session_create_bridge.rs`（探活锚点 + JSON-RPC `session-create` + 降级语义，与 auth/config 桥接同构）：
  插件不可用 / 互调失败 → `Ok(None)` 让位宿主旧路径（无单点）。
- `commands/session.rs::start_session`（创建即启动，`start=true`）与 `create_session_no_start`（`start=false`）改为
  先经桥接，降级走既有 `create_session_with_source` / `create_session_no_start`。命令签名不变（仅注入 State）。

### 4. 测试与验证（全绿）

- 宿主 lib **1033 passed**（新增：`resolve_launch_spec_* 5 项` + `session_create_with_spec_* 2 项` + `SessionManager
  create_session_from_spec 等价对照 2 项` + 真实 wasm 闭环 `test_session_create_with_spec_closed_loop`）
- 宿主集成测试全绿（`pty_session_chain` 全链路配对→认证→WS→真 PTY 输出零改判 = 输出分发顺序不变量守住）
- 插件 crate **108 passed**（launch 域 20 项：重名冲突 / 多配置同名 / Stopped 不计数 / 非法环境取值 / 映射分支/
  两阶段 start 缺省）
- SDK 85 passed；前端 **74 files / 718 tests** 全绿（plugin-contract 更新为五域十五项）；eslint 0 error
- session 插件 wasm 产物已重建并入 `resources/plugins/desktop/com.bedcode.session/`

### 5. 教训

- **wasm 调用栈内 `?` 解 `Result<Option<Value>, HostError>` 与 native 链接差异**：`WasmHost.session_list()` 在 wasm 下
  返回 `Result<Option<Value>, HostError>`，`?` 不能把 `Option<Value>` 当 `Value`（E0308）且 `HostError→String` 需
  map_err（E0277）——插件 wasm 独有代码的编译错误只在 wasip3 构建时暴露，native `cargo test` 全绿不覆盖（wasm 功能
  分支被 cfg 隔离）。
- **宿主 wasm 闭环测试跨测试并行冲突**：`plugin_db_root()` 是进程级共享目录，wasm_runtime::tests 内多个测试并行
  各自 remove_dir_all + 写同一 SQLite 文件 → 我的闭环测试会 BUSY/文件缺失失败（单跑通过、全量偶发失败）。处置：
  arc 唯一引用期（实例化前）`Arc::get_mut` 注入**独立私有库根**，消除文件竞争。
- **`rustfmt <file>` 递归格式化 mod 树**：rustfmt 按 mod 声明递归处理子模块，`rustfmt session.rs` 会顺带格式化
  host_impl/* 与 wasm_runtime.rs 全文（含并发线在途文件）。今后只对自己新增文件用 `rustfmt --check`，确认差异属于
  本票代码再格式化；纯格式污染的非目标文件用 `git checkout` 还原（diff -w=0 验证无业务内容）。
- **权限五同步点**：新增 `session-create` api 需同步 `session:write` 权限（create-with-spec 的权限位），否则插件
  在宿主侧无权限执行创建（权限门直接拒绝）——本票补齐了 plugin.json + 测试 pin 四处。

### 6. 遗留与衔接

- 票 10（restart / remove / rename / resize 裁决下沉）解除阻塞，可开工。
- `host-session.create`（按 config_id 创建，v6）未被本票改动——auto-task 定时任务仍走它（配置读主库投影）；
  票 10 再评估是否收敛到创建编排。
- 移动端零改动（双端偏离 v19 桌面独有），移动端启动路径（server `/sessions/start`）仍走宿主旧路径——
  会话创建编排在桌面命令面生效，移动端行为不变。

---

- [x] `host-session` 追加以启动规格创建会话的原语（命令、参数、工作目录、尺寸、环境、名称），宿主只做 shell 包装 / 发行版转换 / 尺寸缺省 / ID 预生成
- [x] 命名唯一化策略、配置→启动规格映射、两阶段启动编排搬入插件，并具备单测（重名冲突、多配置同名、非法环境取值）
- [x] 宿主既有「创建即启动」与「只创建不启动」两条命令路径都改为经插件编排，行为等价（对照测试）
- [x] 输出分发相关顺序不变量不得破坏：注册输出消费者早于进程启动、启动失败回滚注销——由既有全链路测试守住，断言不改
- [x] 桌面全链路集成测试（配对 → 认证 → WS → 真 PTY 输出）不改断言即过
- [x] 桌面 `cargo test` + 插件 crate `cargo test` + `pnpm run test:run` 全绿
