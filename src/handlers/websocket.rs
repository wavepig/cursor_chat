use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
};
use futures::{sink::SinkExt, stream::StreamExt};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::broadcast;

use crate::models::WsMessage;

// WebSocket处理函数
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    tx: Arc<broadcast::Sender<String>>,
    connections: Arc<Mutex<HashMap<String, String>>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, tx, connections))
}

// 辅助函数：发送更新后的用户列表
fn send_updated_users_list(
    connections: &HashMap<String, String>,
    tx: &broadcast::Sender<String>,
) {
    let users: Vec<String> = connections.values().cloned().collect();
    let users_message = WsMessage {
        user_id: "system".to_string(),
        content: serde_json::to_string(&users).unwrap(),
        message_type: "users".to_string(),
        username: None,
    };
    let _ = tx.send(serde_json::to_string(&users_message).unwrap());
}

// WebSocket连接处理函数
async fn handle_socket(
    socket: WebSocket,
    tx: Arc<broadcast::Sender<String>>,
    connections: Arc<Mutex<HashMap<String, String>>>,
) {
    // 生成用户ID和初始用户名
    let user_id = format!("user_{}", rand::random::<u32>());
    let initial_username = format!("用户{}", rand::random::<u32>());
    let mut rx = tx.subscribe();

    // 发送欢迎消息和当前用户列表
    let join_message = WsMessage {
        user_id: user_id.clone(),
        content: format!("{} 进入聊天室", initial_username),
        message_type: "join".to_string(),
        username: Some(initial_username.clone()),
    };

    // 添加用户到连接列表
    {
        let mut conns = connections.lock().unwrap();
        conns.insert(user_id.clone(), initial_username);
        send_updated_users_list(&conns, &tx);
    }

    // 广播加入消息
    let _ = tx.send(serde_json::to_string(&join_message).unwrap());

    let (mut sender, mut receiver) = socket.split();

    // 为任务克隆所需的值
    let user_id_recv = user_id.clone();
    let tx_recv = tx.clone();
    let connections_recv = connections.clone();

    // 处理接收消息
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = receiver.next().await {
            // 获取当前用户名
            let current_username = connections_recv
                .lock()
                .unwrap()
                .get(&user_id_recv)
                .cloned()
                .unwrap_or_else(|| "未知用户".to_string());

            let chat_message = WsMessage {
                user_id: user_id_recv.clone(),
                content: text,
                message_type: "chat".to_string(),
                username: Some(current_username),
            };
            let _ = tx_recv.send(serde_json::to_string(&chat_message).unwrap());
        }
    });

    // 处理广播消息
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // 等待任意一个任务完成
    tokio::select! {
        _ = (&mut recv_task) => send_task.abort(),
        _ = (&mut send_task) => recv_task.abort(),
    }

    // 用户断开连接，清理资源
    {
        let mut conns = connections.lock().unwrap();
        let username = conns.remove(&user_id).unwrap_or_default();

        // 发送更新后的用户列表
        send_updated_users_list(&conns, &tx);

        // 广播离开消息
        let leave_message = WsMessage {
            user_id: user_id.clone(),
            content: format!("{} 离开聊天室", username),
            message_type: "leave".to_string(),
            username: Some(username),
        };
        let _ = tx.send(serde_json::to_string(&leave_message).unwrap());
    }
} 