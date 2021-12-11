/// command line interface for intiface/buttplug.
///

#[macro_use]
extern crate log;

mod frontend;
mod options;
mod process_messages;

use buttplug::{
  connector::{
    ButtplugRemoteServerConnector,
    ButtplugWebsocketServerTransportBuilder, 
    ButtplugPipeClientTransportBuilder,
  },
  core::{
    errors::ButtplugError,
    messages::{serializer::ButtplugServerJSONSerializer, ButtplugServerMessage},
  },
  server::{remote_server::{ButtplugRemoteServerEvent, ButtplugServerConnectorError}, ButtplugRemoteServer, ButtplugServerBuilder},
  util::logging::ChannelWriter,
};
use frontend::FrontendPBufChannel;
use futures::{FutureExt, Stream, StreamExt, pin_mut, select};
use log_panics;
use process_messages::EngineMessage;
use std::{error::Error, fmt, sync::Arc};
use tokio::{
  self,
  net::TcpListener,
  signal::ctrl_c,
  sync::mpsc::{channel, Receiver},
};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::{filter::EnvFilter, layer::SubscriberExt, util::SubscriberInitExt, filter::LevelFilter};

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
    write!(f, "{}", self.reason)
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
  server: Arc<ButtplugRemoteServer>,
  receiver: impl Stream<Item = ButtplugRemoteServerEvent>,
  frontend_sender: FrontendPBufChannel,
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
              frontend_sender.send(EngineMessage::ClientConnected(client_name)).await;
            }
            ButtplugRemoteServerEvent::Disconnected => {
              info!("Client disconnected.");
              frontend_sender
                .send(EngineMessage::ClientDisconnected)
                .await;
            }
            ButtplugRemoteServerEvent::DeviceAdded(device_id, device_name) => {
              info!("Device Added: {} - {}", device_id, device_name);
              let info = server.device_manager().device_info(device_id).unwrap();
              info!("Device Address: {:?}", info.address);
              frontend_sender
                .send(EngineMessage::DeviceConnected { name: device_name, index: device_id, address: info.address, display_name: info.display_name.unwrap_or("".to_string()) })
                .await;
            }
            ButtplugRemoteServerEvent::DeviceRemoved(device_id) => {
              info!("Device Removed: {}", device_id);
              frontend_sender
                .send(EngineMessage::DeviceDisconnected(device_id))
                .await;
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
  frontend_sender
    .send(EngineMessage::ClientDisconnected)
    .await;
}

async fn reject_all_incoming(
  frontend_sender: FrontendPBufChannel,
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
            frontend_sender
              .send(EngineMessage::ClientRejected("Unknown".to_owned()))
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

fn setup_logging(frontend_sender: FrontendPBufChannel, token: CancellationToken) {
  // Only set up the env logger if we're not outputting messages to a frontend pipe.
  let log_level = options::check_log_level();
  if frontend_sender.has_frontend() {
    // Add panic hook for emitting backtraces through the logging system.
    log_panics::init();
    let (bp_log_sender, mut receiver) = channel::<Vec<u8>>(256);
    let log_sender = frontend_sender.clone();
    tokio::spawn(async move {
      log_sender.send(EngineMessage::EngineStarted).await;
      loop {
        select! {
          log = receiver.recv().fuse() => {
            let log = log.unwrap();
            log_sender
              .send(EngineMessage::EngineLog(std::str::from_utf8(&log).unwrap().to_owned()))
              .await;
          },
          _ = token.cancelled().fuse() => {
            break;
          }
        }
      }
    });

    tracing_subscriber::registry()
      .with(LevelFilter::from(log_level))
      .with(tracing_subscriber::fmt::layer()
        .json()
        //.with_max_level(log_level)
        .with_ansi(false)
        .with_writer(move || ChannelWriter::new(bp_log_sender.clone()))
      )
      .with(sentry_tracing::layer())
      .try_init()
      .unwrap();
  } else {
      if log_level.is_some() {
        tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(sentry_tracing::layer())
        .with(LevelFilter::from(log_level))
        .try_init()
        .unwrap(); 
      } else {
        tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(sentry_tracing::layer())
        .with(EnvFilter::try_from_default_env()
          .or_else(|_| EnvFilter::try_new("info"))
          .unwrap())
        .try_init()
        .unwrap(); 
      };
      println!("Intiface Server, starting up with stdout output.");
  }
}

#[tokio::main]
async fn main() -> Result<(), IntifaceCLIErrorEnum> {
  const API_KEY: &str = include_str!(concat!(env!("OUT_DIR"), "/sentry_api_key.txt"));
  let sentry_guard = if options::should_turn_on_crash_reporting() && !API_KEY.is_empty() {
    Some(sentry::init((API_KEY, sentry::ClientOptions {
      release: sentry::release_name!(),
      ..Default::default()
    })))
  } else {
    None
  };
  
  let frontend_cancellation_token = CancellationToken::new();
  let frontend_cancellation_child_token = frontend_cancellation_token.child_token();
  let process_ended_token = CancellationToken::new();

  // Intiface GUI communicates with its child process via json through named pipes/domain sockets.
  // Checking for this is the first thing we should do, as any output after this either needs to be
  // printed strings or json messages.

  let frontend_sender = frontend::FrontendPBufChannel::create(frontend_cancellation_token, process_ended_token.child_token());

  setup_logging(frontend_sender.clone(), process_ended_token.child_token());

  if sentry_guard.is_some() {
    info!("Using sentry for crash logging.");
  } else {
    info!("Crash logging disabled.");
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
      error!("Error starting server: {:?}", e);
      frontend_sender_clone
        .send(EngineMessage::EngineError(
          format!("Process Error: {:?}", e).to_owned(),
        ))
        .await;
      process_ended_token.cancel();
      return Err(IntifaceCLIErrorEnum::ButtplugError(e));
    }
  };
  let server = Arc::new(ButtplugRemoteServer::new(core_server));
  options::setup_server_device_comm_managers(&server);
  info!("Starting new stay open loop");
  loop {
    let session_connection_token = CancellationToken::new();
    let session_connection_child_token = session_connection_token.child_token();
    let event_receiver = server.event_stream();
    let fscc = frontend_sender_clone.clone();
    let server_clone = server.clone();
    tokio::spawn(async move {
      server_event_receiver(server_clone, event_receiver, fscc, session_connection_child_token).await;
    });
    info!("Creating new stay open connector");

    async fn run_server(server: Arc<ButtplugRemoteServer>, connector_opts: &ConnectorOptions) -> Result<(), ButtplugServerConnectorError> {
      if let Some(port) = connector_opts.ws_insecure_port {
        server.start(ButtplugRemoteServerConnector::<_, ButtplugServerJSONSerializer>::new(ButtplugWebsocketServerTransportBuilder::default()
          .port(port)
          .listen_on_all_interfaces(connector_opts.ws_listen_on_all_interfaces)
          .finish())).await
      } else if let Some(pipe_name) = &connector_opts.ipc_pipe_name {
        server.start(ButtplugRemoteServerConnector::<_, ButtplugServerJSONSerializer>::new(ButtplugPipeClientTransportBuilder::new(&pipe_name)
          .finish())).await
      } else {
        panic!("Neither websocket port nor ipc pipe name are set, cannot create transport.");
      }
    }
    info!("Starting server");
    
    // Let everything spin up, then try crashing.
    #[cfg(debug_assertions)]
    options::maybe_crash_main_thread();
    
    let mut exit_requested = false;
    select! {
      _ = ctrl_c().fuse() => {
        info!("Control-c hit, exiting.");
        exit_requested = true;
      }
      _ = frontend_cancellation_child_token.cancelled().fuse() => {
        info!("Owner requested process exit, exiting.");
        exit_requested = true;
      }
      result = run_server(server.clone(), &connector_opts).fuse() => {
        match result {
          Ok(_) => info!("Connection dropped, restarting stay open loop."),
          Err(e) => {
            error!("{}", format!("Process Error: {:?}", e));
            frontend_sender_clone
              .send(EngineMessage::EngineError(format!("Process Error: {:?}", e).to_owned()))
              .await;
            exit_requested = true;
          }
        }
      }
    };
    match server.disconnect().await {
      Ok(_) => info!("Client forcefully disconnected from server."),
      Err(_) => info!("Client already disconnected from server.")
    };
    session_connection_token.cancel();
    frontend_sender.send(EngineMessage::ClientDisconnected).await;
    if !connector_opts.stay_open || exit_requested {
      info!("Breaking out of event loop in order to exit");
      frontend_sender.send(EngineMessage::EngineStopped).await;
      break;
    }
    info!("Server connection dropped, restarting");
  }
  info!("Exiting");
  if frontend_sender.has_frontend() {
    // Yield before the exit so that the frontend can finish sending log messages.
    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
  }
  process_ended_token.cancel();
  Ok(())
}
