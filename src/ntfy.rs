use crate::event::{AppEvent, ConnectionFailureReason, EventSender};
use crate::topics::{ConnectionKind, SubscribedTopic};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use futures_util::StreamExt;
use serde::Deserialize;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Deserialize)]
struct NtfyEvent {
    #[serde(default)]
    event: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    priority: Option<u8>,
}

#[derive(Debug)]
enum StreamOutcome {
    /// 已成功建立连接（可能随后正常断开）。
    Connected,
    /// 认证失败，不应继续重试。
    Credentials,
    /// 其他连接/读取错误。
    Error(String),
}

fn encoded_topic(topic_id: &str) -> String {
    urlencoding::encode(topic_id).into_owned()
}

fn server_base(server_url: &str) -> String {
    server_url.trim_end_matches('/').to_string()
}

fn auth_header(username: &Option<String>, password: &Option<String>) -> Option<String> {
    match (username, password) {
        (Some(u), Some(p)) if !u.trim().is_empty() && !p.trim().is_empty() => {
            let raw = format!("{}:{}", u, p);
            Some(format!("Basic {}", BASE64.encode(raw.as_bytes())))
        }
        _ => None,
    }
}

fn process_line(topic: &SubscribedTopic, line: &str, tx: &EventSender) {
    let line = line.trim();
    if line.is_empty() {
        return;
    }

    if let Ok(evt) = serde_json::from_str::<NtfyEvent>(line) {
        if evt.event == "message" {
            let title = evt.title.unwrap_or_default();
            let message = evt.message.unwrap_or_default();
            let priority = evt.priority.unwrap_or(3).clamp(1, 5);
            let _ = tx.send(AppEvent::Notification {
                unique: topic.unique(),
                title,
                message,
                priority,
            });
        }
    }
}

/// 启动一个话题监听任务。返回后任务会持续运行直到 token 被取消或连接彻底失败。
pub async fn listen(
    topic: SubscribedTopic,
    kind: ConnectionKind,
    tx: EventSender,
    token: CancellationToken,
    reconnect_attempts: u32,
    reconnect_attempt_delay: u64,
) {
    let mut attempts: u32 = 0;

    loop {
        if token.is_cancelled() {
            return;
        }

        if reconnect_attempts != 0 && attempts >= reconnect_attempts {
            let _ = tx.send(AppEvent::ConnectionFailed {
                unique: topic.unique(),
                reason: ConnectionFailureReason::MultiAttempt,
            });
            return;
        }

        let outcome = match kind {
            ConnectionKind::Websocket => listen_websocket_once(&topic, &tx, &token).await,
            ConnectionKind::HttpJson => listen_http_once(&topic, &tx, &token).await,
        };

        if token.is_cancelled() {
            return;
        }

        match outcome {
            StreamOutcome::Connected => {
                // 连接成功过，按原项目逻辑重置失败计数。
                attempts = 0;
            }
            StreamOutcome::Credentials => {
                let _ = tx.send(AppEvent::ConnectionFailed {
                    unique: topic.unique(),
                    reason: ConnectionFailureReason::Credentials,
                });
                return;
            }
            StreamOutcome::Error(msg) => {
                log::debug!("topic {} stream error: {}", topic.unique(), msg);
            }
        }

        attempts += 1;

        // 有限重试时第一次失败立即重试；无限重试时每次都等待。
        if reconnect_attempts == 0 || attempts > 1 {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(reconnect_attempt_delay)) => {}
                _ = token.cancelled() => return,
            }
        }
    }
}

async fn listen_http_once(
    topic: &SubscribedTopic,
    tx: &EventSender,
    token: &CancellationToken,
) -> StreamOutcome {
    let client = match reqwest::Client::builder().build() {
        Ok(client) => client,
        Err(err) => return StreamOutcome::Error(err.to_string()),
    };

    let url = format!(
        "{}/{}/json",
        server_base(&topic.server_url),
        encoded_topic(&topic.topic_id)
    );

    let mut request = client.get(&url);
    if let Some(auth) = auth_header(&topic.username, &topic.password) {
        request = request.header(reqwest::header::AUTHORIZATION, auth);
    }

    let response = match request.send().await {
        Ok(response) => response,
        Err(err) => return StreamOutcome::Error(err.to_string()),
    };

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return StreamOutcome::Credentials;
    }
    if !status.is_success() {
        return StreamOutcome::Error(format!("HTTP {}", status));
    }

    let mut stream = response.bytes_stream();
    let mut buffer: Vec<u8> = Vec::new();

    loop {
        tokio::select! {
            _ = token.cancelled() => return StreamOutcome::Connected,
            chunk = stream.next() => {
                match chunk {
                    Some(Ok(bytes)) => {
                        buffer.extend_from_slice(bytes.as_ref());
                        while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                            let raw: Vec<u8> = buffer.drain(..=pos).collect();
                            let line = String::from_utf8_lossy(&raw[..raw.len() - 1]);
                            process_line(topic, line.as_ref(), tx);
                        }
                    }
                    Some(Err(err)) => return StreamOutcome::Error(err.to_string()),
                    None => return StreamOutcome::Connected,
                }
            }
        }
    }
}

async fn listen_websocket_once(
    topic: &SubscribedTopic,
    tx: &EventSender,
    token: &CancellationToken,
) -> StreamOutcome {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::Message;

    let url = format!(
        "{}/{}/ws",
        server_base(&topic.server_url),
        encoded_topic(&topic.topic_id)
    );

    let mut request = match url.as_str().into_client_request() {
        Ok(req) => req,
        Err(err) => return StreamOutcome::Error(err.to_string()),
    };

    if let Some(auth) = auth_header(&topic.username, &topic.password) {
        if let Ok(value) = http::HeaderValue::from_str(&auth) {
            request.headers_mut().insert(http::header::AUTHORIZATION, value);
        }
    }

    let (mut ws, _) = match tokio_tungstenite::connect_async(request).await {
        Ok(pair) => pair,
        Err(err) => {
            let text = err.to_string();
            // 常见的 401/403 或非 WebSocket 响应都视为凭据问题。
            if text.contains("401") || text.contains("403") || text.contains("Not a WebSocket") {
                return StreamOutcome::Credentials;
            }
            return StreamOutcome::Error(text);
        }
    };

    loop {
        tokio::select! {
            _ = token.cancelled() => return StreamOutcome::Connected,
            msg = ws.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        for line in text.as_str().lines() {
                            process_line(topic, line, tx);
                        }
                    }
                    Some(Ok(Message::Binary(data))) => {
                        let text = String::from_utf8_lossy(data.as_ref());
                        for line in text.lines() {
                            process_line(topic, line, tx);
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(err)) => return StreamOutcome::Error(err.to_string()),
                    None => return StreamOutcome::Connected,
                }
            }
        }
    }
}
