// Is this just two examples from tokio_tungstenite glued together?
//
// It absolute is!

use futures_util::{future, StreamExt, TryStreamExt};
use log::info;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::connect_async;

pub struct ButtplugRepeater {
  local_port: u16,
  remote_address: String,
}

impl ButtplugRepeater {
  pub fn new(local_port: u16, remote_address: &str) -> Self {
    Self {
      local_port,
      remote_address: remote_address.to_owned()
    }
  }

  pub async fn listen(&self) {
    let addr = format!("127.0.0.1:{}", self.local_port);

    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");
    info!("Listening on: {}", addr);

    while let Ok((stream, _)) = listener.accept().await {
      let remote_address = self.remote_address.clone();
      tokio::spawn(ButtplugRepeater::accept_connection(remote_address, stream));
    }
  }

  async fn accept_connection(server_addr: String, stream: TcpStream) {
    let client_addr = stream
      .peer_addr()
      .expect("connected streams should have a peer address");
    info!("Client address: {}", client_addr);

    let client_ws_stream = tokio_tungstenite::accept_async(stream)
      .await
      .expect("Error during the websocket handshake occurred");

    info!("New WebSocket connection: {}", client_addr);

    let server_url = url::Url::parse(&server_addr).unwrap();

    let ws_stream = match connect_async(&server_url).await {
      Ok((stream, _)) => stream,
      Err(e) => {
        error!("Cannot connect: {:?}", e);
        return;
      }
    };
    info!("WebSocket handshake has been successfully completed");

    let (server_write, server_read) = ws_stream.split();

    let (client_write, client_read) = client_ws_stream.split();

    let client_fut = client_read
      .try_filter(|msg| future::ready(msg.is_text() || msg.is_binary()))
      .forward(server_write);
    let server_fut = server_read
      .try_filter(|msg| future::ready(msg.is_text() || msg.is_binary()))
      .forward(client_write);
    future::select(client_fut, server_fut).await;
    info!("Closing repeater connection.");
  }
}
