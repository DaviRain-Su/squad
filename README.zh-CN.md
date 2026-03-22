[English](README.md)

# squad

**多 AI 智能体终端协作 — 通过简单的 CLI 命令实现。**

squad 让多个 AI CLI 工具（Claude Code、Gemini、Codex 等）通过 Shell 命令 + SQLite 进行通信。无守护进程、无后台进程 — 每条命令都是一次性操作。

## 快速开始

```bash
# 安装
cargo install --path .

# 安装 /squad 斜杠命令到已有的 AI 工具
squad setup

# 初始化项目工作区
squad init

# 在任意 AI CLI 终端中使用斜杠命令
/squad manager      # 终端 1
/squad worker       # 终端 2
/squad inspector    # 终端 3
```

或手动操作：

```bash
# 终端 1 — 启动管理者
squad join manager --role manager

# 终端 2 — 启动工作者
squad join worker --role worker

# 终端 3 — 启动审查者
squad join inspector --role inspector

# 管理者分配任务
squad send manager worker "实现 JWT 认证模块"

# 工作者检查收件箱
squad receive worker

# 工作者完成后汇报
squad send worker manager "完成：已在 src/auth.rs 添加 JWT 认证"

# 管理者转发给审查者
squad send manager inspector "审查认证实现"

# 审查者审查后汇报
squad send inspector manager "PASS: 认证模块没有问题"
```

## 命令一览

| 命令 | 说明 |
|------|------|
| `squad init` | 初始化工作区（创建 `.squad/` 目录） |
| `squad join <id> [--role <role>]` | 以 Agent 身份加入（role 默认为 id） |
| `squad leave <id>` | 移除 Agent |
| `squad agents` | 列出在线 Agent |
| `squad send <from> <to> <message>` | 发送消息（`@all` 广播给所有人） |
| `squad receive <id> [--wait]` | 检查收件箱（`--wait` 仅用于调试） |
| `squad pending` | 查看所有未读消息 |
| `squad history [agent]` | 查看所有消息历史（含已读） |
| `squad roles` | 列出可用角色 |
| `squad teams` | 列出可用团队 |
| `squad team <name>` | 查看团队模板 |
| `squad setup [platform]` | 安装 `/squad` 斜杠命令到 AI 工具 |
| `squad setup --list` | 列出支持的平台和状态 |
| `squad clean` | 清除所有状态 |

## 安装斜杠命令

```bash
squad setup           # 自动检测已安装的工具并安装
squad setup claude    # 只安装到 Claude Code
squad setup --list    # 查看支持的平台
```

| 平台 | 二进制 | 命令位置 |
|------|--------|---------|
| Claude Code | `claude` | `~/.claude/commands/squad.md`（斜杠命令） |
| Gemini CLI | `gemini` | `~/.gemini/commands/squad.toml`（斜杠命令） |
| Codex CLI | `codex` | `~/.codex/prompts/squad.md`（斜杠命令） |
| OpenCode | `opencode` | `~/.config/opencode/commands/squad.md`（斜杠命令） |

安装后，在任何执行过 `squad init` 的项目中使用 `/squad <角色>` 即可。

## 工作原理

Agent 通过共享的 SQLite 数据库（`.squad/messages.db`）通信。每个 Agent 在自己的终端中运行，使用 CLI 命令收发消息。

### 消息检查

Agent 完成任务后，检查新消息：

```
Agent 完成任务
  → squad send <id> manager "完成：摘要..."
  → squad receive <id>                     ← 检查新任务
  → 没有消息则继续其他工作
  → 稍后再次检查
```

## 角色模板

角色是 `.squad/roles/` 下的 `.md` 文件，定义 Agent 行为。内置三个角色：

- **manager** — 分解目标、分配任务、协调审查
- **worker** — 执行任务、汇报结果
- **inspector** — 审查代码、发送 PASS/FAIL 结论

自定义角色只需添加 `.md` 文件：

```bash
echo "你是数据库专家..." > .squad/roles/dba.md
squad join db-expert --role dba
```

## 团队模板

团队是 `.squad/teams/` 下的 YAML 文件，定义所需角色组合。使用 `squad team <name>` 查看。

## 许可证

MIT
