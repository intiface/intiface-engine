use super::{
  ConnectorOptions, 
  ServerOptions, 
  IntifaceCLIErrorEnum,
  IntifaceError,
  server::{ButtplugServerFactory, create_server_factory},
  utils::generate_certificate,
};

use super::frontend::{self, FrontendPBufSender};
use std::fs;
use argh::FromArgs;
use buttplug::device::configuration_manager::{
      set_external_device_config, set_user_device_config, DeviceConfigurationManager,
};

const VERSION: &'static str = env!("CARGO_PKG_VERSION");

/// command line interface for intiface/buttplug.
///
/// Note: Commands are one word to keep compat with C#/JS executables currently.
#[derive(FromArgs)]
struct IntifaceCLIArguments {
    // Options that do something then exit
    /// print version and exit.
    #[argh(switch)]
    serverversion: bool,

    /// generate certificate file at the path specified, then exit.
    #[argh(option)]
    generatecert: Option<String>,

    // Options that set up the server networking
    /// if passed, websocket server listens on all interfaces. Otherwise, only
    /// listen on 127.0.0.1.
    #[argh(switch)]
    wsallinterfaces: bool,

    /// insecure port for websocket servers.
    #[argh(option)]
    wsinsecureport: Option<u16>,

    /// secure port for websocket servers.
    #[argh(option)]
    wssecureport: Option<u16>,

    /// certificate file for secure websocket server
    #[argh(option)]
    wscertfile: Option<String>,

    /// private key file for secure websocket server
    #[argh(option)]
    wsprivfile: Option<String>,

    /// pipe name for ipc server
    #[argh(option)]
    ipcpipe: Option<String>,

    // Options that set up communications with intiface GUI
    /// if passed, output protobufs for parent process via stdio, instead of strings.
    #[argh(switch)]
    frontendpipe: bool,

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

    /// unused but needed for compat
    #[argh(option)]
    log: String,
}

pub fn check_options_and_pipe() -> FrontendPBufSender {
    let args: IntifaceCLIArguments = argh::from_env();
    if args.frontendpipe {
        frontend::run_frontend_task()
    } else {
        FrontendPBufSender::default()
    }
}

pub fn parse_options(sender: FrontendPBufSender) -> Result<Option<(ConnectorOptions, ButtplugServerFactory)>, IntifaceCLIErrorEnum> {
    let args: IntifaceCLIArguments = argh::from_env();

    // Options that will do a thing then exit:
    //
    // - serverversion
    // - generatecert
    if args.serverversion {
        debug!("Server version command sent, printing and exiting.");
        println!("Intiface CLI (Rust Edition) Version {}", VERSION);
        return Ok(None);
    }
    if let Some(path) = args.generatecert {
        return generate_certificate(path)
        .and_then(|_| Ok(None));
    }

    // Options that set up the server networking

    let mut connector_info = None;

    if args.wsallinterfaces {
        info!("Intiface CLI Options: Websocket Use All Interfaces option passed.");
        if connector_info.is_none() {
            connector_info = Some(ConnectorOptions::default());
        }
        if let Some(info) = &mut connector_info {
            info.ws_listen_on_all_interfaces = true;
        }
    }

    if let Some(wsinsecureport) = &args.wsinsecureport {
        info!(
            "Intiface CLI Options: Websocket Insecure Port {}",
            wsinsecureport
        );
        if connector_info.is_none() {
            connector_info = Some(ConnectorOptions::default());
        }
        if let Some(info) = &mut connector_info {
            info.ws_insecure_port = Some(*wsinsecureport);
        }
    }

    if let Some(wscertfile) = &args.wscertfile {
        info!(
            "Intiface CLI Options: Websocket Certificate File {}",
            wscertfile
        );
        if connector_info.is_none() {
            connector_info = Some(ConnectorOptions::default());
        }
        if let Some(info) = &mut connector_info {
            info.ws_cert_file = Some((*wscertfile).clone());
        }
    }

    if let Some(wsprivfile) = &args.wsprivfile {
        info!(
            "Intiface CLI Options: Websocket Private Key File {}",
            wsprivfile
        );
        if connector_info.is_none() {
            connector_info = Some(ConnectorOptions::default());
        }
        if let Some(info) = &mut connector_info {
            info.ws_priv_file = Some((*wsprivfile).clone());
        }
    }

    if let Some(wssecureport) = &args.wssecureport {
        info!(
            "Intiface CLI Options: Websocket Insecure Port {}",
            wssecureport
        );
        // After this point, we should definitely already know we're connecting
        // because we need both cert options. If they aren't there, exit.
        if let Some(info) = &mut connector_info {
            if info.ws_cert_file.is_none() || info.ws_priv_file.is_none() {
                return Err(IntifaceCLIErrorEnum::IntifaceError(IntifaceError::new(
                    "Must have certificate and private key file to run secure server",
                )));
            }
            info.ws_secure_port = Some(*wssecureport);
        } else {
            return Err(IntifaceCLIErrorEnum::IntifaceError(IntifaceError::new(
                "Must have certificate and private key file to run secure server",
            )));
        }
    }

    if let Some(ipcpipe) = &args.ipcpipe {
        info!("Intiface CLI Options: IPC Pipe Name {}", ipcpipe);
    }

    // If we don't have a device configuration by this point, panic.

    if connector_info.is_none() {
        return Err(IntifaceError::new(
            "Must have a connection argument (wsinscureport, wssecureport, ipcport) to run!",
        )
        .into());
    }

    // Options that set up communications with intiface GUI
    let mut server_info = ServerOptions::default();

    server_info.server_name = args.servername;
    server_info.max_ping_time = args.pingtime;

    if args.frontendpipe {
        info!("Intiface CLI Options: Using frontend pipe");
        server_info.use_frontend_pipe = true;
    }
    
    if args.stayopen {
      info!("Intiface CLI Options: Leave server open after disconnect.");
      server_info.stay_open = true;
    }

    // Options that set up Buttplug server parameters

    if let Some(deviceconfig) = &args.deviceconfig {
        info!(
            "Intiface CLI Options: External Device Config {}",
            deviceconfig
        );
        let cfg = fs::read_to_string(deviceconfig).unwrap();
        set_external_device_config(Some(cfg));
        // Make an unused DeviceConfigurationManager here, as it'll panic if it's invalid.
        let _manager = DeviceConfigurationManager::new();
    }

    if let Some(userdeviceconfig) = &args.userdeviceconfig {
        info!(
            "Intiface CLI Options: User Device Config {}",
            userdeviceconfig
        );
        let cfg = fs::read_to_string(userdeviceconfig).unwrap();
        set_user_device_config(Some(cfg));
        let _manager = DeviceConfigurationManager::new();
    }

    let server_factory = create_server_factory(server_info, sender);

    Ok(Some((connector_info.unwrap(), server_factory)))
}