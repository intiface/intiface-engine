use super::{
  process_messages::{EngineMessage, IntifaceMessage},
  Frontend,
};
use crate::error::IntifaceError;

use async_trait::async_trait;
use futures::FutureExt;
use futures::{AsyncRead, AsyncWrite, SinkExt, StreamExt};
use std::{sync::Arc, time::Duration};
use tokio::{
  self,
  net::TcpListener,
  select,
  sync::{
    mpsc::{channel, Receiver, Sender},
    Notify, OnceCell,
  },
};
use tokio_util::sync::CancellationToken;

async fn run_connection_loop<S>(
  ws_stream: async_tungstenite::WebSocketStream<S>,
  mut request_receiver: Receiver<EngineMessage>,
  response_sender: Sender<IntifaceMessage>,
  disconnect_notifier: Arc<Notify>,
  cancellation_token: CancellationToken,
) where
  S: AsyncRead + AsyncWrite + Unpin,
{
  info!("Starting websocket server connection event loop.");

  let (mut websocket_server_sender, mut websocket_server_receiver) = ws_stream.split();

  // Start pong count at 1, so we'll clear it after sending our first ping.
  let mut pong_count = 1u32;
  let mut sleep = tokio::time::sleep(Duration::from_secs(1));

  loop {
    sleep = tokio::time::sleep(Duration::from_secs(1));
    select! {
      _ = disconnect_notifier.notified() => {
        info!("Websocket server connector requested disconnect.");
        if websocket_server_sender.close().await.is_err() {
          warn!("Cannot close, assuming connection already closed");
          return;
        }
      },
      _ = sleep => {
        if pong_count == 0 {
          warn!("No pongs received, considering connection closed.");
          return;
        }
        pong_count = 0;
        if websocket_server_sender
          .send(async_tungstenite::tungstenite::Message::Ping(vec!(0)))
          .await
          .is_err() {
          warn!("Cannot send ping to client, considering connection closed.");
          return;
        }
      },
      serialized_msg = request_receiver.recv() => {
        if let Some(serialized_msg) = serialized_msg {
          if websocket_server_sender
            .send(async_tungstenite::tungstenite::Message::Text(serde_json::to_string(&serialized_msg).unwrap()))
            .await
            .is_err() {
            warn!("Cannot send text value to server, considering connection closed.");
            return;
          }
        } else {
          info!("Websocket server connector owner dropped, disconnecting websocket connection.");
          cancellation_token.cancel();
          if websocket_server_sender.close().await.is_err() {
            warn!("Cannot close, assuming connection already closed");
          }
          return;
        }
      }
      websocket_server_msg = websocket_server_receiver.next().fuse() => match websocket_server_msg {
        Some(ws_data) => {
          match ws_data {
            Ok(msg) => {
              match msg {
                async_tungstenite::tungstenite::Message::Text(text_msg) => {
                  trace!("Got text: {}", text_msg);
                  if response_sender.send(serde_json::from_str(&text_msg).unwrap()).await.is_err() {
                    warn!("Connector that owns transport no longer available, exiting.");
                    break;
                  }
                }
                async_tungstenite::tungstenite::Message::Close(_) => {
                  cancellation_token.cancel();
                  if websocket_server_sender.close().await.is_err() {
                    warn!("Cannot close, assuming connection already closed");
                    return;
                  }
                  //let _ = response_sender.send(ButtplugTransportIncomingMessage::Close("Websocket server closed".to_owned())).await;
                  break;
                }
                async_tungstenite::tungstenite::Message::Ping(_) => {
                  // noop
                  continue;
                }
                async_tungstenite::tungstenite::Message::Frame(_) => {
                  // noop
                  continue;
                }
                async_tungstenite::tungstenite::Message::Pong(_) => {
                  // noop
                  pong_count += 1;
                  continue;
                }
                async_tungstenite::tungstenite::Message::Binary(_) => {
                  error!("Don't know how to handle binary message types!");
                }
              }
            },
            Err(err) => {
              cancellation_token.cancel();
              warn!("Error from websocket server, assuming disconnection: {:?}", err);
              //let _ = response_sender.send(ButtplugTransportIncomingMessage::Close("Websocket server closed".to_owned())).await;
              break;
            }
          }
        },
        None => {
          warn!("Websocket channel closed, breaking");
          return;
        }
      }
    }
  }
}

#[derive(Clone, Debug)]
pub struct WebsocketFrontend {
  sender: OnceCell<Sender<EngineMessage>>,
  port: u16,
  disconnect_notifier: Arc<Notify>,
  cancellation_token: CancellationToken,
}

impl WebsocketFrontend {
  pub fn new(port: u16, cancellation_token: CancellationToken) -> Self {
    Self {
      sender: OnceCell::new(),
      disconnect_notifier: Arc::new(Notify::new()),
      port,
      cancellation_token,
    }
  }
}

#[async_trait]
impl Frontend for WebsocketFrontend {
  async fn send(&self, msg: EngineMessage) {
    if let Some(sender) = self.sender.get() {
      sender.send(msg).await.unwrap();
    }
  }

  async fn connect(&self) -> Result<(), IntifaceError> {
    let disconnect_notifier = self.disconnect_notifier.clone();

    let (incoming_sender, incoming_receiver) = channel::<IntifaceMessage>(256);
    let (outgoing_sender, outgoing_receiver) = channel::<EngineMessage>(256);

    self.sender.set(outgoing_sender).unwrap();
    let base_addr = "127.0.0.1";

    let addr = format!("{}:{}", base_addr, self.port);
    debug!("Websocket: Trying to listen on {}", addr);
    let response_sender_clone = incoming_sender;
    let disconnect_notifier_clone = disconnect_notifier;

    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&addr).await;
    debug!("Websocket: Socket bound.");
    let listener = try_socket.map_err(|e| IntifaceError::new(&format!("Websocket bind error: {:?}", e)))?;
    debug!("Websocket: Listening on: {}", addr);
    if let Ok((stream, _)) = listener.accept().await {
      info!("Websocket: Got connection");
      let ws_fut = async_tungstenite::tokio::accept_async(stream);
      let ws_stream = ws_fut.await.map_err(|err| {
        error!("Websocket server accept error: {:?}", err);
        IntifaceError::new(&format!("Websocket server accept error: {:?}", err))
      })?;
      let cancellation_token = self.cancellation_token.clone();
      tokio::spawn(async move {
        run_connection_loop(
          ws_stream,
          outgoing_receiver,
          response_sender_clone,
          disconnect_notifier_clone,
          cancellation_token,
        )
        .await;
      });
      Ok(())
    } else {
      Err(IntifaceError::new("Cannot run accept on websocket frontend port."))
    }
  }

  fn disconnect(self) {
    self.disconnect_notifier.notify_waiters();
  }
}
