/// command line interface for intiface/buttplug.
///

#[macro_use]
extern crate log;

mod frontend;
mod options;

use buttplug::{
  connector::{
    ButtplugRemoteServerConnector, ButtplugWebsocketServerTransport,
    ButtplugWebsocketServerTransportOptions,
  },
  core::{
    errors::ButtplugError,
    messages::{serializer::ButtplugServerJSONSerializer, ButtplugServerMessage},
  },
  server::{remote_server::ButtplugRemoteServerEvent, ButtplugRemoteServer, ButtplugServerOptions},
  util::logging::ChannelWriter,
};
use frontend::intiface_gui::server_process_message::{
  ClientConnected, ClientDisconnected, DeviceConnected, DeviceDisconnected, Msg, ProcessEnded,
  ProcessError, ProcessLog, ProcessStarted,
};
use frontend::FrontendPBufChannel;
use futures::{pin_mut, select, FutureExt, Stream, StreamExt};
use log_panics;
use std::{error::Error, fmt};
use tokio::{
  self,
  signal::ctrl_c,
  sync::mpsc::{channel, Receiver},
};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::filter::EnvFilter;

#[derive(Default, Clone)]
pub struct ConnectorOptions {
  server_options: ButtplugServerOptions,
  stay_open: bool,
  use_frontend_pipe: bool,
  ws_listen_on_all_interfaces: bool,
  ws_insecure_port: Option<u16>,
  ipc_pipe_name: Option<String>,
}

impl From<ConnectorOptions> for ButtplugWebsocketServerTransportOptions {
  fn from(options: ConnectorOptions) -> ButtplugWebsocketServerTransportOptions {
    ButtplugWebsocketServerTransportOptions {
      ws_insecure_port: options.ws_insecure_port.unwrap(),
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

#[allow(dead_code)]
fn setup_frontend_filter_channel<T>(
  mut receiver: Receiver<ButtplugServerMessage>,
  frontend_channel: FrontendPBufChannel,
) -> Receiver<ButtplugServerMessage> {
  let (sender_filtered, recv_filtered) = channel(256);

  tokio::spawn(async move {
    loop {
      match receiver.recv().await {
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

async fn server_event_receiver(
  receiver: impl Stream<Item = ButtplugRemoteServerEvent>,
  frontend_sender: FrontendPBufChannel,
) {
  pin_mut!(receiver);
  while let Some(event) = receiver.next().await {
    match event {
      ButtplugRemoteServerEvent::Connected(client_name) => {
        frontend_sender
          .send(Msg::ClientConnected(ClientConnected {
            client_name: client_name,
          }))
          .await;
      }
      ButtplugRemoteServerEvent::Disconnected => {
        frontend_sender
          .send(Msg::ClientDisconnected(ClientDisconnected {}))
          .await;
      }
      ButtplugRemoteServerEvent::DeviceAdded(device_id, device_name) => {
        frontend_sender
          .send(Msg::DeviceConnected(DeviceConnected {
            device_name,
            device_id,
          }))
          .await;
      }
      ButtplugRemoteServerEvent::DeviceRemoved(device_id) => {
        frontend_sender
          .send(Msg::DeviceDisconnected(DeviceDisconnected { device_id }))
          .await;
      }
    }
  }
  frontend_sender
    .send(Msg::ClientDisconnected(ClientDisconnected {}))
    .await;
}

#[tokio::main]
async fn main() -> Result<(), IntifaceCLIErrorEnum> {
  let parent_token = CancellationToken::new();
  let process_token = parent_token.child_token();
  // Intiface GUI communicates with its child process via protobufs through
  // stdin/stdout. Checking for this is the first thing we should do, as any
  // output after this either needs to be printed strings or pbuf messages.
  //
  // Only set up the env logger if we're not outputting pbufs to a frontend
  // pipe.
  let frontend_sender = options::check_frontend_pipe(parent_token);
  let log_level = options::check_log_level();
  #[allow(unused_variables)]
  if let Some(sender) = &frontend_sender {
    // Add panic hook for emitting backtraces through the logging system.
    log_panics::init();
    sender
      .send(Msg::ProcessStarted(ProcessStarted::default()))
      .await;
    let (bp_log_sender, mut receiver) = channel::<Vec<u8>>(256);
    let log_sender = sender.clone();
    tokio::spawn(async move {
      while let Some(log) = receiver.recv().await {
        log_sender
          .send(Msg::ProcessLog(ProcessLog {
            message: std::str::from_utf8(&log).unwrap().to_owned(),
          }))
          .await;
      }
    });
    tracing_subscriber::fmt()
      .json()
      .with_max_level(log_level)
      .with_ansi(false)
      .with_writer(ChannelWriter::new(bp_log_sender))
      .init();
  } else {
    if log_level.is_some() {
      tracing_subscriber::fmt()
        .with_max_level(log_level.unwrap())
        .init();
    } else {
      let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();
      tracing_subscriber::fmt().with_env_filter(filter).init();
    }
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
    let server = ButtplugRemoteServer::new_with_options(&connector_opts.server_options).unwrap();
    let event_receiver = server.event_stream();
    if frontend_sender_clone.is_some() {
      let fscc = frontend_sender_clone.clone().unwrap();
      tokio::spawn(async move {
        server_event_receiver(event_receiver, fscc).await;
      });
    }
    options::setup_server_device_comm_managers(&server);
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
      let mut exit_requested = false;
      select! {
        _ = ctrl_c().fuse() => {
          info!("Control-c hit, exiting.");
          exit_requested = true;
        }
        _ = process_token.cancelled().fuse() => {
          info!("Owner requested process exit, exiting.");
          exit_requested = true;
        }
        result = server.start(connector).fuse() => {
          match result {
            Ok(_) => info!("Connection dropped, restarting stay open loop."),
            Err(e) => {
              error!("{}", format!("Process Error: {:?}", e));
              if let Some(sender) = &frontend_sender_clone {
                sender
                  .send(Msg::ProcessError(ProcessError { message: format!("Process Error: {:?}", e).to_owned() }))
                  .await;
              }
            }
          }
        }
      };
      if let Some(sender) = &frontend_sender_clone {
        sender
          .send(Msg::ClientDisconnected(ClientDisconnected::default()))
          .await;
      }
      if exit_requested {
        info!("Breaking out of event loop in order to exit");
        if let Some(sender) = &frontend_sender {
          sender
            .send(Msg::ProcessEnded(ProcessEnded::default()))
            .await;
        }
        break;
      }
      info!("Server connection dropped, restarting");
    }
  } else {
    let server = ButtplugRemoteServer::new_with_options(&connector_opts.server_options).unwrap();
    let event_receiver = server.event_stream();
    let fscc = frontend_sender_clone.clone();
    if fscc.is_some() {
      tokio::spawn(async move {
        server_event_receiver(event_receiver, fscc.unwrap()).await;
      });
    }
    options::setup_server_device_comm_managers(&server);
    let connector = ButtplugRemoteServerConnector::<
      ButtplugWebsocketServerTransport,
      ButtplugServerJSONSerializer,
    >::new(ButtplugWebsocketServerTransport::new(
      connector_opts.clone().into(),
    ));
    select! {
      _ = ctrl_c().fuse() => {
        info!("Control-c hit, exiting.");
      }
      _ = process_token.cancelled().fuse() => {
        info!("Owner requested process exit, exiting.");
      }
      result = server.start(connector).fuse() => {
        match result {
          Ok(_) => info!("Connection dropped, restarting stay open loop."),
          Err(e) => {
            error!("{}", format!("Process Error: {:?}", e));
            if let Some(sender) = &frontend_sender_clone {
              sender
                .send(Msg::ProcessError(ProcessError { message: format!("Process Error: {:?}", e).to_owned() }))
                .await;
            }
          }
        }
      }
    };
    if let Some(sender) = &frontend_sender_clone {
      sender
        .send(Msg::ClientDisconnected(ClientDisconnected::default()))
        .await;
    }
  }

  info!("Exiting");
  Ok(())
}
