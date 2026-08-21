use crate::updater::UpdateCheckResult;
use eframe::egui;
use tokio::sync::mpsc;

/// 包装 `UnboundedSender`，每次发送事件后请求 egui 重绘，
/// 保证后台任务（通知、连接状态、更新检查）能立刻反映到界面上。
#[derive(Clone)]
pub struct EventSender {
    inner: mpsc::UnboundedSender<AppEvent>,
    ctx: egui::Context,
}

impl EventSender {
    pub fn new(inner: mpsc::UnboundedSender<AppEvent>, ctx: egui::Context) -> Self {
        Self { inner, ctx }
    }

    pub fn send(&self, event: AppEvent) -> Result<(), mpsc::error::SendError<AppEvent>> {
        let result = self.inner.send(event);
        self.ctx.request_repaint();
        result
    }
}

#[derive(Debug)]
pub enum AppEvent {
    Notification {
        unique: String,
        title: String,
        message: String,
        priority: u8,
    },
    ConnectionFailed {
        unique: String,
        reason: ConnectionFailureReason,
    },
    UpdateCheckFinished {
        result: UpdateCheckResult,
        interactive: bool,
    },
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ConnectionFailureReason {
    MultiAttempt,
    Credentials,
    Other(String),
}

impl std::fmt::Display for ConnectionFailureReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MultiAttempt => write!(f, "多次连接失败，已停止重试"),
            Self::Credentials => write!(f, "认证失败（凭据缺失或错误）"),
            Self::Other(msg) => write!(f, "{}", msg),
        }
    }
}
