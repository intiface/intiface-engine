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
use tokio::{select, sync::broadcast};
use buttplug::server::ButtplugRemoteServerEvent;
use tokio_util::sync::CancellationToken;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[async_trait]
pub trait Frontend: Sync + Send {
  async fn send(&self, msg: EngineMessage);
  async fn connect(&self) -> Result<(), IntifaceError>;
  fn disconnect(self);
  fn event_stream(&self) -> broadcast::Receiver<IntifaceMessage>;
}

pub async fn frontend_external_event_loop(
  frontend: Arc<dyn Frontend>,
  connection_cancellation_token: Arc<CancellationToken>,
) {
  let mut external_receiver = frontend.event_stream();
  loop {
    select!{
      external_message = external_receiver.recv() => {
        match external_message {
          Ok(message) => match message {
            IntifaceMessage::RequestEngineVersion{expected_version:_} => {
              // TODO We should check the version here and shut down on mismatch.
              info!("Engine version request received from frontend.");
              frontend
                .send(EngineMessage::EngineVersion{ version: VERSION.to_owned() })
                .await;
            },
            IntifaceMessage::Stop{} => {
              connection_cancellation_token.cancel();
              info!("Got external stop request");
              break;
            }
          },
          Err(_) => {
            info!("Frontend sender dropped, assuming connection lost, breaking.");
            break;
          }
        }
      },
      _ = connection_cancellation_token.cancelled() => {
        info!("Connection cancellation token activated, breaking");
        break;
      }
    }
  }
}

pub async fn frontend_server_event_loop(
  receiver: impl Stream<Item = ButtplugRemoteServerEvent>,
  frontend: Arc<dyn Frontend>,
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
              frontend.send(EngineMessage::ClientConnected{client_name}).await;
            }
            ButtplugRemoteServerEvent::Disconnected => {
              info!("Client disconnected.");
              frontend
                .send(EngineMessage::ClientDisconnected{})
                .await;
            }
            ButtplugRemoteServerEvent::DeviceAdded(device_id, device_name, device_address, device_display_name) => {
              info!("Device Added: {} - {} - {}", device_id, device_name, device_address);
              frontend
                .send(EngineMessage::DeviceConnected { name: device_name, index: device_id, address: device_address, display_name: device_display_name })
                .await;
            }
            ButtplugRemoteServerEvent::DeviceRemoved(device_id) => {
              info!("Device Removed: {}", device_id);
              frontend
                .send(EngineMessage::DeviceDisconnected{index: device_id})
                .await;
            }
          },
          None => {
            info!("Lost connection with main thread, breaking.");
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
}

#[derive(Default)]
struct NullFrontend {}

#[async_trait]
impl Frontend for NullFrontend {
  async fn send(&self, _: EngineMessage) {}
  async fn connect(&self) -> Result<(), IntifaceError> { Ok(()) }
  fn disconnect(self) {}
  fn event_stream(&self) -> broadcast::Receiver<IntifaceMessage> {
    let (_, receiver) = broadcast::channel(255);
    receiver
  }
}

pub async fn setup_frontend(options: &EngineOptions, cancellation_token: &Arc<CancellationToken>) -> Arc<dyn Frontend> {
  if let Some(frontend_websocket_port) = options.frontend_websocket_port() {
    Arc::new(WebsocketFrontend::new(frontend_websocket_port, cancellation_token.clone()))
  } else {
    Arc::new(NullFrontend::default())
  }
}