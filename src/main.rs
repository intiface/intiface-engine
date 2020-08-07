/// command line interface for intiface/buttplug.
///

#[macro_use]
extern crate log;

mod frontend;
mod options;
mod utils;

use async_channel::{bounded, Receiver};
use async_std::task;
#[cfg(target_os = "windows")]
use buttplug::server::comm_managers::xinput::XInputDeviceCommunicationManager;
use buttplug::{
  connector::{
    ButtplugRemoteServerConnector, ButtplugWebsocketServerTransport,
    ButtplugWebsocketServerTransportOptions,
  },
  core::{
    errors::ButtplugError,
    messages::{serializer::ButtplugServerJSONSerializer, ButtplugServerMessage},
  },
  server::{
    comm_managers::{
      btleplug::BtlePlugCommunicationManager,
      lovense_dongle::{
        LovenseHIDDongleCommunicationManager, LovenseSerialDongleCommunicationManager,
      },
      serialport::SerialPortCommunicationManager,
      DeviceCommunicationManager, DeviceCommunicationManagerCreator,
    },
    ButtplugRemoteServer,
  },
};
use frontend::intiface_gui::server_process_message::{
  Msg, ProcessEnded, ProcessLog, ProcessStarted,
};
use frontend::{intiface_gui::server_process_message::ClientConnected, FrontendPBufSender};
use futures::StreamExt;
use std::{error::Error, fmt};

#[derive(Default, Clone)]
pub struct ConnectorOptions {
  server_name: String,
  max_ping_time: u64,
  stay_open: bool,
  use_frontend_pipe: bool,
  ws_listen_on_all_interfaces: bool,
  ws_insecure_port: Option<u16>,
  ws_secure_port: Option<u16>,
  ws_cert_file: Option<String>,
  ws_priv_file: Option<String>,
  ipc_pipe_name: Option<String>,
}

impl From<ConnectorOptions> for ButtplugWebsocketServerTransportOptions {
  fn from(options: ConnectorOptions) -> ButtplugWebsocketServerTransportOptions {
    ButtplugWebsocketServerTransportOptions {
      ws_cert_file: options.ws_cert_file,
      ws_priv_file: options.ws_priv_file,
      ws_insecure_port: options.ws_insecure_port,
      ws_secure_port: options.ws_secure_port,
      ws_listen_on_all_interfaces: options.ws_listen_on_all_interfaces,
    }
  }
}

#[derive(Debug)]
pub struct IntifaceError {
  reason: String,
}

impl IntifaceError {
  pub fn new(error_msg: &str) -> Self {
    Self {
      reason: error_msg.to_owned(),
    }
  }
}

impl fmt::Display for IntifaceError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "self.reason")
  }
}

impl Error for IntifaceError {
  fn source(&self) -> Option<&(dyn Error + 'static)> {
    None
  }
}

#[derive(Debug)]
pub enum IntifaceCLIErrorEnum {
  IoError(std::io::Error),
  ButtplugError(ButtplugError),
  IntifaceError(IntifaceError),
}

impl From<std::io::Error> for IntifaceCLIErrorEnum {
  fn from(err: std::io::Error) -> Self {
    IntifaceCLIErrorEnum::IoError(err)
  }
}

impl From<ButtplugError> for IntifaceCLIErrorEnum {
  fn from(err: ButtplugError) -> Self {
    IntifaceCLIErrorEnum::ButtplugError(err)
  }
}

impl From<IntifaceError> for IntifaceCLIErrorEnum {
  fn from(err: IntifaceError) -> Self {
    IntifaceCLIErrorEnum::IntifaceError(err)
  }
}

fn try_add_comm_manager<T>(server: &ButtplugRemoteServer)
where
  T: 'static + DeviceCommunicationManager + DeviceCommunicationManagerCreator,
{
  if let Err(e) = server.add_comm_manager::<T>() {
    info!("Can't add Btleplug Comm Manager: {:?}", e);
  }
}

fn setup_server_device_comm_managers(server: &ButtplugRemoteServer) {
  try_add_comm_manager::<BtlePlugCommunicationManager>(server);
  try_add_comm_manager::<LovenseHIDDongleCommunicationManager>(server);
  try_add_comm_manager::<LovenseSerialDongleCommunicationManager>(server);
  try_add_comm_manager::<SerialPortCommunicationManager>(server);
  try_add_comm_manager::<XInputDeviceCommunicationManager>(server);
}

#[allow(dead_code)]
fn setup_frontend_filter_channel<T>(
  mut receiver: Receiver<ButtplugServerMessage>,
  frontend_sender: FrontendPBufSender,
) -> Receiver<ButtplugServerMessage> {
  let (sender_filtered, recv_filtered) = bounded(256);

  task::spawn(async move {
    loop {
      match receiver.next().await {
        Some(msg) => {
          match msg {
            ButtplugServerMessage::ServerInfo(_) => {
              let msg = ClientConnected {
                client_name: "Unknown Name".to_string(),
              };
              frontend_sender.send(Msg::ClientConnected(msg)).await;
            }
            _ => {}
          }
          sender_filtered.send(msg).await.unwrap();
        }
        None => break,
      }
    }
  });

  recv_filtered
}

#[async_std::main]
async fn main() -> Result<(), IntifaceCLIErrorEnum> {
  // Intiface GUI communicates with its child process via protobufs through
  // stdin/stdout. Checking for this is the first thing we should do, as any
  // output after this either needs to be printed strings or pbuf messages.
  //
  // Only set up the env logger if we're not outputting pbufs to a frontend
  // pipe.
  let frontend_sender = options::check_options_and_pipe();
  #[allow(unused_variables)]
  if !frontend_sender.is_active() {
    tracing_subscriber::fmt::init();
  } else {
    frontend_sender
      .send(Msg::ProcessLog(ProcessLog {
        message: "Testing message".to_string(),
      }))
      .await;
    frontend_sender
      .send(Msg::ProcessStarted(ProcessStarted::default()))
      .await;
  }
  // Parse options, get back our connection information and a curried server
  // factory closure.
  let connector_opts = match options::parse_options() {
    Ok(opts) => match opts {
      Some(o) => o,
      None => return Ok(()),
    },
    Err(e) => return Err(e),
  };

  // Hang out until those listeners get sick of listening.
  info!("Intiface CLI Setup finished, running server tasks until all joined.");

  if connector_opts.stay_open {
    task::block_on(async move {
      let server =
        ButtplugRemoteServer::new(&connector_opts.server_name, connector_opts.max_ping_time);
      setup_server_device_comm_managers(&server);
      info!("Starting new stay open loop");
      loop {
        info!("Creating new stay open connector");
        let connector = ButtplugRemoteServerConnector::<
          ButtplugWebsocketServerTransport,
          ButtplugServerJSONSerializer,
        >::new(ButtplugWebsocketServerTransport::new(
          connector_opts.clone().into(),
        ));
        info!("Starting server");
        server.start(connector).await.unwrap();
        info!("Server connection dropped, restarting");
      }
    });
  } else {
    task::block_on(async move {
      loop {
        let server =
          ButtplugRemoteServer::new(&connector_opts.server_name, connector_opts.max_ping_time);
        setup_server_device_comm_managers(&server);
        let connector = ButtplugRemoteServerConnector::<
          ButtplugWebsocketServerTransport,
          ButtplugServerJSONSerializer,
        >::new(ButtplugWebsocketServerTransport::new(
          connector_opts.clone().into(),
        ));
        server.start(connector).await.unwrap();
      }
    });
  }

  info!("Exiting");
  frontend_sender
    .send(Msg::ProcessEnded(ProcessEnded::default()))
    .await;
  Ok(())
}
