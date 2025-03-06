use serde::{Deserialize, Serialize};

// WebSocket消息结构
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WsMessage {
    pub user_id: String,
    pub content: String,
    pub message_type: String, // "chat", "join", "leave", "rename"
    pub username: Option<String>,
}

// 用户名修改请求
#[derive(Debug, Deserialize)]
pub struct RenameRequest {
    pub user_id: String,
    pub new_name: String,
}

// 用户名修改响应
#[derive(Debug, Serialize)]
pub struct RenameResponse {
    pub success: bool,
    pub message: String,
} 