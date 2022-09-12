use crate::{
  error::IntifaceEngineError,
  frontend::{setup_frontend, process_messages::EngineMessage, frontend_server_event_loop},
  options::EngineOptions, device_communication_managers::setup_server_device_comm_managers,
};
use buttplug::{
  core::{
    connector::{
      ButtplugPipeClientTransportBuilder, ButtplugRemoteServerConnector,
      ButtplugWebsocketServerTransportBuilder,
    },
    message::serializer::ButtplugServerJSONSerializer,
  },
  server::{ButtplugRemoteServer, ButtplugServerConnectorError, ButtplugServerBuilder},
};
use tokio::select;
use tokio_util::sync::CancellationToken;

#[cfg(debug_assertions)]
pub fn maybe_crash_main_thread(options: &EngineOptions) {
  if options.crash_main_thread() {
    panic!("Crashing main thread by request");
  }
}

#[allow(dead_code)]
#[cfg(debug_assertions)]
pub fn maybe_crash_task_thread(options: &EngineOptions) {
  use std::time::Duration;
  if options.crash_task_thread() {
    tokio::spawn(async {
      tokio::time::sleep(Duration::from_millis(100)).await;
      panic!("Crashing a task thread by request");
    });
  }
}

async fn setup_buttplug_server(
  options: &EngineOptions,
) -> Result<ButtplugRemoteServer, IntifaceEngineError> {
  //options::setup_server_device_comm_managers(&mut connector_opts.server_builder);

  let mut server_builder = ButtplugServerBuilder::default();
  server_builder
    .name(options.server_name())
    .max_ping_time(options.max_ping_time());

  if options.allow_raw_messages() {
    server_builder.allow_raw_messages();
  }

  if let Some(device_config_json) = options.device_config_json() {
    server_builder
      .device_configuration_json(Some(device_config_json.clone()));
  }

  if let Some(user_device_config_json) = &options.user_device_config_json() {
    server_builder
      .user_device_configuration_json(Some(user_device_config_json.clone()));
  }

  setup_server_device_comm_managers(options, &mut server_builder);

  let core_server = match server_builder.finish() {
    Ok(server) => server,
    Err(e) => {
      error!("Error starting server: {:?}", e);
      return Err(IntifaceEngineError::ButtplugServerError(e));
    }
  };
  Ok(ButtplugRemoteServer::new(core_server))
}

async fn run_server(
  server: &ButtplugRemoteServer,
  options: &EngineOptions,
) -> Result<(), ButtplugServerConnectorError> {
  if let Some(port) = options.websocket_port() {
    server
      .start(ButtplugRemoteServerConnector::<
        _,
        ButtplugServerJSONSerializer,
      >::new(
        ButtplugWebsocketServerTransportBuilder::default()
          .port(port)
          .listen_on_all_interfaces(options.websocket_use_all_interfaces())
          .finish(),
      ))
      .await
  } else if let Some(pipe_name) = options.ipc_pipe_name() {
    server
      .start(ButtplugRemoteServerConnector::<
        _,
        ButtplugServerJSONSerializer,
      >::new(
        ButtplugPipeClientTransportBuilder::new(pipe_name).finish(),
      ))
      .await
  } else {
    panic!("Neither websocket port nor ipc pipe name are set, cannot create transport.");
  }
}

#[derive(Default)]
pub struct IntifaceEngine {
  stop_token: CancellationToken,
}

impl IntifaceEngine {
  pub async fn run(&self, options: &EngineOptions) -> Result<(), IntifaceEngineError> {
    // At this point we will have received and validated options.

    // Set up crash logging for the duration of the server session.
    const API_KEY: &str = include_str!(concat!(env!("OUT_DIR"), "/sentry_api_key.txt"));
    let sentry_guard = if options.crash_reporting() && !API_KEY.is_empty() {
      Some(sentry::init((
        API_KEY,
        sentry::ClientOptions {
          release: sentry::release_name!(),
          ..Default::default()
        },
      )))
    } else {
      None
    };

    // Create the cancellation tokens for
    let frontend_cancellation_token = CancellationToken::new();
    let frontend_cancellation_child_token = frontend_cancellation_token.child_token();

    // Intiface GUI communicates with its child process via json through stdio.
    // Checking for this is the first thing we should do, as any output after this either needs to be
    // printed strings or json messages.

    let frontend = setup_frontend(options, &self.stop_token).await;

    if sentry_guard.is_some() {
      info!("Using sentry for crash logging.");
    } else {
      info!("Crash logging disabled.");
    }

    // Hang out until those listeners get sick of listening.
    info!("Intiface CLI Setup finished, running server tasks until all joined.");
    let server = setup_buttplug_server(options).await?;

    let event_receiver = server.event_stream();
    let frontend_clone = frontend.clone();
    let stop_child_token = self.stop_token.child_token();
    tokio::spawn(async move {
      frontend_server_event_loop(event_receiver, frontend_clone, stop_child_token).await;
    });
    loop {
      let session_connection_token = CancellationToken::new();
      info!("Starting server");

      // Let everything spin up, then try crashing.

      #[cfg(debug_assertions)]
      maybe_crash_main_thread(&options);

      let mut exit_requested = false;
      select! {
        _ = self.stop_token.cancelled() => {
          info!("Owner requested process exit, exiting.");
          exit_requested = true;
        }
        _ = frontend_cancellation_child_token.cancelled() => {
          info!("Owner requested process exit, exiting.");
          exit_requested = true;
        }
        result = run_server(&server, &options) => {
          match result {
            Ok(_) => info!("Connection dropped, restarting stay open loop."),
            Err(e) => {
              error!("{}", format!("Process Error: {:?}", e));
              frontend
                .send(EngineMessage::EngineError{ error: format!("Process Error: {:?}", e).to_owned()})
                .await;
              exit_requested = true;
            }
          }
        }
      };
      match server.disconnect().await {
        Ok(_) => info!("Client forcefully disconnected from server."),
        Err(_) => info!("Client already disconnected from server."),
      };
      session_connection_token.cancel();
      frontend.send(EngineMessage::ClientDisconnected {}).await;
      if exit_requested {
        info!("Breaking out of event loop in order to exit");
        frontend.send(EngineMessage::EngineStopped {}).await;
        break;
      }
      info!("Server connection dropped, restarting");
    }
    info!("Exiting");
    Ok(())
  }

  pub fn stop(&self) {
    self.stop_token.cancel();
  }
}
