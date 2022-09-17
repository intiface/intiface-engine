mod websocket_frontend;
pub mod process_messages;
use async_trait::async_trait;
use crate::{
  error::IntifaceError,
  options::EngineOptions
};
use websocket_frontend::WebsocketFrontend;
pub use process_messages::{EngineMessage, IntifaceMessage};
use std::{
  sync::Arc,
};
use futures::{
  Stream,
  StreamExt,
  pin_mut
};
use tokio::{select, net::TcpListener};
use buttplug::server::ButtplugRemoteServerEvent;
use tokio_util::sync::CancellationToken;

#[async_trait]
pub trait Frontend: Sync + Send {
  async fn send(&self, msg: EngineMessage);
  async fn connect(&self) -> Result<(), IntifaceError>;
  fn disconnect(self);
}

async fn reject_all_incoming(
  frontend_sender: Arc<dyn Frontend>,
  address: &str,
  port: u16,
  token: CancellationToken,
) {
  info!("Rejecting all incoming clients while connected");
  let addr = format!("{}:{}", address, port);
  let try_socket = TcpListener::bind(&addr).await;
  let listener = try_socket.expect("Cannot hold port while connected?!");

  loop {
    select! {
      _ = token.cancelled() => {
        break;
      }
      ret = listener.accept() =>  {
        match ret {
          Ok(_) => {
            error!("Someone tried to connect while we're already connected!!!!");
            frontend_sender
              .send(EngineMessage::ClientRejected{reason: "Unknown".to_owned()})
              .await;
          }
          Err(_) => {
            break;
          }
        }
      }
    }
  }
  info!("Leaving client rejection loop.");
}

pub async fn frontend_server_event_loop(
  receiver: impl Stream<Item = ButtplugRemoteServerEvent>,
  frontend_sender: Arc<dyn Frontend>,
  connection_cancellation_token: CancellationToken,
) {
  pin_mut!(receiver);
  loop {
    select! {
      maybe_event = receiver.next() => {
        match maybe_event {
          Some(event) => match event {
            ButtplugRemoteServerEvent::Connected(client_name) => {
              info!("Client connected: {}", client_name);
              let sender = frontend_sender.clone();
              let token = connection_cancellation_token.child_token();
              tokio::spawn(async move {
                reject_all_incoming(sender, "localhost", 12345, token).await;
              });
              frontend_sender.send(EngineMessage::ClientConnected{client_name}).await;
            }
            ButtplugRemoteServerEvent::Disconnected => {
              info!("Client disconnected.");
              frontend_sender
                .send(EngineMessage::ClientDisconnected{})
                .await;
            }
            ButtplugRemoteServerEvent::DeviceAdded(device_id, device_name, device_address, device_display_name) => {
              info!("Device Added: {} - {} - {}", device_id, device_name, device_address);
              frontend_sender
                .send(EngineMessage::DeviceConnected { name: device_name, index: device_id, address: device_address, display_name: device_display_name })
                .await;
            }
            ButtplugRemoteServerEvent::DeviceRemoved(device_id) => {
              info!("Device Removed: {}", device_id);
              frontend_sender
                .send(EngineMessage::DeviceDisconnected{index: device_id})
                .await;
            }
          },
          None => {
            warn!("Lost connection with main thread, breaking.");
            break;
          },
        }
      },
      _ = connection_cancellation_token.cancelled() => {
        info!("Connection cancellation token activated, breaking");
        break;
      }
    }
  }
  info!("Exiting server event receiver loop");
  frontend_sender
    .send(EngineMessage::ClientDisconnected{})
    .await;
}

#[derive(Default)]
struct NullFrontend {}

#[async_trait]
impl Frontend for NullFrontend {
  async fn send(&self, _: EngineMessage) {}
  async fn connect(&self) -> Result<(), IntifaceError> { Ok(()) }
  fn disconnect(self) {}
}

pub async fn setup_frontend(options: &EngineOptions, cancellation_token: &CancellationToken) -> Arc<dyn Frontend> {
  if let Some(frontend_websocket_port) = options.frontend_websocket_port() {
    Arc::new(WebsocketFrontend::new(frontend_websocket_port, cancellation_token.child_token()))
  } else {
    Arc::new(NullFrontend::default())
  }
}