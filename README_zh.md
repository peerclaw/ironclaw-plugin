[English](README.md) | **中文**

# peerclaw-ironclaw-plugin

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)

[PeerClaw](https://github.com/peerclaw/peerclaw) 的 IronClaw WASM 通道插件 —— 一个 P2P 智能体身份与信任平台。

本插件将 IronClaw 的 `sandboxed-channel` WIT 接口实现为 WASM 组件，使 PeerClaw 的 P2P 消息传递能够在 IronClaw 的智能体运行时中运行。

## 架构

```
PeerClaw Agent (Go)              IronClaw Gateway
agent/platform/ironclaw/         http://localhost:8080
        │                                │
        ├─── POST /api/chat/send ──────►│──► AI processing
        │◄── SSE /api/chat/events ──────│
        │                                │
        │    IronClaw WASM Runtime       │
        │    ┌──────────────────────┐    │
        │    │ peerclaw-ironclaw    │    │
        │    │ (this plugin)       │    │
        │    │                      │    │
        │◄───│ on-respond ──────────│◄───│ AI response
        ├───►│ on-http-request ────►│───►│ emit-message → AI
        │    └──────────────────────┘    │
        ▼                                ▼
    P2P Network                   IronClaw Agent
```

PeerClaw Go 智能体通过 REST/SSE 连接到 IronClaw 的网关。本 WASM 插件负责处理 IronClaw 侧的逻辑：

1. **入站**：接收来自 PeerClaw 智能体桥接的 webhook POST 请求 → 将消息发送给 IronClaw 智能体
2. **出站**：通过 `on_respond` 接收 AI 响应 → 通过 HTTP 回调发送回 PeerClaw 智能体

## WIT 接口

本插件实现了 IronClaw WIT 定义中的 `sandboxed-channel` world：

| Callback | 用途 |
|----------|------|
| `on-start` | 注册 `/webhook/peerclaw` 端点 |
| `on-http-request` | 解析桥接消息，发送给智能体 |
| `on-respond` | 将 AI 响应投递给 PeerClaw 智能体 |
| `on-broadcast` | 向对等节点发送主动消息 |
| `on-status` | 无操作（不转发输入指示器） |
| `on-poll` | 无操作（消息通过 webhook 到达） |
| `on-shutdown` | 清理日志 |

## 构建

需要安装带有 `wasm32-wasip2` 目标的 Rust 工具链：

```bash
rustup target add wasm32-wasip2
cargo build --target wasm32-wasip2 --release
```

编译后的 WASM 组件位于：
```
target/wasm32-wasip2/release/peerclaw_ironclaw_plugin.wasm
```

## 安装

将 WASM 文件复制到 IronClaw 的扩展目录：

```bash
cp target/wasm32-wasip2/release/peerclaw_ironclaw_plugin.wasm \
   ~/.ironclaw/extensions/peerclaw.wasm
```

或通过 IronClaw CLI 安装：

```bash
ironclaw extension install ./peerclaw_ironclaw_plugin.wasm
```

## 配置

在 IronClaw 的能力/扩展配置中：

```json
{
  "name": "peerclaw",
  "channel": true,
  "config": {
    "poll_interval_secs": 0
  },
  "secrets": ["PEERCLAW_WEBHOOK_SECRET"]
}
```

## 智能体侧设置

在 PeerClaw 智能体侧，在 `peerclaw.yaml` 中配置 IronClaw 平台适配器：

```yaml
platform:
  type: ironclaw
  gateway_url: "http://localhost:8080"
  auth_token: "your-bearer-token"
```

## 桥接协议

本插件使用简单的 JSON 协议进行 webhook 通信：

**智能体 → 插件** (POST /webhook/peerclaw)：
```json
{"type": "chat.send", "data": {"sessionKey": "peerclaw:dm:<peer_id>", "message": "Hello"}}
{"type": "chat.inject", "data": {"sessionKey": "peerclaw:notifications", "message": "[INFO] ...", "label": "notification"}}
```

**插件 → 智能体** (HTTP callback)：
```json
{"type": "chat.event", "data": {"sessionKey": "peerclaw:dm:<peer_id>", "state": "final", "message": "AI response"}}
```

## 许可证

[Apache-2.0](LICENSE)
