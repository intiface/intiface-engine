/// command line interface for intiface/buttplug.
///

#[macro_use]
extern crate log;

mod frontend;
mod options;
mod process_messages;

use buttplug::{
  connector::{
    ButtplugRemoteServerConnector, ButtplugWebsocketServerTransport,
    ButtplugWebsocketServerTransportBuilder,
  },
  core::{
    errors::ButtplugError,
    messages::{serializer::ButtplugServerJSONSerializer, ButtplugServerMessage},
  },
  server::{remote_server::ButtplugRemoteServerEvent, ButtplugRemoteServer, ButtplugServerBuilder},
  util::logging::ChannelWriter,
};
use frontend::FrontendPBufChannel;
use futures::{pin_mut, select, FutureExt, Stream, StreamExt};
use log_panics;
use process_messages::EngineMessage;
use std::{error::Error, fmt, time::Duration};
use tokio::{
  self,
  net::TcpListener,
  signal::ctrl_c,
  sync::mpsc::{channel, Receiver},
  time::sleep,
};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::filter::EnvFilter;

#[derive(Default, Clone)]
pub struct ConnectorOptions {
  server_builder: ButtplugServerBuilder,
  stay_open: bool,
  use_frontend_pipe: bool,
  ws_listen_on_all_interfaces: bool,
  ws_insecure_port: Option<u16>,
  ipc_pipe_name: Option<String>,
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
              let msg = EngineMessage::ClientConnected("Unknown Name".to_string());
              frontend_channel.send(msg).await;
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
  frontend_sender: Option<FrontendPBufChannel>,
  connection_cancellation_token: CancellationToken,
) {
  pin_mut!(receiver);
  loop {
    select! {
      maybe_event = receiver.next().fuse() => {
        match maybe_event {
          Some(event) => match event {
            ButtplugRemoteServerEvent::Connected(client_name) => {
              info!("Client connected: {}", client_name);
              let sender = frontend_sender.clone();
              let token = connection_cancellation_token.child_token();
              tokio::spawn(async move {
                reject_all_incoming(sender, "localhost", 12345, token).await;
              });
              if let Some(frontend_sender) = &frontend_sender {
                frontend_sender.send(EngineMessage::ClientConnected(client_name)).await;
              }
            }
            ButtplugRemoteServerEvent::Disconnected => {
              info!("Client disconnected.");
              if let Some(frontend_sender) = &frontend_sender {
                frontend_sender
                  .send(EngineMessage::ClientDisconnected)
                  .await;
              }
            }
            ButtplugRemoteServerEvent::DeviceAdded(device_id, device_name) => {
              info!("Device Added: {} - {}", device_id, device_name);
              if let Some(frontend_sender) = &frontend_sender {
                frontend_sender
                  .send(EngineMessage::DeviceConnected(device_name, device_id))
                  .await;
              }
            }
            ButtplugRemoteServerEvent::DeviceRemoved(device_id) => {
              info!("Device Removed: {}", device_id);
              if let Some(frontend_sender) = &frontend_sender {
                frontend_sender
                  .send(EngineMessage::DeviceDisconnected(device_id))
                  .await;
              }
            }
          },
          None => break,
        }
      },
      _ = connection_cancellation_token.cancelled().fuse() => {
        break;
      }
    }
  }
  info!("Exiting server event receiver loop");
  if let Some(frontend_sender) = &frontend_sender {
    frontend_sender
      .send(EngineMessage::ClientDisconnected)
      .await;
  }
}

async fn reject_all_incoming(
  frontend_sender: Option<FrontendPBufChannel>,
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
      _ = token.cancelled().fuse() => {
        break;
      }
      ret = listener.accept().fuse() =>  {
        match ret {
          Ok(_) => {
            error!("Someone tried to connect while we're already connected!!!!");
            if let Some(frontend_sender) = &frontend_sender {
              frontend_sender
                .send(EngineMessage::ClientRejected("Unknown".to_owned()))
                .await;
            }
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

#[tokio::main]
async fn main() -> Result<(), IntifaceCLIErrorEnum> {
  let parent_token = CancellationToken::new();
  let process_token = parent_token.child_token();
  let finish_token = CancellationToken::new();
  let log_token = finish_token.child_token();
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
    sender.send(EngineMessage::EngineStarted).await;
    let (bp_log_sender, mut receiver) = channel::<Vec<u8>>(256);
    let log_sender = sender.clone();
    tokio::spawn(async move {
      loop {
        select! {
          log = receiver.recv().fuse() => {
            let log = log.unwrap();
            log_sender
              .send(EngineMessage::EngineLog(std::str::from_utf8(&log).unwrap().to_owned()))
              .await;
          },
          _ = log_token.cancelled().fuse() => {
            break;
          }
        }
      }
    });
    tracing_subscriber::fmt()
      .json()
      .with_max_level(log_level)
      .with_ansi(false)
      .with_writer(move || ChannelWriter::new(bp_log_sender.clone()))
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

  let core_server = match connector_opts.server_builder.finish() {
    Ok(server) => server,
    Err(e) => {
      process_token.cancel();
      error!("Error starting server: {:?}", e);
      if let Some(sender) = &frontend_sender_clone {
        sender
          .send(EngineMessage::EngineError(
            format!("Process Error: {:?}", e).to_owned(),
          ))
          .await;
      }
      return Err(IntifaceCLIErrorEnum::ButtplugError(e));
    }
  };
  let server = ButtplugRemoteServer::new(core_server);
  options::setup_server_device_comm_managers(&server);
  info!("Starting new stay open loop");
  loop {
    let token = CancellationToken::new();
    let child_token = token.child_token();
    let event_receiver = server.event_stream();
    let fscc = frontend_sender_clone.clone();
    tokio::spawn(async move {
      server_event_receiver(event_receiver, fscc, child_token).await;
    });
    info!("Creating new stay open connector");
    let transport = ButtplugWebsocketServerTransportBuilder::default()
      .port(connector_opts.ws_insecure_port.unwrap())
      .listen_on_all_interfaces(connector_opts.ws_listen_on_all_interfaces)
      .finish();
    let connector = ButtplugRemoteServerConnector::<
      ButtplugWebsocketServerTransport,
      ButtplugServerJSONSerializer,
    >::new(transport);
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
                .send(EngineMessage::EngineError(format!("Process Error: {:?}", e).to_owned()))
                .await;
            }
            exit_requested = true;
            break;
          }
        }
      }
    };
    token.cancel();
    if let Some(sender) = &frontend_sender_clone {
      sender.send(EngineMessage::ClientDisconnected).await;
    }
    if !connector_opts.stay_open || exit_requested {
      info!("Breaking out of event loop in order to exit");
      if let Some(sender) = &frontend_sender {
        // If the ProcessEnded message is sent too soon after client disconnected, electron has a
        // tendency to miss it completely. This sucks.
        sleep(Duration::from_millis(100)).await;
        sender.send(EngineMessage::EngineStopped).await;
      }
      break;
    }
    info!("Server connection dropped, restarting");
  }

  finish_token.cancel();
  info!("Exiting");
  Ok(())
}
