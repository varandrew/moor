use serde_json::Value;
use tokio::sync::broadcast;

/// 类型化领域事件。每个变体对应一种 SSE 事件类型,携带自己的载荷。
/// 发射方用变体构造(打错变体名 = 编译错误),消费侧用 [Evt::name]
/// 还原 SSE 事件名字符串——线上 SSE 协议不变。
///
/// 事件名常量集中在 [Evt::name],是事件词汇表的单一事实来源。
#[derive(Debug, Clone)]
pub enum Evt {
    ServerHealth {
        snapshot: super::server_manager::ServerHealthSnapshot,
    },
    /// 服务器状态变更(stopped/starting/running/error)。
    ServerStatus {
        server_id: String,
        status: String,
        error_message: Option<String>,
    },
    /// 服务器工具列表变更(启动后重新发现工具)。
    ServerTools { server_id: String },
    /// 活动 Profile 切换。
    ProfileActivated { profile_id: String },
    /// 设置变更(整体 Settings 值)。
    SettingsChanged { settings: Value },
}

impl Evt {
    /// 还原 SSE 事件名字符串。前端按这个值分发处理函数。
    pub fn name(&self) -> &'static str {
        match self {
            Evt::ServerHealth { .. } => "server:health",
            Evt::ServerStatus { .. } => "server:status",
            Evt::ServerTools { .. } => "server:tools",
            Evt::ProfileActivated { .. } => "profile:activated",
            Evt::SettingsChanged { .. } => "settings:changed",
        }
    }

    /// 序列化载荷成 SSE data 字段。
    pub fn payload(&self) -> Value {
        match self {
            Evt::ServerHealth { snapshot } => {
                serde_json::to_value(snapshot).expect("health snapshot serializes")
            }
            Evt::ServerStatus {
                server_id,
                status,
                error_message,
            } => serde_json::json!({
                "serverId": server_id,
                "status": status,
                "errorMessage": error_message,
            }),
            Evt::ServerTools { server_id } => serde_json::json!({ "serverId": server_id }),
            Evt::ProfileActivated { profile_id } => {
                serde_json::json!({ "profileId": profile_id })
            }
            Evt::SettingsChanged { settings } => settings.clone(),
        }
    }
}

pub struct EventBus {
    sender: broadcast::Sender<Evt>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn emit(&self, event: Evt) {
        let _ = self.sender.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Evt> {
        self.sender.subscribe()
    }
}
