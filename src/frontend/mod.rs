pub mod process_messages;
use crate::remote_server::ButtplugRemoteServerEvent;
use crate::{error::IntifaceError, options::EngineOptions};
use async_trait::async_trait;
use futures::{pin_mut, Stream, StreamExt};
use mdns_sd::{ServiceDaemon, ServiceInfo};
pub use process_messages::{EngineMessage, IntifaceMessage};
use rand::distributions::{Alphanumeric, DistString};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::{
  select,
  sync::{broadcast, Notify},
};
use tokio_util::sync::CancellationToken;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[async_trait]
pub trait Frontend: Sync + Send {
  async fn send(&self, msg: EngineMessage);
  async fn connect(&self) -> Result<(), IntifaceError>;
  fn disconnect_notifier(&self) -> Arc<Notify>;
  fn disconnect(&self);
  fn event_stream(&self) -> broadcast::Receiver<IntifaceMessage>;
}

pub async fn frontend_external_event_loop(
  frontend: Arc<dyn Frontend>,
  connection_cancellation_token: Arc<CancellationToken>,
) {
  let mut external_receiver = frontend.event_stream();
  loop {
    select! {
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
        info!("Connection cancellation token activated, breaking from frontend external event loop.");
        break;
      }
    }
  }
}

pub async fn frontend_server_event_loop(
  options: &EngineOptions,
  receiver: impl Stream<Item = ButtplugRemoteServerEvent>,
  frontend: Arc<dyn Frontend>,
  connection_cancellation_token: CancellationToken,
) {
  pin_mut!(receiver);

  let mut mdns = None;

  if options.broadcast_server_mdns() {
    // Create a daemon
    let mdns_daemon = ServiceDaemon::new().expect("Failed to create daemon");

    // Create a service info.
    let service_type = "_intiface_engine._tcp.local.";
    let random_suffix = Alphanumeric.sample_string(&mut rand::thread_rng(), 6);
    let instance_name = format!(
      "intiface_engine_{}_{}",
      options
        .mdns_suffix()
        .as_ref()
        .unwrap_or(&"".to_owned())
        .to_owned(),
      random_suffix
    );
    info!(
      "Bringing up mDNS Advertisment using instance name {}",
      instance_name
    );
    let host_name = format!("{}.local.", instance_name);
    let port = options.websocket_port().unwrap_or(12345);
    let properties: HashMap<String, String> = HashMap::new();
    let mut my_service = ServiceInfo::new(
      service_type,
      &instance_name,
      &host_name,
      "",
      port,
      properties,
    )
    .unwrap();
    my_service = my_service.enable_addr_auto();
    mdns_daemon.register(my_service).unwrap();
    mdns = Some(mdns_daemon);
  }

  loop {
    select! {
      maybe_event = receiver.next() => {
        match maybe_event {
          Some(event) => match event {
            ButtplugRemoteServerEvent::ClientConnected(client_name) => {
              info!("Client connected: {}", client_name);
              frontend.send(EngineMessage::ClientConnected{client_name}).await;
              if let Some(mdns_daemon) = &mdns {
                mdns_daemon.shutdown().unwrap();
              }
            }
            ButtplugRemoteServerEvent::ClientDisconnected => {
              info!("Client disconnected.");
              frontend
                .send(EngineMessage::ClientDisconnected{})
                .await;
            }
            ButtplugRemoteServerEvent::DeviceAdded { index: device_id, name: device_name, identifier: device_address, display_name: device_display_name } => {
              info!("Device Added: {} - {} - {:?}", device_id, device_name, device_address);
              frontend
                .send(EngineMessage::DeviceConnected { name: device_name, index: device_id, identifier: device_address, display_name: device_display_name })
                .await;
            }
            ButtplugRemoteServerEvent::DeviceRemoved { index: device_id } => {
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
        info!("Connection cancellation token activated, breaking from frontend server event loop");
        break;
      }
    }
  }
  if let Some(mdns_daemon) = mdns {
    if let Err(e) = mdns_daemon.shutdown() {
      error!("{:?}", e);
    }
  }
  info!("Exiting server event receiver loop");
}

#[derive(Default)]
struct NullFrontend {
  notify: Arc<Notify>,
}

#[async_trait]
impl Frontend for NullFrontend {
  async fn send(&self, _: EngineMessage) {}
  async fn connect(&self) -> Result<(), IntifaceError> {
    Ok(())
  }
  fn disconnect(&self) {
    self.notify.notify_waiters();
  }
  fn disconnect_notifier(&self) -> Arc<Notify> {
    self.notify.clone()
  }
  fn event_stream(&self) -> broadcast::Receiver<IntifaceMessage> {
    let (_, receiver) = broadcast::channel(255);
    receiver
  }
}

