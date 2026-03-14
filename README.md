# peerclaw-ironclaw-plugin

IronClaw WASM channel plugin for [PeerClaw](https://github.com/peerclaw/peerclaw) — a P2P agent identity and trust platform.

This plugin implements IronClaw's `sandboxed-channel` WIT interface as a WASM component, enabling PeerClaw P2P messaging within IronClaw's agent runtime.

## Architecture

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

The PeerClaw Go agent connects to IronClaw's gateway via REST/SSE. This WASM plugin handles the IronClaw side:

1. **Inbound**: Receives webhook POST from PeerClaw agent bridge → emits message to IronClaw agent
2. **Outbound**: Receives AI response via `on_respond` → sends back to PeerClaw agent via HTTP callback

## WIT Interface

This plugin implements the `sandboxed-channel` world from IronClaw's WIT definition:

| Callback | Purpose |
|----------|---------|
| `on-start` | Register `/webhook/peerclaw` endpoint |
| `on-http-request` | Parse bridge messages, emit to agent |
| `on-respond` | Deliver AI response to PeerClaw agent |
| `on-broadcast` | Send proactive message to peer |
| `on-status` | No-op (typing indicators not forwarded) |
| `on-poll` | No-op (messages arrive via webhook) |
| `on-shutdown` | Cleanup logging |

## Building

Requires Rust toolchain with `wasm32-wasip2` target:

```bash
rustup target add wasm32-wasip2
cargo build --target wasm32-wasip2 --release
```

The compiled WASM component will be at:
```
target/wasm32-wasip2/release/peerclaw_ironclaw_plugin.wasm
```

## Installation

Copy the WASM file to IronClaw's extensions directory:

```bash
cp target/wasm32-wasip2/release/peerclaw_ironclaw_plugin.wasm \
   ~/.ironclaw/extensions/peerclaw.wasm
```

Or install via IronClaw CLI:

```bash
ironclaw extension install ./peerclaw_ironclaw_plugin.wasm
```

## Configuration

In IronClaw's capabilities/extension config:

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

## Agent-Side Setup

On the PeerClaw agent side, configure the IronClaw platform adapter in your `peerclaw.yaml`:

```yaml
platform:
  type: ironclaw
  gateway_url: "http://localhost:8080"
  auth_token: "your-bearer-token"
```

## Bridge Protocol

The plugin uses a simple JSON protocol for webhook communication:

**Agent → Plugin** (POST /webhook/peerclaw):
```json
{"type": "chat.send", "data": {"sessionKey": "peerclaw:dm:<peer_id>", "message": "Hello"}}
{"type": "chat.inject", "data": {"sessionKey": "peerclaw:notifications", "message": "[INFO] ...", "label": "notification"}}
```

**Plugin → Agent** (HTTP callback):
```json
{"type": "chat.event", "data": {"sessionKey": "peerclaw:dm:<peer_id>", "state": "final", "message": "AI response"}}
```

## License

Apache-2.0
