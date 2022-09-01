use super::{ConnectorOptions, IntifaceCLIErrorEnum, IntifaceError};
use argh::FromArgs;
#[cfg(target_os = "windows")]
use buttplug::server::device::hardware::communication::xinput::XInputDeviceCommunicationManagerBuilder;
use buttplug::server::{
  device::hardware::communication::{
    btleplug::BtlePlugCommunicationManagerBuilder,
    lovense_connect_service::LovenseConnectServiceCommunicationManagerBuilder,
    lovense_dongle::{
      LovenseHIDDongleCommunicationManagerBuilder, LovenseSerialDongleCommunicationManagerBuilder,
    },
    serialport::SerialPortCommunicationManagerBuilder,
    websocket_server::websocket_server_comm_manager::WebsocketServerDeviceCommunicationManagerBuilder,
  },
  ButtplugServerBuilder,
};

use std::fs;
use tracing::Level;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// command line interface for intiface/buttplug.
///
/// Note: Commands are one word to keep compat with C#/JS executables currently.
#[derive(FromArgs)]
pub struct IntifaceCLIArguments {
  // Options that do something then exit
  /// print version and exit.
  #[argh(switch)]
  version: bool,

  /// print version and exit.
  #[argh(switch)]
  pub serverversion: bool,

  /// turn on crash reporting to sentry
  #[argh(switch)]
  crash_reporting: bool,
  
  // Options that set up the server networking
  /// if passed, websocket server listens on all interfaces. Otherwise, only
  /// listen on 127.0.0.1.
  #[argh(switch)]
  wsallinterfaces: bool,

  /// insecure port for websocket servers.
  #[argh(option)]
  wsinsecureport: Option<u16>,

  /// pipe name for ipc server
  #[argh(option)]
  ipcpipe: Option<String>,

  // Options that set up communications with intiface GUI
  /// if passed, output protobufs for parent process via stdio, instead of strings.
  #[argh(option)]
  frontendpipe: Option<String>,

  // Options that set up Buttplug server parameters
  /// name of server to pass to connecting clients.
  #[argh(option)]
  #[argh(default = "\"Buttplug Server\".to_owned()")]
  servername: String,

  /// path to the device configuration file
  #[argh(option)]
  deviceconfig: Option<String>,

  /// path to user device configuration file
  #[argh(option)]
  userdeviceconfig: Option<String>,

  /// ping timeout maximum for server (in milliseconds)
  #[argh(option)]
  #[argh(default = "0")]
  pingtime: u32,

  /// if passed, server will stay running after client disconnection
  #[argh(switch)]
  stayopen: bool,

  /// set log level for output
  #[allow(dead_code)]
  #[argh(option)]
  log: Option<Level>,

  /// allow raw messages (dangerous, only use for development)
  #[argh(switch)]
  allowraw: bool,

  /// turn off bluetooth le device support
  #[argh(switch)]
  without_bluetooth_le: bool,

  /// turn off serial device support
  #[argh(switch)]
  without_serial: bool,

  /// turn off hid device support
  #[allow(dead_code)]
  #[argh(switch)]
  without_hid: bool,

  /// turn off lovense dongle serial device support
  #[argh(switch)]
  without_lovense_dongle_serial: bool,

  /// turn off lovense dongle hid device support
  #[argh(switch)]
  without_lovense_dongle_hid: bool,

  /// turn off xinput gamepad device support (windows only)
  #[argh(switch)]
  without_xinput: bool,

  /// turn on lovense connect app device support (off by default)
  #[argh(switch)]
  with_lovense_connect: bool,

  /// turn on websocket server device comm manager
  #[argh(switch)]
  with_device_websocket_server: bool,

  /// port for device websocket server comm manager (defaults to 54817)
  #[argh(option)]
  device_websocket_server_port: Option<u16>,

  #[cfg(debug_assertions)]
  /// crash the main thread (that holds the runtime)
  #[argh(switch)]
  crash_main_thread: bool,

  #[allow(dead_code)]
  #[cfg(debug_assertions)]
  /// crash the task thread (for testing logging/reporting)
  #[argh(switch)]
  crash_task_thread: bool,
}

pub fn should_turn_on_crash_reporting() -> bool {
  let args: IntifaceCLIArguments = argh::from_env();
  args.crash_reporting
}


pub fn setup_server_device_comm_managers(server_builder: &mut ButtplugServerBuilder) {
  let args: IntifaceCLIArguments = argh::from_env();
  if !args.without_bluetooth_le {
    info!("Including Bluetooth LE (btleplug) Device Comm Manager Support");
    server_builder.comm_manager(BtlePlugCommunicationManagerBuilder::default());
  }
  if !args.without_lovense_dongle_hid {
    info!("Including Lovense HID Dongle Support");
    server_builder.comm_manager(LovenseHIDDongleCommunicationManagerBuilder::default());
  }
  if !args.without_lovense_dongle_serial {
    info!("Including Lovense Serial Dongle Support");
    server_builder.comm_manager(LovenseSerialDongleCommunicationManagerBuilder::default());
  }
  if !args.without_serial {
    info!("Including Serial Port Support");
    server_builder.comm_manager(SerialPortCommunicationManagerBuilder::default());
  }
  #[cfg(target_os = "windows")]
  if !args.without_xinput {
    info!("Including XInput Gamepad Support");
    server_builder.comm_manager(XInputDeviceCommunicationManagerBuilder::default());
  }
  if args.with_lovense_connect {
    info!("Including Lovense Connect App Support");
    server_builder.comm_manager(LovenseConnectServiceCommunicationManagerBuilder::default());
  }
  if args.with_device_websocket_server {
    info!("Including Websocket Server Device Support");
    let mut builder = WebsocketServerDeviceCommunicationManagerBuilder::default().listen_on_all_interfaces(true);
    if let Some(port) = args.device_websocket_server_port {
      builder = builder.server_port(port);
    }
    server_builder.comm_manager(builder);
  }
}

#[cfg(debug_assertions)]
pub fn maybe_crash_main_thread() {
  let args: IntifaceCLIArguments = argh::from_env();
  if args.crash_main_thread {
    panic!("Crashing main thread by request");
  }
}

#[allow(dead_code)]
#[cfg(debug_assertions)]
pub fn maybe_crash_task_thread() {
  use std::time::Duration;
  let args: IntifaceCLIArguments = argh::from_env();
  if args.crash_task_thread {
    tokio::spawn(async {
      tokio::time::sleep(Duration::from_millis(100)).await;
      panic!("Crashing a task thread by request");
    });
  }
}

pub fn check_log_level() -> Option<Level> {
  let args: IntifaceCLIArguments = argh::from_env();
  args.log
}

pub fn frontend_pipe() -> Option<String> {
  let args: IntifaceCLIArguments = argh::from_env();
  args.frontendpipe
}

pub fn parse_options() -> Result<Option<ConnectorOptions>, IntifaceCLIErrorEnum> {
  let args: IntifaceCLIArguments = argh::from_env();
  
  if args.version {
    debug!("Server version command sent, printing and exiting.");
    println!(
      "Intiface CLI (Rust Edition) Version {}, Commit {}, Built {}",
      VERSION,
      env!("VERGEN_GIT_SHA_SHORT"),
      env!("VERGEN_BUILD_TIMESTAMP")
    );
    return Ok(None);
  }

  // Options that set up the server networking

  let mut connector_info = ConnectorOptions::default();
  let mut connector_info_set = false;

  if args.wsallinterfaces {
    info!("Intiface CLI Options: Websocket Use All Interfaces option passed.");
    connector_info.ws_listen_on_all_interfaces = true;
    connector_info_set = true;
  }

  if let Some(wsinsecureport) = &args.wsinsecureport {
    info!(
      "Intiface CLI Options: Websocket Insecure Port {}",
      wsinsecureport
    );
    connector_info.ws_insecure_port = Some(*wsinsecureport);
    connector_info_set = true;
  }

  if let Some(ipcpipe) = &args.ipcpipe {
    // TODO We should actually implement pipes :(
    info!("Intiface CLI Options: IPC Pipe Name {}", ipcpipe);
  }

  // If we don't have a device configuration by this point, panic.

  if !connector_info_set {
    return Err(
      IntifaceError::new(
        "Must have a connection argument (wsinsecureport, wssecureport, ipcport) to run!",
      )
      .into(),
    );
  }

  connector_info
    .server_builder
    .name(&args.servername)
    .max_ping_time(args.pingtime);

  if args.allowraw {
    connector_info.server_builder.allow_raw_messages();
  }
  if args.frontendpipe.is_some() {
    info!("Intiface CLI Options: Using frontend pipe");
    connector_info.use_frontend_pipe = true;
  }

  if args.stayopen {
    info!("Intiface CLI Options: Leave server open after disconnect.");
    connector_info.stay_open = true;
  }

  // Options that set up Buttplug server parameters

  if let Some(deviceconfig) = &args.deviceconfig {
    info!(
      "Intiface CLI Options: External Device Config {}",
      deviceconfig
    );
    match fs::read_to_string(deviceconfig) {
      Ok(cfg) => connector_info
        .server_builder
        .device_configuration_json(Some(cfg)),
      Err(err) => panic!("Error opening external device configuration: {:?}", err),
    };
  }

  if let Some(userdeviceconfig) = &args.userdeviceconfig {
    info!(
      "Intiface CLI Options: User Device Config {}",
      userdeviceconfig
    );
    match fs::read_to_string(userdeviceconfig) {
      Ok(cfg) => connector_info
        .server_builder
        .user_device_configuration_json(Some(cfg)),
      Err(err) => panic!("Error opening user device configuration: {:?}", err),
    };
  }

  Ok(Some(connector_info))
}
