mod handlers;
mod models;
mod utils;

use axum::{routing::{get, post}, Router};
use std::{net::SocketAddr, sync::{Arc, Mutex}, collections::HashMap};
use tokio::sync::broadcast;

use crate::{
    handlers::{handle_json, handle_rename, serve_html, ws_handler},
    utils::CHANNEL_CAPACITY,
};

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
        .route("/ws", get(move |ws| {
            ws_handler(ws, tx.clone(), connections.clone())
        }))
        .with_state(state);

    // 设置服务器地址
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    
    // 启动服务器
    println!("Server starting on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("Server is running on http://{}", addr);
    
    axum::serve(listener, app).await?;
    Ok(())
}