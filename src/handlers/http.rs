use axum::{
    extract::{Json, State},
    response::{Html, IntoResponse, Json as JsonResponse},
};
use std::{
    collections::HashMap,
    fs,
    sync::{Arc, Mutex},
};
use tokio::sync::broadcast;

use crate::models::{RenameRequest, RenameResponse, RequestData, ResponseData, WsMessage};

// 处理HTML页面请求的函数
pub async fn serve_html() -> impl IntoResponse {
    match fs::read_to_string("src/index.html") {
        Ok(html_content) => Html(html_content).into_response(),
        Err(err) => {
            eprintln!("Error reading index.html: {}", err);
            Html("<h1>Internal Server Error</h1>").into_response()
        }
    }
}

// 处理JSON请求的函数
pub async fn handle_json(Json(payload): Json<RequestData>) -> JsonResponse<ResponseData> {
    let response = ResponseData {
        message: format!("收到消息: {}", payload.message),
        status: "success".to_string(),
    };
    JsonResponse(response)
}

// 处理用户名修改请求
pub async fn handle_rename(
    State((tx, connections)): State<(
        Arc<broadcast::Sender<String>>,
        Arc<Mutex<HashMap<String, String>>>,
    )>,
    Json(payload): Json<RenameRequest>,
) -> JsonResponse<RenameResponse> {
    let new_name = payload.new_name.trim();

    // 验证用户名
    if new_name.is_empty() || new_name.len() > 20 {
        return JsonResponse(RenameResponse {
            success: false,
            message: "用户名长度必须在1-20个字符之间".to_string(),
        });
    }

    if new_name.contains(|c: char| !c.is_alphanumeric() && c != '_') {
        return JsonResponse(RenameResponse {
            success: false,
            message: "用户名只能包含字母、数字和下划线".to_string(),
        });
    }

    let mut conns = connections.lock().unwrap();

    // 检查新用户名是否已存在
    if conns.values().any(|name| name == new_name) {
        return JsonResponse(RenameResponse {
            success: false,
            message: "用户名已存在".to_string(),
        });
    }

    // 更新用户名
    if let Some(old_name) = conns.get_mut(&payload.user_id) {
        let old_name_str = old_name.clone();
        *old_name = new_name.to_string();

        // 广播用户名修改消息
        let rename_message = WsMessage {
            user_id: payload.user_id.clone(),
            content: format!("{} 已将用户名修改为 {}", old_name_str, new_name),
            message_type: "rename".to_string(),
            username: Some(new_name.to_string()),
        };

        let _ = tx.send(serde_json::to_string(&rename_message).unwrap());

        // 发送更新后的用户列表
        let users: Vec<String> = conns.values().cloned().collect();
        let users_message = WsMessage {
            user_id: "system".to_string(),
            content: serde_json::to_string(&users).unwrap(),
            message_type: "users".to_string(),
            username: None,
        };
        let _ = tx.send(serde_json::to_string(&users_message).unwrap());

        JsonResponse(RenameResponse {
            success: true,
            message: format!("成功修改用户名为 {}", new_name),
        })
    } else {
        JsonResponse(RenameResponse {
            success: false,
            message: "用户不存在".to_string(),
        })
    }
}
