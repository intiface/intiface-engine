use argh::FromArgs;
use getset::{CopyGetters, Getters};
use intiface_engine::{
  setup_console_logging, EngineOptions, EngineOptionsBuilder, IntifaceEngine, IntifaceEngineError,
  IntifaceError,
};
use std::fs;
use tokio::{select, signal::ctrl_c};
use tracing::debug;
use tracing::info;
use tracing::Level;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// command line interface for intiface/buttplug.
///
/// Note: Commands are one word to keep compat with C#/JS executables currently.
#[derive(FromArgs, Getters, CopyGetters)]
pub struct IntifaceCLIArguments {
  // Options that do something then exit
  /// print version and exit.
  #[argh(switch)]
  #[getset(get_copy = "pub")]
  version: bool,

  /// print version and exit.
  #[argh(switch)]
  #[getset(get_copy = "pub")]
  server_version: bool,

  /// turn on crash reporting to sentry
  #[argh(switch)]
  #[getset(get_copy = "pub")]
  crash_reporting: bool,

  // Options that set up the server networking
  /// if passed, websocket server listens on all interfaces. Otherwise, only
  /// listen on 127.0.0.1.
  #[argh(switch)]
  #[getset(get_copy = "pub")]
  websocket_use_all_interfaces: bool,

  /// insecure port for websocket servers.
  #[argh(option)]
  #[getset(get_copy = "pub")]
  websocket_port: Option<u16>,

  // Options that set up communications with intiface GUI
  /// if passed, output json for parent process via websockets
  #[argh(option)]
  #[getset(get_copy = "pub")]
  frontend_websocket_port: Option<u16>,

  // Options that set up Buttplug server parameters
  /// name of server to pass to connecting clients.
  #[argh(option)]
  #[argh(default = "\"Buttplug Server\".to_owned()")]
  #[getset(get = "pub")]
  server_name: String,

  /// path to the device configuration file
  #[argh(option)]
  #[getset(get = "pub")]
  device_config_file: Option<String>,

  /// path to user device configuration file
  #[argh(option)]
  #[getset(get = "pub")]
  user_device_config_file: Option<String>,

  /// ping timeout maximum for server (in milliseconds)
  #[argh(option)]
  #[argh(default = "0")]
  #[getset(get_copy = "pub")]
  max_ping_time: u32,

  /// set log level for output
  #[allow(dead_code)]
  #[argh(option)]
  #[getset(get_copy = "pub")]
  log: Option<Level>,

  /// allow raw messages (dangerous, only use for development)
  #[argh(switch)]
  #[getset(get_copy = "pub")]
  allow_raw: bool,

  /// turn off bluetooth le device support
  #[argh(switch)]
  #[getset(get_copy = "pub")]
  use_bluetooth_le: bool,

  /// turn off serial device support
  #[argh(switch)]
  #[getset(get_copy = "pub")]
  use_serial: bool,

  /// turn off hid device support
  #[allow(dead_code)]
  #[argh(switch)]
  #[getset(get_copy = "pub")]
  use_hid: bool,

  /// turn off lovense dongle serial device support
  #[argh(switch)]
  #[getset(get_copy = "pub")]
  use_lovense_dongle_serial: bool,

  /// turn off lovense dongle hid device support
  #[argh(switch)]
  #[getset(get_copy = "pub")]
  use_lovense_dongle_hid: bool,

  /// turn off xinput gamepad device support (windows only)
  #[argh(switch)]
  #[getset(get_copy = "pub")]
  use_xinput: bool,

  /// turn on lovense connect app device support (off by default)
  #[argh(switch)]
  #[getset(get_copy = "pub")]
  use_lovense_connect: bool,

  /// turn on websocket server device comm manager
  #[argh(switch)]
  #[getset(get_copy = "pub")]
  use_device_websocket_server: bool,

  /// port for device websocket server comm manager (defaults to 54817)
  #[argh(option)]
  #[getset(get_copy = "pub")]
  device_websocket_server_port: Option<u16>,

  #[cfg(debug_assertions)]
  /// crash the main thread (that holds the runtime)
  #[argh(switch)]
  #[getset(get_copy = "pub")]
  crash_main_thread: bool,

  #[allow(dead_code)]
  #[cfg(debug_assertions)]
  /// crash the task thread (for testing logging/reporting)
  #[argh(switch)]
  #[getset(get_copy = "pub")]
  crash_task_thread: bool,
}

impl TryFrom<IntifaceCLIArguments> for EngineOptions {
  type Error = IntifaceError;
  fn try_from(args: IntifaceCLIArguments) -> Result<Self, IntifaceError> {
    let mut builder = EngineOptionsBuilder::default();

    if let Some(deviceconfig) = args.device_config_file() {
      info!(
        "Intiface CLI Options: External Device Config {}",
        deviceconfig
      );
      match fs::read_to_string(deviceconfig) {
        Ok(cfg) => builder.device_config_json(&cfg),
        Err(err) => {
          return Err(IntifaceError::new(&format!(
            "Error opening external device configuration: {:?}",
            err
          )))
        }
      };
    }

    if let Some(userdeviceconfig) = args.user_device_config_file() {
      info!(
        "Intiface CLI Options: User Device Config {}",
        userdeviceconfig
      );
      match fs::read_to_string(userdeviceconfig) {
        Ok(cfg) => builder.user_device_config_json(&cfg),
        Err(err) => {
          return Err(IntifaceError::new(&format!(
            "Error opening user device configuration: {:?}",
            err
          )))
        }
      };
    }

    builder
      .allow_raw_messages(args.allow_raw())
      .crash_reporting(args.crash_reporting())
      .websocket_use_all_interfaces(args.websocket_use_all_interfaces())
      .use_bluetooth_le(args.use_bluetooth_le())
      .use_serial_port(args.use_serial())
      .use_hid(args.use_hid())
      .use_lovense_dongle_serial(args.use_lovense_dongle_serial())
      .use_lovense_dongle_hid(args.use_lovense_dongle_hid())
      .use_xinput(args.use_xinput())
      .use_lovense_connect(args.use_lovense_connect())
      .use_device_websocket_server(args.use_device_websocket_server())
      .max_ping_time(args.max_ping_time())
      .server_name(args.server_name());

    #[cfg(debug_assertions)]
    {
      builder
        .crash_main_thread(args.crash_main_thread())
        .crash_task_thread(args.crash_task_thread());
    }

    if let Some(value) = args.log() {
      builder.log_level(value);
    }
    if let Some(value) = args.websocket_port() {
      builder.websocket_port(value);
    }
    if let Some(value) = args.frontend_websocket_port() {
      builder.frontend_websocket_port(value);
    }
    if let Some(value) = args.device_websocket_server_port() {
      builder.device_websocket_server_port(value);
    }
    Ok(builder.finish())
  }
}

#[tokio::main]
async fn main() -> Result<(), IntifaceEngineError> {
  let args: IntifaceCLIArguments = argh::from_env();
  if args.server_version() {
    println!("{}", VERSION);
    return Ok(());
  }

  if args.version() {
    debug!("Server version command sent, printing and exiting.");
    println!(
      "Intiface CLI (Rust Edition) Version {}, Commit {}, Built {}",
      VERSION,
      env!("VERGEN_GIT_SHA_SHORT"),
      env!("VERGEN_BUILD_TIMESTAMP")
    );
    return Ok(());
  }

  if args.frontend_websocket_port().is_none() {
    setup_console_logging(args.log());
  }

  let options = EngineOptions::try_from(args).map_err(IntifaceEngineError::from)?;
  let engine = IntifaceEngine::default();
  select! {
    _ = engine.run(&options, None) => {

    }
    _ = ctrl_c() => {
      info!("Control-c hit, exiting.");
      engine.stop();
    }
  }

  Ok(())
}
