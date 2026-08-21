use serde::{Deserialize, Serialize};
use std::fs;

use crate::settings::data_dir;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SubscribedTopic {
    pub topic_id: String,
    pub server_url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl SubscribedTopic {
    pub fn unique(&self) -> String {
        format!("{}@{}", self.topic_id, self.server_url)
    }

    /// 根据服务器 URL 的 scheme 判断原始项目使用的连接类型。
    pub fn connection_kind(&self) -> ConnectionKind {
        let lower = self.server_url.to_lowercase();
        if lower.starts_with("ws://") || lower.starts_with("wss://") {
            ConnectionKind::Websocket
        } else {
            ConnectionKind::HttpJson
        }
    }

    pub fn has_credentials(&self) -> bool {
        self.username.as_deref().map(str::trim).unwrap_or("").len() > 0
            && self.password.as_deref().map(str::trim).unwrap_or("").len() > 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionKind {
    Websocket,
    HttpJson,
}

impl ConnectionKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Websocket => "WebSocket",
            Self::HttpJson => "Long HTTP JSON",
        }
    }
}

fn topics_path() -> std::path::PathBuf {
    data_dir().join("topics.json")
}

fn legacy_topics_path() -> std::path::PathBuf {
    data_dir().join("topics.txt")
}

pub fn load_topics() -> Vec<SubscribedTopic> {
    let path = topics_path();
    let legacy_path = legacy_topics_path();

    // 兼容旧版 topics.txt：迁移为 topics.json 后删除旧文件。
    if !path.exists() && legacy_path.exists() {
        if let Ok(text) = fs::read_to_string(&legacy_path) {
            let topics: Vec<SubscribedTopic> = text
                .lines()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|topic_id| SubscribedTopic {
                    topic_id: topic_id.to_string(),
                    server_url: "https://ntfy.sh".to_string(),
                    username: None,
                    password: None,
                })
                .collect();
            save_topics(&topics);
            let _ = fs::remove_file(&legacy_path);
            return topics;
        }
    }

    let Ok(text) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    if text.trim().is_empty() {
        return Vec::new();
    }
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn save_topics(topics: &[SubscribedTopic]) {
    let path = topics_path();
    if let Ok(text) = serde_json::to_string_pretty(topics) {
        let _ = fs::write(path, text);
    }
}
