use axum::{
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
    extract::{
        Json as ExtractJson,
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    net::SocketAddr,
    sync::{Arc, Mutex},
    collections::HashMap,
};
use tokio::sync::broadcast;

// 定义广播通道的容量
const CHANNEL_CAPACITY: usize = 32;

// 定义WebSocket消息结构
#[derive(Clone, Debug, Serialize, Deserialize)]
struct WsMessage {
    user_id: String,
    content: String,
    message_type: String, // "chat", "join", "leave", "rename"
    username: Option<String>, // 添加可选的用户名字段
}

// 用户名修改请求
#[derive(Debug, Deserialize)]
struct RenameRequest {
    user_id: String,
    new_name: String,
}

// 用户名修改响应
#[derive(Debug, Serialize)]
struct RenameResponse {
    success: bool,
    message: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建广播通道
    let (tx, _rx) = broadcast::channel::<String>(CHANNEL_CAPACITY);
    let tx = Arc::new(tx);
    
    // 创建用户连接映射
    let connections = Arc::new(Mutex::new(HashMap::new()));

    // 创建路由
    let state = (tx.clone(), connections.clone());
    let app = Router::new()
        .route("/", get(serve_html))
        .route("/api/message", post(handle_json))
        .route("/api/rename", post(handle_rename))
        .route("/ws", get(move |ws: WebSocketUpgrade| {
            ws_handler(ws, tx.clone(), connections.clone())
        }))
        .with_state(state);

    // 设置服务器地址
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    
    // 启动服务器
    println!("Server starting on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("Server is running on http://{}", addr);
    
    axum::serve(listener, app).await?;
    Ok(())
}

// 处理HTML页面请求的函数
async fn serve_html() -> impl IntoResponse {
    match fs::read_to_string("src/index.html") {
        Ok(html_content) => Html(html_content).into_response(),
        Err(err) => {
            eprintln!("Error reading index.html: {}", err);
            Html("<h1>Internal Server Error</h1>").into_response()
        }
    }
}

// 定义请求和响应的数据结构
#[derive(Debug, Deserialize)]
struct RequestData {
    message: String,
}

#[derive(Debug, Serialize)]
struct ResponseData {
    message: String,
    status: String,
}

// 处理JSON请求的函数
async fn handle_json(
    ExtractJson(payload): ExtractJson<RequestData>,
) -> Json<ResponseData> {
    let response = ResponseData {
        message: format!("收到消息: {}", payload.message),
        status: "success".to_string(),
    };
    Json(response)
}

// 处理用户名修改请求
#[axum::debug_handler]
async fn handle_rename(
    State((tx, connections)): State<(Arc<broadcast::Sender<String>>, Arc<Mutex<HashMap<String, String>>>)>,
    Json(payload): Json<RenameRequest>,
) -> Json<RenameResponse> {
    let new_name = payload.new_name.trim();
    
    // 验证用户名
    if new_name.is_empty() || new_name.len() > 20 {
        return Json(RenameResponse {
            success: false,
            message: "用户名长度必须在1-20个字符之间".to_string(),
        });
    }

    if new_name.contains(|c: char| !c.is_alphanumeric() && c != '_') {
        return Json(RenameResponse {
            success: false,
            message: "用户名只能包含字母、数字和下划线".to_string(),
        });
    }

    let mut conns = connections.lock().unwrap();
    
    // 检查新用户名是否已存在
    if conns.values().any(|name| name == new_name) {
        return Json(RenameResponse {
            success: false,
            message: "用户名已存在".to_string(),
        });
    }

    // 更新用户名
    if let Some(old_name) = conns.get_mut(&payload.user_id) {
        *old_name = new_name.to_string();
        
        // 广播用户名修改消息
        let rename_message = WsMessage {
            user_id: payload.user_id.clone(),
            content: format!("已将用户名修改为 {}", new_name),
            message_type: "rename".to_string(),
            username: Some(new_name.to_string()),
        };
        
        let _ = tx.send(serde_json::to_string(&rename_message).unwrap());
        
        Json(RenameResponse {
            success: true,
            message: format!("成功修改用户名为 {}", new_name),
        })
    } else {
        Json(RenameResponse {
            success: false,
            message: "用户不存在".to_string(),
        })
    }
}

// WebSocket处理函数
async fn ws_handler(
    ws: WebSocketUpgrade,
    tx: Arc<broadcast::Sender<String>>,
    connections: Arc<Mutex<HashMap<String, String>>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, tx, connections))
}

// WebSocket连接处理函数
async fn handle_socket(
    socket: WebSocket,
    tx: Arc<broadcast::Sender<String>>,
    connections: Arc<Mutex<HashMap<String, String>>>,
) {
    // 生成用户ID
    let user_id = format!("user_{}", rand::random::<u32>());
    let mut rx = tx.subscribe();

    // 发送欢迎消息和当前用户列表
    let join_message = WsMessage {
        user_id: user_id.clone(),
        content: "joined the chat".to_string(),
        message_type: "join".to_string(),
        username: None,
    };
    
    // 添加用户到连接列表
    {
        let mut conns = connections.lock().unwrap();
        conns.insert(user_id.clone(), "online".to_string());
        
        // 发送用户列表
        let users: Vec<String> = conns.keys().cloned().collect();
        let users_message = WsMessage {
            user_id: "system".to_string(),
            content: serde_json::to_string(&users).unwrap(),
            message_type: "users".to_string(),
            username: None,
        };
        let _ = tx.send(serde_json::to_string(&users_message).unwrap());
    }

    // 广播加入消息
    let _ = tx.send(serde_json::to_string(&join_message).unwrap());

    let (mut sender, mut receiver) = socket.split();

    // 为任务克隆所需的值
    let user_id_recv = user_id.clone();
    let tx_recv = tx.clone();

    // 处理接收消息
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = receiver.next().await {
            let chat_message = WsMessage {
                user_id: user_id_recv.clone(),
                content: text,
                message_type: "chat".to_string(),
                username: None,
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
    };

    // 用户断开连接，清理资源
    {
        let mut conns = connections.lock().unwrap();
        conns.remove(&user_id);
        
        // 发送更新后的用户列表
        let users: Vec<String> = conns.keys().cloned().collect();
        let users_message = WsMessage {
            user_id: "system".to_string(),
            content: serde_json::to_string(&users).unwrap(),
            message_type: "users".to_string(),
            username: None,
        };
        let _ = tx.send(serde_json::to_string(&users_message).unwrap());
    }

    // 广播离开消息
    let leave_message = WsMessage {
        user_id: user_id.clone(),
        content: "left the chat".to_string(),
        message_type: "leave".to_string(),
        username: None,
    };
    let _ = tx.send(serde_json::to_string(&leave_message).unwrap());
}