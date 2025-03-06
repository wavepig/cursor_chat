use serde::{Deserialize, Serialize};

// JSON请求数据结构
#[derive(Debug, Deserialize)]
pub struct RequestData {
    pub message: String,
}

// JSON响应数据结构
#[derive(Debug, Serialize)]
pub struct ResponseData {
    pub message: String,
    pub status: String,
} 