#![recursion_limit = "256"]

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
    ButtplugServerOptions,
    remote_server::ButtplugRemoteServerEvent,
  },
  util::logging::ChannelWriter
};
use frontend::intiface_gui::server_process_message::{
  Msg, ProcessEnded, ProcessLog, ProcessStarted, ProcessError,
  ClientConnected, ClientDisconnected, DeviceConnected, DeviceDisconnected,
};
use frontend::FrontendPBufChannel;
use futures::StreamExt;
use tracing_subscriber::{prelude::*, filter::{LevelFilter, EnvFilter}};
use std::{error::Error, fmt};

#[derive(Default, Clone)]
pub struct ConnectorOptions {
  server_options: ButtplugServerOptions,
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
  #[cfg(target_os = "windows")]
  try_add_comm_manager::<XInputDeviceCommunicationManager>(server);
}

#[allow(dead_code)]
fn setup_frontend_filter_channel<T>(
  mut receiver: Receiver<ButtplugServerMessage>,
  frontend_channel: FrontendPBufChannel,
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
              frontend_channel.send(Msg::ClientConnected(msg)).await;
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

async fn server_event_receiver(mut receiver: Receiver<ButtplugRemoteServerEvent>, frontend_sender: FrontendPBufChannel) {
  while let Some(event) = receiver.next().await {
    match event {
      ButtplugRemoteServerEvent::Connected(client_name) => {
        frontend_sender
          .send(Msg::ClientConnected(ClientConnected {
            client_name: client_name
          })).await;
      },
      ButtplugRemoteServerEvent::Disconnected => {
        frontend_sender
          .send(Msg::ClientDisconnected(ClientDisconnected {
          })).await;
      },
      ButtplugRemoteServerEvent::DeviceAdded(device_id, device_name) => {
        frontend_sender
          .send(Msg::DeviceConnected(DeviceConnected {
            device_name,
            device_id
          })).await;
      }
      ButtplugRemoteServerEvent::DeviceRemoved(device_id) => {
        frontend_sender
          .send(Msg::DeviceDisconnected(DeviceDisconnected {
            device_id
          })).await;
      }
    }
  }
  frontend_sender
  .send(Msg::ClientDisconnected(ClientDisconnected {
  })).await;
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
  if let Some(sender) = &frontend_sender {
    sender
    .send(Msg::ProcessStarted(ProcessStarted::default()))
    .await;
    let (bp_log_sender, mut receiver) = bounded::<Vec<u8>>(256);
    let log_sender = sender.clone();
    async_std::task::spawn(async move {
      while let Some(log) = receiver.next().await {
        log_sender
          .send(Msg::ProcessLog(ProcessLog {
            message: std::str::from_utf8(&log).unwrap().to_owned()
          }))
          .await;
      }
    });
    let sub = tracing_subscriber::fmt::layer().with_ansi(false).with_writer(ChannelWriter::new(bp_log_sender));
    tracing_subscriber::registry().with(LevelFilter::INFO).with(sub).init();
  } else {
    let filter = EnvFilter::try_from_default_env().or_else(|_| EnvFilter::try_new("info")).unwrap();
    tracing_subscriber::fmt().with_env_filter(filter).init();
    println!("Intiface Server, starting up with stdout output.");
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
  let frontend_sender_clone = frontend_sender.clone();    
  if connector_opts.stay_open {  
    task::block_on(async move {
      let (server, event_receiver) =
        ButtplugRemoteServer::new_with_options(&connector_opts.server_options).unwrap();
      if frontend_sender_clone.is_some() {
        let fscc = frontend_sender_clone.clone().unwrap();
        task::spawn(async move {
          server_event_receiver(event_receiver, fscc).await;
        });
      }
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
        if let Err(e) = server.start(connector).await {
          if let Some(sender) = &frontend_sender_clone {
            sender
              .send(Msg::ProcessError(ProcessError { message: format!("Process Error: {:?}", e).to_owned() }))
              .await;
          } else {
            println!("{}", format!("Process Error: {:?}", e));
          }
        }        
        info!("Server connection dropped, restarting");
        if let Some(sender) = &frontend_sender_clone {
          sender
            .send(Msg::ClientDisconnected(ClientDisconnected::default()))
            .await;
        }
      }
    });
  } else {
    task::block_on(async move {
      let (server, event_receiver) =
        ButtplugRemoteServer::new_with_options(&connector_opts.server_options).unwrap();
      let fscc = frontend_sender_clone.clone();
      if fscc.is_some() {
        task::spawn(async move {
          server_event_receiver(event_receiver, fscc.unwrap()).await;
        });
      }
      setup_server_device_comm_managers(&server);
      let connector = ButtplugRemoteServerConnector::<
        ButtplugWebsocketServerTransport,
        ButtplugServerJSONSerializer,
      >::new(ButtplugWebsocketServerTransport::new(
        connector_opts.clone().into(),
      ));
      if let Err(e) = server.start(connector).await {
        if let Some(sender) = &frontend_sender_clone {
          sender
            .send(Msg::ProcessError(ProcessError { message: format!("Process Error: {:?}", e.source()).to_owned() }))
            .await;
        } else {
          println!("{}", format!("Process Error: {:?}", e));
        }
      }
      if let Some(sender) = &frontend_sender_clone {
        sender
          .send(Msg::ClientDisconnected(ClientDisconnected::default()))
          .await;
      }
    });
  }

  info!("Exiting");
  if let Some(sender) = &frontend_sender {
    sender
      .send(Msg::ProcessEnded(ProcessEnded::default()))
      .await;
  }
  Ok(())
}
