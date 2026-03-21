[English](README.md)

# squad

**多 AI 智能体终端协作工具 — 让 Claude Code、Codex、Gemini 等自动协同工作。**

squad 运行一个本地守护进程，通过 MCP、hook 脚本或文件监听等方式在 AI 命令行智能体之间路由消息，并通过循环、流水线或并行工作流进行协调。

> **平台要求：** squad 需要 **macOS 或 Linux**，暂不支持 Windows。

---

## 快速开始

```bash
# 1. 安装（需要 Rust — https://rustup.rs）
git clone https://github.com/mco-org/squad.git
cd squad
./install.sh          # 构建并安装 squad、squad-mcp、squad-hook

# 或使用 cargo 直接安装：
cargo install --git https://github.com/mco-org/squad

# 2. 初始化项目工作区
cd my-project
squad init            # 生成包含 builder + reviewer 模板的 squad.yaml

# 3. 将 squad 注册到你的 AI 智能体（以 Claude Code 为例）
squad setup cc --update-claude-md

# 4. 启动守护进程
squad start

# 5. 以目标启动工作流
squad run "refactor the auth module"

# 6. 实时查看进度
squad watch
```

> **提示：** 随时运行 `squad doctor` 检查守护进程是否在线、`squad-mcp` 是否在 PATH 中、以及 `.mcp.json` 配置是否正确。

---

## 核心概念

### 守护进程（Daemon）

`squad` 守护进程是运行在工作区的后台进程，负责管理智能体注册、消息路由、心跳检测、工作流状态和持久化。所有智能体均通过守护进程的 Unix socket（`.squad/squad.sock`）进行通信。

### MCP 服务器

`squad-mcp` 是一个 Model Context Protocol 服务器，AI 智能体（Claude Code 等）通过它连接到 squad。它暴露以下四个工具：

| 工具 | 说明 |
|------|------|
| `send_message` | 向另一个智能体发送消息 |
| `check_inbox` | 从守护进程邮箱获取消息 |
| `mark_done` | 记录任务完成并推进工作流 |
| `send_heartbeat` | 通知守护进程当前智能体仍在线 |

### 工作流（Workflow）

工作流是定义在 `squad.yaml` 中的一系列步骤（或步骤集合）。每个步骤将一个动作分配给一个智能体，工作流引擎根据智能体 `mark_done` 的结果在步骤之间路由执行。

### 适配器（Adapters）

适配器是守护进程与非 MCP 智能体通信的方式：

| 适配器 | 机制 |
|--------|------|
| `mcp`（默认） | 智能体通过 `squad-mcp` MCP 服务器连接 |
| `hook` | 守护进程调用 shell 脚本并设置 `$SQUAD_MESSAGE` 环境变量 |
| `watch` | 守护进程写入文件，智能体读取并覆写该文件 |

---

## 配置参考

完整 `squad.yaml` 示例：

```yaml
project: my-project

heartbeat_timeout_seconds: 30

persistence:
  enabled: false

recovery:
  on_agent_offline: reconnect   # reconnect | restart | notify | ignore
  reconnect_attempts: 3
  reconnect_interval_seconds: 5

agents:
  builder:
    adapter: mcp                # mcp | hook | watch

  reviewer:
    adapter: hook
    hook_script: .squad/hooks/reviewer.sh

  codex:
    adapter: watch
    watch_file: .squad/codex-output.txt

workflow:
  mode: loop                    # loop | pipeline | parallel
  start_at: implement
  max_iterations: 10
  on_timeout: stop              # stop | notify | restart
  timeout_seconds: 300

  steps:
    - id: implement
      agent: builder
      action: implement
      message: "Goal: {goal}\n\nPrevious output:\n{previous_output}"
      on_pass: review
      on_fail: implement

    - id: review
      agent: reviewer
      action: review
      message: "Review iteration {iteration}:\n{previous_output}"
      next: done
```

### 模板变量

步骤的 `message`（别名：`prompt`）字段支持以下变量：

| 变量 | 含义 |
|------|------|
| `{goal}` | 传入工作流的初始目标字符串 |
| `{previous_output}` | 上一步骤 `mark_done` 的摘要内容 |
| `{iteration}` | 当前迭代次数 |

---

## CLI 命令

| 命令 | 说明 |
|------|------|
| `squad init` | 创建 `squad.yaml` 模板（文件已存在则跳过） |
| `squad init --force` | 覆盖现有 `squad.yaml` 并清除历史记录 |
| `squad setup <agent>` | 为指定智能体注册 squad MCP 服务器（`cc`、`codex`、`gemini`、`qwen`） |
| `squad setup --list` | 列出支持的智能体 |
| `squad start` | 在后台启动守护进程 |
| `squad run <goal>` | 以指定目标启动工作流 |
| `squad stop` | 优雅停止守护进程 |
| `squad status` | 显示守护进程状态和智能体健康状况 |
| `squad doctor` | 诊断守护进程、`squad-mcp` 和 `.mcp.json` |
| `squad log` | 打印审计日志 |
| `squad log --tail N` | 显示最后 N 条审计记录 |
| `squad log --filter key=val` | 按字段过滤日志 |
| `squad history` | 显示工作流会话历史摘要 |
| `squad clean` | 删除运行时状态（消息、会话、审计） |
| `squad watch` | 打开实时 TUI 仪表板 |

---

## TUI 仪表板

按 `q` 退出。

```
┌─ squad — my-project ─────────────────────────────────────────────────────┐
│ mode: loop  step: implement  iteration: 3/10  running: true              │
├──────────────────────────────────┬───────────────────────────────────────┤
│ Agents                           │ Messages                              │
│                                  │                                       │
│ builder    [working] online      │ workflow -> builder                   │
│ reviewer   [idle]    online      │   Goal: refactor auth module          │
│                                  │                                       │
│                                  │ workflow -> reviewer                  │
│                                  │   Review iteration 2: ...             │
│                                  │                                       │
└──────────────────────────────────┴───────────────────────────────────────┘
```

---

## 支持的智能体

任何能充当 MCP 客户端的 AI 命令行工具均可开箱即用：

| 智能体 | 适配器 | 说明 |
|--------|--------|------|
| Claude Code（`claude`） | `mcp` | 在 `~/.claude/settings.json` 中添加 `squad-mcp` 作为 MCP 服务器 |
| OpenAI Codex CLI | `hook` 或 `watch` | 使用 shell 脚本或文件监听 |
| Gemini CLI | `hook` 或 `watch` | 与 Codex 相同 |
| 任意 CLI 工具 | `hook` | 以 `$SQUAD_MESSAGE` 运行任意命令 |
| 文件型智能体 | `watch` | 智能体读写共享文件 |

### 通过 MCP 接入 Claude Code

在 `~/.claude/settings.json`（或工作区 `.claude/settings.json`）中添加：

```json
{
  "mcpServers": {
    "squad": {
      "command": "squad-mcp",
      "env": {
        "SQUAD_AGENT_ID": "builder"
      }
    }
  }
}
```

---

## 运行时文件

所有运行时状态存放在 `.squad/` 目录中（自动创建，已加入 .gitignore）：

```
.squad/
  squad.sock       Unix socket（守护进程 IPC）
  daemon.pid       守护进程 PID
  state.json       工作流状态
  session.json     会话元数据
  messages.log     实时消息流（TUI 使用）
  messages.db      持久化消息存储
  audit.log        完整审计日志
  hooks/
    on_complete.sh 示例完成 hook
    codex.sh       示例 Codex hook
```

---

## 文档

- [快速上手](docs/getting-started.md) — 两个智能体的完整演练
- [工作流模式](docs/workflow-modes.md) — 循环、流水线、并行
- [适配器](docs/adapters.md) — mcp、hook、watch
- [CLI 参考](docs/cli-reference.md) — 所有命令和标志
- [squad.yaml 参考](docs/squad-yaml.md) — 完整配置参考
