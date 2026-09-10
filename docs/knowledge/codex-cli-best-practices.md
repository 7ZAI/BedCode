# Codex CLI 编程最佳实践指南

> 本文档基于 OpenAI 官方文档（developers.openai.com）编写，帮助你高效使用 Codex CLI 提升编程效率。

---

## 目录

1. [Codex CLI 简介](#codex-cli-简介)
2. [环境配置与安装](#环境配置与安装)
3. [提示词最佳实践](#提示词最佳实践)
4. [AGENTS.md 持久化指引](#agentsmd-持久化指引)
5. [配置与权限管理](#配置与权限管理)
6. [MCP 服务器集成](#mcp-服务器集成)
7. [Skills 技能系统](#skills-技能系统)
8. [子代理与并行工作](#子代理与并行工作)
9. [会话管理与斜杠命令](#会话管理与斜杠命令)
10. [常见工作流示例](#常见工作流示例)
11. [常见错误与避免方法](#常见错误与避免方法)
12. [实用技巧与快捷键](#实用技巧与快捷键)

---

## Codex CLI 简介

Codex CLI 是 OpenAI 开源的终端编码代理，基于 Rust 构建，可在 macOS、Windows 和 Linux 上本地运行。它能够读取、修改和执行你机器上的代码，像一个可配置的编程队友。

**核心能力：**
- 读取和编辑本地代码文件
- 执行 shell 命令并查看输出
- 自动规划、测试和验证
- 支持图片输入（截图、设计稿）和图片生成
- 通过 MCP 连接外部工具
- 通过 Skills 封装可复用工作流
- 支持子代理并行处理复杂任务

**适用场景：** 代码解释、Bug 修复、编写测试、UI 原型开发、代码重构、文档更新、本地代码审查。

---

## 环境配置与安装

### 安装

```bash
# macOS / Linux
curl -L https://github.com/openai/codex/releases/latest/download/codex-installer.sh | bash

# Windows (PowerShell)
# 从 GitHub releases 下载安装器
```

### 升级

新版本发布频繁，升级只需重新运行安装器。

### Windows 注意事项

- 在 PowerShell 中原生运行（推荐使用 Windows 沙箱模式）
- 如需 Linux 环境，可使用 WSL2

---

## 提示词最佳实践

Codex 已经足够强大，即使提示词不完美也能产生有价值的结果。但清晰的提示词能让结果更可靠，尤其在大型代码库或高风险任务中。

### 四要素框架

好的提示词应包含四个要素：

1. **目标（Goal）：** 你要改变或构建什么？
2. **上下文（Context）：** 哪些文件、文件夹、文档、示例或错误信息与此任务相关？可用 `@` 引用文件。
3. **约束（Constraints）：** Codex 应遵循哪些标准、架构、安全要求或惯例？
4. **完成条件（Done when）：** 任务完成的标志是什么？测试通过？行为改变？Bug 不再复现？

**示例：**

```text
Bug: 点击设置页的"保存"按钮有时显示"已保存"但实际未持久化。

复现步骤:
1) pnpm run dev 启动应用
2) 进入 /settings
3) 切换 "启用通知" 开关
4) 点击保存
5) 刷新页面：开关恢复原位

约束:
- 不要改变 API 接口
- 修复最小化，添加回归测试

完成条件: 上述复现步骤不再触发 Bug，且相关测试通过
```

### 推理强度选择

根据任务难度选择推理强度：

- **Low：** 快速、范围明确的简单任务
- **Medium：** 日常复杂度的变更或调试
- **High：** 更复杂的多步骤变更
- **Extra High：** 长时间、高推理密度的代理任务

```bash
codex --model gpt-5.4 --reasoning-effort high "修复这个复杂的并发 Bug"
```

### 复杂任务先规划

对于复杂、模糊或难以描述的任务，先让 Codex 规划再实施：

- **使用 Plan 模式：** `/plan` 或 `Shift+Tab` 切换，让 Codex 收集上下文并提出澄清问题
- **让 Codex 采访你：** 告诉 Codex 先质疑你的假设，把模糊想法具体化
- **使用 PLANS.md 模板：** 对长时间多步骤工作使用执行计划模板

---

## AGENTS.md 持久化指引

`AGENTS.md` 是 Codex 的持久化指令文件，相当于给代理写的 README。Codex 在每次工作时自动加载它。

### 为什么重要？

将反复使用的提示词规则写入 `AGENTS.md`，而不是每次手动重复。这是从"一次性助手"升级到"可配置队友"的关键。

### 发现机制

Codex 按优先级加载指令：

1. **全局：** `~/.codex/AGENTS.md`（或 `AGENTS.override.md`）— 个人默认偏好
2. **项目级：** 从项目根目录到当前目录逐层扫描，下层覆盖上层
3. **合并顺序：** 从根到当前目录拼接，更靠近当前目录的文件后出现，优先级更高

### 好的 AGENTS.md 应包含

- 项目目录结构和重要目录说明
- 如何运行项目
- 构建、测试、lint 命令
- 编码惯例和 PR 期望
- 约束和禁止事项
- "完成"的定义和验证方法

### 分层示例

```
项目根/AGENTS.md              → 全团队标准
services/payments/AGENTS.override.md → 支付团队特殊规则
~/.codex/AGENTS.md             → 个人全局偏好
```

### 实用建议

- 使用 `/init` 命令快速生成初始 `AGENTS.md`
- 保持简洁实用，过长时拆分到子目录
- 当 Codex 重复犯同一错误时，做回顾并更新 `AGENTS.md`
- 默认大小限制 32 KiB，可通过 `project_doc_max_bytes` 调整

---

## 配置与权限管理

### 配置分层

```text
~/.codex/config.toml       → 个人默认配置
.codex/config.toml         → 项目级配置
命令行参数                  → 临时覆盖
```

Profile 配置：`$CODEX_HOME/profile-name.config.toml`，用 `--profile` 选择。

### 权限模式

Codex 的沙箱和审批是两个独立维度：

| 审批模式 | 说明 |
|---------|------|
| **Auto**（默认） | 允许在工作目录内读写和执行命令，超出范围需确认 |
| **Read-only** | 只能浏览文件，不做任何修改 |
| **Full Access** | 全机器权限，慎用 |

**建议：** 从默认权限开始，只在信任的仓库或明确的场景下逐步放宽。

### 沙箱模式

| 沙箱模式 | 说明 |
|---------|------|
| `read-only` | 只允许读取文件 |
| `workspace-write` | 允许读取文件和编辑工作目录内的文件 |
| `full-access` | 无限制，慎用 |

---

## MCP 服务器集成

MCP（Model Context Protocol）让 Codex 连接外部工具和数据源。

### 添加 MCP 服务器

```bash
# CLI 方式
codex mcp add <名称> --url <URL>

# 或在 config.toml 中配置
```

### 建议原则

- **只在解锁真实工作流时添加工具**，不要一开始就接入所有工具
- 从 1-2 个最常用的工具开始
- 常用 MCP 服务器：Playwright（浏览器自动化）、GitHub（代码管理）、Figma（设计集成）

---

## Skills 技能系统

Skill 是将可复用工作流打包为 `SKILL.md` 文件，包含指令、上下文和辅助逻辑。

### 何时创建 Skill？

**规则：** 如果你反复使用同一提示词或反复纠正同一工作流，就应该把它变成 Skill。

### 适合 Skill 的场景

- 日志排查
- 发布说明起草
- PR 审查（按检查清单）
- 迁移规划
- 事故/告警摘要
- 标准调试流程

### Skill 设计原则

- 每个 Skill 只做一件事
- 从 2-3 个具体用例开始
- 定义清晰的输入和输出
- 描述要说明 Skill 做什么以及何时使用
- 用 `$skill-creator` 技能快速创建第一个版本

### Skill 存储

- 个人：`$HOME/.codex/skills/`
- 团队共享：项目内 `.agents/skills/`（有助于新人入职）

---

## 子代理与并行工作

Codex 支持子代理（Subagent）工作流，可将任务拆分后并行执行。

### 使用原则

- 主代理聚焦核心问题，子代理承担探索、测试、排查等辅助任务
- 确保每个子代理的任务边界清晰、写作用域不重叠
- 不要把关键阻塞任务委派给子代理——在本地执行以保持关键路径

### 示例

```text
请并行完成以下工作：
1) 探索 src/auth/ 目录下的认证流程
2) 为 src/utils/transform.ts 编写单元测试
3) 检查最近的 CI 失败日志
```

---

## 会话管理与斜杠命令

### 关键斜杠命令

| 命令 | 说明 |
|------|------|
| `/plan` | 进入规划模式，先规划再实施 |
| `/review` | 对当前工作树做代码审查 |
| `/fork` | 从当前对话创建新分支线程 |
| `/compact` | 对话过长时压缩上下文 |
| `/resume` | 恢复之前的会话 |
| `/model` | 切换模型或调整推理强度 |
| `/permissions` | 切换审批模式 |
| `/clear` | 清屏但不结束对话 |
| `/goal` | 设定持久目标，Codex 据此判断任务是否完成 |
| `/init` | 在当前目录生成初始 AGENTS.md |
| `/exit` | 退出交互会话 |

### 会话恢复

```bash
codex resume            # 选择最近的会话恢复
codex resume --last     # 直接恢复最近的会话
codex resume <ID>       # 恢复指定 ID 的会话
```

### 线程管理建议

- 每个任务一个线程，而非每个项目一个线程
- 同一问题的相关工作保持在同一线程（保留推理轨迹）
- 工作真正分支时才 `/fork`

---

## 常见工作流示例

### 1. 解释代码库

```text
我需要理解这个服务的协议。阅读 @foo.ts @schema.ts 并解释 schema 和请求/响应流程。
重点关注必填 vs 可选字段以及向后兼容性规则。
```

### 2. 修复 Bug

```text
Bug: 点击"保存"后显示"已保存"但未实际持久化。

复现:
1) pnpm run dev
2) /settings
3) 切换开关
4) 保存
5) 刷新 → 开关恢复

约束: 不改 API 形状，修复最小化，加回归测试。
完成后重新运行复现步骤确认修复。
```

### 3. 编写测试

```text
为 @transform.ts 的 invert_list 函数编写单元测试。
覆盖正常路径和边缘情况。
遵循项目中已有测试的惯例。
```

### 4. 从截图原型开发

```text
基于此截图创建新的仪表盘页面。

约束:
- 使用 React + Vite + Tailwind，TypeScript 编写
- 尽可能匹配间距、字体和布局

交付:
- 新路由/页面渲染 UI
- 必要的小组件
- README.md 运行说明
```

### 5. 代码审查

```bash
codex
# 然后输入:
/review
# 或指定重点:
/review 重点关注边缘情况和安全问题
```

### 6. UI 迭代

先启动 dev server，然后用小而具体的提示词迭代：

```text
对首页提出 2-3 个样式改进方案。
```
选择方向后继续细化：

```text
采用方案 2。仅修改 header：字体更 editorial，增大留白，确保移动端适配。
```

---

## 常见错误与避免方法

| 错误 | 正确做法 |
|------|----------|
| 在提示词中堆砌持久规则 | 将规则移入 `AGENTS.md` 或 Skill |
| 不告诉 Codex 如何运行构建/测试 | 在 AGENTS.md 中明确列出命令 |
| 多步骤复杂任务跳过规划 | 先 `/plan` 再实施 |
| 过早给 Codex 全机器权限 | 从默认权限开始，逐步放宽 |
| 多个线程修改同一文件 | 使用 git worktree 或明确分工 |
| 工作流不稳定就自动化 | 先手动验证可靠性，再自动化 |
| 逐步骤盯着 Codex 操作 | 让 Codex 自主完成，你在旁做其他事 |
| 一个项目一个线程 | 一个任务一个线程 |

---

## 实用技巧与快捷键

### 输入技巧

| 操作 | 说明 |
|------|------|
| `@` + 文件路径 | 引用工作区文件作为上下文 |
| `!` + 命令 | 运行本地 shell 命令并将输出作为上下文 |
| `Ctrl+G` | 打开外部编辑器编写长提示词 |
| `Ctrl+R` | 在 composer 中搜索提示词历史 |
| `Tab`（Codex 运行中） | 排队下一条输入 |
| `Enter`（Codex 运行中） | 向当前轮次注入新指令 |
| `Esc Esc` | 编辑上一条用户消息，可回溯历史 |

### 启动技巧

```bash
codex                      # 在当前目录启动交互模式
codex "解释这个代码库"      # 带初始提示词启动
codex --cd <路径>            # 指定工作目录
codex --add-dir ../backend   # 添加额外可写目录
codex exec "修复 CI 失败"    # 非交互式运行（适合脚本/自动化）
```

### 环境准备建议

启动 Codex 前确保环境已就绪：
- 激活 Python 虚拟环境（或其他语言环境）
- 启动必要的后台服务
- 导出需要的环境变量

这样 Codex 不会浪费 token 去探测环境配置。

---

## 参考链接

- [Codex CLI 官方文档](https://developers.openai.com/codex/cli)
- [Codex 最佳实践指南](https://developers.openai.com/codex/learn/best-practices)
- [AGENTS.md 指引](https://developers.openai.com/codex/guides/agents-md)
- [Codex 工作流示例](https://developers.openai.com/codex/workflows)
- [Codex 提示词指南](https://developers.openai.com/codex/prompting)
- [Codex CLI 特性详解](https://developers.openai.com/codex/cli/features)
- [Codex 配置参考](https://developers.openai.com/codex/config-reference)

---

*文档生成日期：2026-07-06 | 基于 OpenAI 官方文档*
