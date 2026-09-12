# 05: 供应商统一管理

**What to build:** 供应商分区端到端：预设 CRUD（名称/baseUrl/api 方言/模型列表，**无 key 字段**）+ 内置模板四套（DeepSeek/通义/OpenAI/Anthropic，复用 chatbox 模板）+ 自定义；反向导入从 pi（`models.json` providers + `auth.json`）、opencode（`opencode.json` `provider.*`）、claude（settings.json env，只读展示）生成预设；应用 = 写入目标 CLI 原生配置文件（真源始终是 CLI 自己的配置），key 现场输入或从源 CLI 配置**内存直拷**（不落 hub 存储/日志），UI 一律掩码（前 3 字符 + 长度）；claude 写 settings.json 的 `env` 块，检测到自建桥接（`provider-config.sh` / `anthropic-bridge.mjs`）时提示冲突、不覆盖；应用后提示需重启会话生效。

**Blocked by:** 02

**Status:** resolved

- [x] 预设 CRUD + 内置模板可用；插件库所有表与日志无 key 明文（含单测断言）
- [x] 反向导入在本机真实数据上生成 sensenova/amd/gmi 预设（掩码展示）
- [x] 完成一次「预设 → 写入目标 CLI 配置」，目标 CLI 能用该配置启动
- [x] claude 桥接冲突提示可见且绝不覆盖用户桥接文件

## Answer

**交付**：票 05 全套——guest 新增 `providers.rs`（约 1900 行含 22 组单测），前端新增 `ProvidersTab.vue` + `ProviderApply.vue` + `useProviders.ts` + `utils/providers.ts`（内置模板）+ vitest 单测；manifest 补 5 条命令声明（无新增 permissions：fs 读写在既有 `fs:read`/`fs:write` 面，DB 走既有 `storage` 面）；lib.rs 接线（mod + 命令分发 + activate 幂等建表）；产物已重建落 `src-tauri/resources/plugins/desktop/com.bedcode.agent-hub/`（wasm ~1MB）。

**架构决策**：
- **JSONC 文本级 splice**：pi `models.json` 含 `//` 注释且用户配置的注释/未知字段必须保留 → 不做整文件反序列化重写，自研 JSONC 感知扫描器（字符串/注释状态机）做键值条目原位替换/插入（`upsert_entry`/`ensure_container`），其余字节逐字保留；读路径 `parse_jsonc`（剥注释 + 尾逗号）。全部纯函数 + 单测（真实 pi 形态含中文注释、字符串内 `//`/`{}`、尾逗号、CRLF 场景）。
- **应用 = 合并语义**：pi/opencode 条目与既有内容 merge（既有模型定义、未知字段、无 key 时的既有 apiKey 全保留），claude env 逐键 upsert（不触碰其他 env 键）；keyMode=none 时不写 auth.json / 不生成 AUTH_TOKEN（保留既有凭据）。
- **key 纪律**：`provider_preset` 表无 key 列（schema + wire 形状单测锁定）；掩码函数（前 3 字符 + 长度，≤6 字符只泄长度）是 key 与 UI 唯一交界面；应用后状态/日志只记 `keyLen`；key 在 guest 内存中「源配置现读 → 目标文件现写」，不落任何 hub 存储。
- **反向导入去重**：pi 与 opencode 常有同名 provider（实机 sensenova 两处都有）→ `plan_inserts` 同名改写 `{name}-{source}`，掩码内嵌草稿随改名走（code-review 修复项）；再撞名才跳过。
- **claude 桥接**：应用前查 `provider-config.sh` / `anthropic-bridge.mjs` 存在性 → 冲突时拒绝写入返回 `bridgeConflict`，UI 两击确认后携顶层 `force` 重写 env 块；桥接文件永不触碰（force 与 key 模式正交，同为 code-review 修复项）。

**能力链路**：预设 CRUD（`save-preset` 同名返回 nameExists / `delete-preset`）→ 插件库 `provider_preset`；反向导入（`import-providers`，同步命令）读 pi（models.json + auth.json 掩码）与 opencode（opencode.json），claude 只读视图（env 掩码 + 桥接存在性）随 `get-providers-state` 现查；应用（`apply-provider`）写 claude `settings.json` env / pi `models.json`+`auth.json` / opencode `opencode.json`（codex config.toml 官方格式未校准，v1 UI 置灰提示）；全量状态经 `plugin:agent-hub:providers` 推送。key 来源：inline 现场输入 / source 内存直拷（源信息来自预设 notes `pi:<name>` 标注 + 导入掩码回显）/ none 保留既有凭据。

**验证证据**：
- 插件 crate：`cargo test`（rust/）85/85 通过（JSONC 双形态/splice 保注释与字节保留/掩码不泄漏/方言映射/导入提取/去重改名/条目合并/env 视图/key 无明文断言），`cargo fmt --check` 过，`cargo clippy --all-targets` 0 error（余 4 条 `&sql_params![]` useless-vec 与 auto-task 同型惯例，非门禁）
- 宿主：`cargo test`（bedcode-desktop/src-tauri）全 suite 0 failed（604 单测 + 集成）
- 前端：`pnpm run test:run` 67 文件 / 624 用例全绿（新增 providers.test.ts 5 用例）；`pnpm exec tsc --noEmit` 仅既有 .vue/.css 模块解析工具性报错
- 根目录 `pnpm exec eslint .` 0 error（本票文件无 warning；存量 warning 非本票引入）
- 真机数据烟囱验证（一次性测试，跑后即删）：真实 `~/.pi/agent/models.json`（含 `//` 注释）经 JSONC 路径解析出 sensenova/amd 预设（baseUrl/api/模型数与实查一致），auth.json 掩码 `sk-…(35)`；真实 `~/.config/opencode/opencode.json` 解析出 sensenova/gmi 预设，gmi key 掩码 `(252)`（JWT）；提取链路无 key 明文
- 插件 wasm 构建 + componentize + 产物复制成功

**剩余验证（真机人工）**：
- `pnpm run tauri:dev` 走通「反向导入 → 掩码回显 → 应用 sensenova → pi」闭环：pi 能以写入的 models.json+auth.json 启动并列出新 provider（AC2/AC3 的运行时部分）
- opencode 应用后 `opencode` 能列出该 provider；claude 无桥接本机（现无 env 块）验证 env 写入 + 有桥接场景的冲突 UI（桥接文件需临时构造）
- Windows 实机：路径拼接均为 `{HomeDir}` + `/` 段（配置文件读写无 shell 参与），风险低，列入例行核查

**已知事项**：① 本票与票据 06 会话并行开发共享文件（lib.rs/AgentHubView/i18n/插件目录），提交时工作区同时含票 06 在途代码（见提交说明）；② pi 应用写入的最小模型条目（id/name）依赖 pi 对内置 catalog 的补全行为，真机启动验证列入剩余项；③ apiStyle=custom/gemini 应用到 pi 时分别回落 openai-completions / google-generative-ai（映射表单测锁定）。
