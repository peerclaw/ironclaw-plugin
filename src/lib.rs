// PeerClaw channel plugin for IronClaw.
//
// This WASM component implements the `sandboxed-channel` WIT interface,
// enabling PeerClaw P2P messaging within IronClaw's agent runtime.
//
// Architecture:
// - The PeerClaw Go agent connects to the IronClaw gateway via HTTP/SSE
//   (agent/platform/ironclaw adapter).
// - This WASM plugin handles the IronClaw side: receiving webhook messages
//   from the PeerClaw agent bridge and delivering AI responses back.
// - Communication uses a simple JSON bridge protocol over HTTP webhooks.

wit_bindgen::generate!({
    path: "wit/channel.wit",
    world: "sandboxed-channel",
});

use exports::near::agent::channel::{
    AgentResponse, ChannelConfig, Guest, HttpEndpointConfig, IncomingHttpRequest,
    OutgoingHttpResponse, PollConfig, StatusUpdate,
};
use near::agent::channel_host;
use serde::{Deserialize, Serialize};

struct PeerClawChannel;

/// Bridge protocol message from the PeerClaw agent.
#[derive(Deserialize)]
struct BridgeMessage {
    /// Message type: "chat.send" or "chat.inject"
    #[serde(rename = "type")]
    msg_type: String,
    data: serde_json::Value,
}

/// Chat message data from bridge.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatSendData {
    session_key: String,
    message: String,
}

/// Inject notification data from bridge.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InjectData {
    #[allow(dead_code)]
    session_key: String,
    message: String,
    #[allow(dead_code)]
    label: Option<String>,
}

/// Plugin configuration from capabilities file.
#[derive(Deserialize)]
struct PluginConfig {
    /// Poll interval in seconds (default: 0 = disabled).
    #[serde(default)]
    poll_interval_secs: u32,
}

/// Response to send back to the PeerClaw agent bridge.
#[derive(Serialize)]
struct BridgeResponse {
    #[serde(rename = "type")]
    msg_type: String,
    data: BridgeResponseData,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BridgeResponseData {
    session_key: String,
    state: String,
    message: String,
}

impl Guest for PeerClawChannel {
    fn on_start(config_json: String) -> Result<ChannelConfig, String> {
        channel_host::log(channel_host::LogLevel::Info, "PeerClaw channel starting");

        let config: PluginConfig =
            serde_json::from_str(&config_json).unwrap_or(PluginConfig { poll_interval_secs: 0 });

        let poll = if config.poll_interval_secs > 0 {
            Some(PollConfig {
                interval_ms: config.poll_interval_secs * 1000,
                enabled: true,
            })
        } else {
            None
        };

        Ok(ChannelConfig {
            display_name: "PeerClaw".to_string(),
            http_endpoints: vec![HttpEndpointConfig {
                path: "/webhook/peerclaw".to_string(),
                methods: vec!["POST".to_string()],
                require_secret: true,
            }],
            poll,
        })
    }

    fn on_http_request(req: IncomingHttpRequest) -> OutgoingHttpResponse {
        if req.method != "POST" {
            return OutgoingHttpResponse {
                status: 405,
                headers_json: r#"{"Allow":"POST"}"#.to_string(),
                body: b"Method Not Allowed".to_vec(),
            };
        }

        let body_str = match std::str::from_utf8(&req.body) {
            Ok(s) => s,
            Err(_) => {
                return OutgoingHttpResponse {
                    status: 400,
                    headers_json: "{}".to_string(),
                    body: b"Invalid UTF-8".to_vec(),
                };
            }
        };

        let bridge_msg: BridgeMessage = match serde_json::from_str(body_str) {
            Ok(m) => m,
            Err(e) => {
                channel_host::log(
                    channel_host::LogLevel::Warn,
                    &format!("Invalid bridge message: {e}"),
                );
                return OutgoingHttpResponse {
                    status: 400,
                    headers_json: "{}".to_string(),
                    body: format!("Invalid JSON: {e}").into_bytes(),
                };
            }
        };

        match bridge_msg.msg_type.as_str() {
            "chat.send" => {
                let data: ChatSendData = match serde_json::from_value(bridge_msg.data) {
                    Ok(d) => d,
                    Err(e) => {
                        return OutgoingHttpResponse {
                            status: 400,
                            headers_json: "{}".to_string(),
                            body: format!("Invalid chat.send data: {e}").into_bytes(),
                        };
                    }
                };

                // Extract peer ID from session key for user identification.
                let user_id = extract_peer_id(&data.session_key);

                channel_host::emit_message(channel_host::EmittedMessage {
                    user_id: &user_id,
                    user_name: None,
                    content: &data.message,
                    thread_id: Some(&data.session_key),
                    metadata_json: &format!(
                        r#"{{"channel":"peerclaw","sessionKey":"{}"}}"#,
                        data.session_key
                    ),
                    attachments: vec![],
                });

                channel_host::log(
                    channel_host::LogLevel::Debug,
                    &format!("Emitted P2P message from {user_id}"),
                );

                OutgoingHttpResponse {
                    status: 200,
                    headers_json: r#"{"Content-Type":"application/json"}"#.to_string(),
                    body: br#"{"ok":true}"#.to_vec(),
                }
            }
            "chat.inject" => {
                let data: InjectData = match serde_json::from_value(bridge_msg.data) {
                    Ok(d) => d,
                    Err(e) => {
                        return OutgoingHttpResponse {
                            status: 400,
                            headers_json: "{}".to_string(),
                            body: format!("Invalid chat.inject data: {e}").into_bytes(),
                        };
                    }
                };

                // Notifications are injected as system messages.
                channel_host::emit_message(channel_host::EmittedMessage {
                    user_id: "peerclaw-system",
                    user_name: Some("PeerClaw"),
                    content: &data.message,
                    thread_id: None,
                    metadata_json: r#"{"channel":"peerclaw","type":"notification"}"#,
                    attachments: vec![],
                });

                OutgoingHttpResponse {
                    status: 200,
                    headers_json: r#"{"Content-Type":"application/json"}"#.to_string(),
                    body: br#"{"ok":true}"#.to_vec(),
                }
            }
            _ => OutgoingHttpResponse {
                status: 400,
                headers_json: "{}".to_string(),
                body: format!("Unknown message type: {}", bridge_msg.msg_type).into_bytes(),
            },
        }
    }

    fn on_poll() {
        // No-op: PeerClaw messages arrive via webhook, not polling.
    }

    fn on_respond(response: AgentResponse) -> Result<(), String> {
        // Extract session key from metadata for routing.
        let session_key = extract_session_key_from_metadata(&response.metadata_json)
            .or_else(|| response.thread_id.clone())
            .unwrap_or_default();

        if session_key.is_empty() {
            return Err("No session key in response metadata".to_string());
        }

        // Send response back to PeerClaw agent via the bridge webhook.
        let bridge_resp = BridgeResponse {
            msg_type: "chat.event".to_string(),
            data: BridgeResponseData {
                session_key,
                state: "final".to_string(),
                message: response.content.clone(),
            },
        };

        let body = serde_json::to_vec(&bridge_resp).map_err(|e| format!("serialize: {e}"))?;

        // Read the agent bridge callback URL from workspace state.
        let callback_url = channel_host::workspace_read("callback_url")
            .unwrap_or_else(|| "http://localhost:19100/callback".to_string());

        let result = channel_host::http_request(
            "POST",
            &callback_url,
            r#"{"Content-Type":"application/json"}"#,
            Some(&body),
            Some(10_000),
        );

        match result {
            Ok(resp) if resp.status < 400 => {
                channel_host::log(
                    channel_host::LogLevel::Debug,
                    &format!("Response delivered to PeerClaw agent ({}B)", body.len()),
                );
                Ok(())
            }
            Ok(resp) => Err(format!(
                "Bridge callback returned {}",
                resp.status
            )),
            Err(e) => Err(format!("Bridge callback failed: {e}")),
        }
    }

    fn on_status(_update: StatusUpdate) {
        // Status updates (typing, thinking) are not forwarded to PeerClaw peers.
    }

    fn on_broadcast(user_id: String, response: AgentResponse) -> Result<(), String> {
        // Broadcast is used for proactive messages. Route through on_respond.
        let mut resp = response;
        // Set thread_id to route back to the peer.
        if resp.thread_id.is_none() {
            resp.thread_id = Some(format!("peerclaw:dm:{user_id}"));
        }
        Self::on_respond(resp)
    }

    fn on_shutdown() {
        channel_host::log(channel_host::LogLevel::Info, "PeerClaw channel stopped");
    }
}

/// Extract peer ID from a session key like "peerclaw:dm:<peer_id>".
fn extract_peer_id(session_key: &str) -> String {
    const PREFIX: &str = "peerclaw:dm:";
    if let Some(id) = session_key.strip_prefix(PREFIX) {
        id.to_string()
    } else {
        session_key.to_string()
    }
}

/// Extract session key from response metadata JSON.
fn extract_session_key_from_metadata(metadata_json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(metadata_json).ok()?;
    v.get("sessionKey")?.as_str().map(|s| s.to_string())
}

export!(PeerClawChannel);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_peer_id_with_prefix() {
        assert_eq!(extract_peer_id("peerclaw:dm:agent-abc"), "agent-abc");
    }

    #[test]
    fn test_extract_peer_id_without_prefix() {
        assert_eq!(extract_peer_id("some-other-key"), "some-other-key");
    }

    #[test]
    fn test_extract_peer_id_empty_suffix() {
        assert_eq!(extract_peer_id("peerclaw:dm:"), "");
    }

    #[test]
    fn test_extract_session_key_from_metadata_valid() {
        let meta = r#"{"sessionKey":"peerclaw:dm:peer-1","channel":"peerclaw"}"#;
        assert_eq!(
            extract_session_key_from_metadata(meta),
            Some("peerclaw:dm:peer-1".to_string())
        );
    }

    #[test]
    fn test_extract_session_key_from_metadata_missing() {
        let meta = r#"{"channel":"peerclaw"}"#;
        assert_eq!(extract_session_key_from_metadata(meta), None);
    }

    #[test]
    fn test_extract_session_key_from_metadata_invalid_json() {
        assert_eq!(extract_session_key_from_metadata("not json"), None);
    }

    #[test]
    fn test_bridge_message_parse_chat_send() {
        let json = r#"{"type":"chat.send","data":{"session_key":"peerclaw:dm:peer-1","message":"Hello"}}"#;
        let msg: BridgeMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.msg_type, "chat.send");
        let data: ChatSendData = serde_json::from_value(msg.data).unwrap();
        assert_eq!(data.session_key, "peerclaw:dm:peer-1");
        assert_eq!(data.message, "Hello");
    }

    #[test]
    fn test_bridge_message_parse_chat_inject() {
        let json = r#"{"type":"chat.inject","data":{"session_key":"peerclaw:dm:p","message":"Notification","label":"alert"}}"#;
        let msg: BridgeMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.msg_type, "chat.inject");
        let data: InjectData = serde_json::from_value(msg.data).unwrap();
        assert_eq!(data.message, "Notification");
        assert_eq!(data.label, Some("alert".to_string()));
    }

    #[test]
    fn test_bridge_message_parse_invalid_json() {
        let result: Result<BridgeMessage, _> = serde_json::from_str("not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_bridge_response_serialize() {
        let resp = BridgeResponse {
            msg_type: "chat.event".to_string(),
            data: BridgeResponseData {
                session_key: "peerclaw:dm:peer-1".to_string(),
                state: "final".to_string(),
                message: "AI response".to_string(),
            },
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("chat.event"));
        assert!(json.contains("AI response"));
        assert!(json.contains("final"));
    }

    #[test]
    fn test_plugin_config_default() {
        let config: PluginConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config.poll_interval_secs, 0);
    }

    #[test]
    fn test_plugin_config_custom() {
        let config: PluginConfig =
            serde_json::from_str(r#"{"poll_interval_secs":30}"#).unwrap();
        assert_eq!(config.poll_interval_secs, 30);
    }
}
