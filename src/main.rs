#[macro_use]
extern crate log;

use argh::FromArgs;
use async_std::{
    net::{TcpListener, TcpStream},
    prelude::FutureExt,
    task,
};
use buttplug::{
    core::errors::ButtplugError,
    device::configuration_manager::{
        set_external_device_config, set_user_device_config, DeviceConfigurationManager,
    },
    server::wrapper::{ButtplugJSONServerWrapper, ButtplugServerWrapper},
};
use env_logger;
use futures::prelude::*;
use rcgen::generate_simple_self_signed;
use std::{
    error::Error,
    fmt,
    fs::{self, File},
    io::{self, Write, BufReader},
    path::{PathBuf, Path},
    sync::Arc,
    process,
};
use async_tls::TlsAcceptor;
use rustls::internal::pemfile::{certs, rsa_private_keys, pkcs8_private_keys};
use rustls::{Certificate, NoClientAuth, PrivateKey, ServerConfig};

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
}

enum StreamOutput {
    WebsocketMessage(async_tungstenite::tungstenite::Message),
    ButtplugMessage(String),
}

async fn accept_connection<S>(stream: S, name: &str, max_ping_time: u32) 
where S: AsyncRead + AsyncWrite + Unpin {
/*     let addr = stream
        .peer_addr()
        .expect("connected streams should have a peer address");
    info!("Peer address: {}", addr);
 */
    let ws_stream = async_tungstenite::accept_async(stream)
        .await
        .expect("Error during the websocket handshake occurred");

    info!("New WebSocket connection.");

    let (mut server, mut receiver) = ButtplugJSONServerWrapper::new(name, max_ping_time as u128);
    server.server_ref().add_comm_manager::<buttplug::server::comm_managers::btleplug::BtlePlugCommunicationManager>();
    #[cfg(target_os="windows")]
    server.server_ref().add_comm_manager::<buttplug::server::comm_managers::xinput::XInputDeviceCommunicationManager>();

    let (mut write, mut read) = ws_stream.split();

    loop {
        let read_waitier = async {
            match read.next().await {
                Some(res) => match res {
                    Ok(msg) => Some(StreamOutput::WebsocketMessage(msg)),
                    Err(_) => None,
                },
                None => None,
            }
        };
        let buttplug_waiter = async {
            match receiver.next().await {
                Some(msg) => Some(StreamOutput::ButtplugMessage(msg)),
                None => None,
            }
        };
        let racer = buttplug_waiter.race(read_waitier);
        match racer.await {
            Some(output) => {
                match output {
                    StreamOutput::WebsocketMessage(msg) => match msg {
                        async_tungstenite::tungstenite::Message::Text(text_msg) => {
                            write
                                .send(async_tungstenite::tungstenite::Message::Text(
                                    server.parse_message(text_msg).await,
                                ))
                                .await
                                .unwrap();
                        }
                        async_tungstenite::tungstenite::Message::Close(_) => {
                            break;
                        }
                        _ => {
                            panic!("Don't know how to handle this message type!");
                        }
                    },
                    StreamOutput::ButtplugMessage(msg) => {
                        // If we get a message out of the server, just fling it back over.
                        write
                            .send(async_tungstenite::tungstenite::Message::Text(msg))
                            .await
                            .unwrap();
                    }
                }
            }
            None => {
                break;
            }
        }
    }
}

#[derive(Default)]
struct ConnectorInfo {
    ws_listen_on_all_interfaces: bool,
    ws_insecure_port: Option<u16>,
    ws_secure_port: Option<u16>,
    ws_cert_file: Option<String>,
    ws_priv_file: Option<String>,
    ipc_pipe_name: Option<String>,
}

#[derive(Debug)]
struct IntifaceError {
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
enum IntifaceCLIErrorEnum {
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

#[async_std::main]
async fn main() -> Result<(), IntifaceCLIErrorEnum> {
    let _ = env_logger::builder().is_test(true).try_init();
    let args: IntifaceCLIArguments = argh::from_env();

    // Options that will do a thing then exit:
    //
    // - serverversion
    // - generatecert
    if args.serverversion {
        debug!("Server version command sent, printing and exiting.");
        println!("Intiface CLI (Rust Edition) Version {}", VERSION);
        return Ok(());
    }
    if let Some(path) = args.generatecert {
        debug!("Generate cert command used, creating cert and exiting.");
        let subject_alt_names = vec!["localhost".to_string()];
        let cert = generate_simple_self_signed(subject_alt_names).unwrap();
        let mut base_path = PathBuf::new();
        base_path.push(&path);
        if !base_path.is_dir() {
            println!(
                "Certificate write path {} does not exist or is not a directory.",
                path
            );
            process::exit(1);
        }
        base_path.set_file_name("cert.pem");
        let mut pem_out = File::create(&base_path).map_err(|x| IntifaceCLIErrorEnum::from(x))?;
        base_path.set_file_name("key.pem");
        let mut key_out = File::create(&base_path).map_err(|x| IntifaceCLIErrorEnum::from(x))?;
        write!(pem_out, "{}", cert.serialize_pem().unwrap())
            .map_err(|x| IntifaceCLIErrorEnum::from(x))?;
        write!(key_out, "{}", cert.serialize_private_key_pem())
            .map_err(|x| IntifaceCLIErrorEnum::from(x))?;
        return Ok(());
    }

    // Options that set up the server networking

    let mut connector_info = None;

    if args.wsallinterfaces {
        info!("Intiface CLI Options: Websocket Use All Interfaces option passed.");
        if connector_info.is_none() {
            connector_info = Some(ConnectorInfo::default());
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
            connector_info = Some(ConnectorInfo::default());
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
            connector_info = Some(ConnectorInfo::default());
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
            connector_info = Some(ConnectorInfo::default());
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

    if args.frontendpipe {
        info!("Intiface CLI Options: Using frontend pipe");
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
        let manager = DeviceConfigurationManager::new();
    }

    if let Some(userdeviceconfig) = &args.userdeviceconfig {
        info!(
            "Intiface CLI Options: User Device Config {}",
            userdeviceconfig
        );
        let cfg = fs::read_to_string(userdeviceconfig).unwrap();
        set_user_device_config(Some(cfg));
        let manager = DeviceConfigurationManager::new();
    }

    let info = connector_info.take().unwrap();

    let mut tasks = vec![];

    if let Some(ws_insecure_port) = info.ws_insecure_port {
        let server_name = args.servername.clone();
        let ping = args.pingtime.clone();
        tasks.push(task::spawn(async move {
            let addr = format!("127.0.0.1:{}", ws_insecure_port);
            debug!("Websocket Insecure: Trying to listen on {}", addr);
            // Create the event loop and TCP listener we'll accept connections on.
            let try_socket = TcpListener::bind(&addr).await;
            debug!("Websocket Insecure: Socket bound.");
            let listener = try_socket.expect("Failed to bind");
            debug!("Websocket Insecure: Listening on: {}", addr);

            while let Ok((stream, _)) = listener.accept().await {
                info!("Websocket Insecure: Got connection");
                let server_name_clone = server_name.clone();
                task::spawn(async move {
                    accept_connection(stream, &server_name_clone, ping).await;
                });
            }
        }));
    }

    if let Some(ws_secure_port) = info.ws_secure_port {
        let server_name = args.servername.clone();
        let ping = args.pingtime.clone();

        let certs = certs(&mut BufReader::new(File::open(info.ws_cert_file.unwrap())?))
                        .map_err(|_| IntifaceCLIErrorEnum::from(io::Error::new(io::ErrorKind::InvalidInput, "invalid cert file")))?;

        let mut keys = pkcs8_private_keys(&mut BufReader::new(File::open(info.ws_priv_file.unwrap())?))
                        .map_err(|_| IntifaceCLIErrorEnum::from(io::Error::new(io::ErrorKind::InvalidInput, "invalid key file")))?;
        println!("{:?}", keys);
        // we don't use client authentication
        let mut config = ServerConfig::new(NoClientAuth::new());
        config
            // set this server to use one cert together with the loaded private key
            .set_single_cert(certs, keys.remove(0))
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
        let acceptor = TlsAcceptor::from(Arc::new(config));
        tasks.push(task::spawn(async move {
            let addr = format!("127.0.0.1:{}", ws_secure_port);
            debug!("Websocket Insecure: Trying to listen on {}", addr);
            // Create the event loop and TCP listener we'll accept connections on.
            let try_socket = TcpListener::bind(&addr).await;
            debug!("Websocket Insecure: Socket bound.");
            let listener = try_socket.expect("Failed to bind");
            debug!("Websocket Insecure: Listening on: {}", addr);

            while let Ok((stream, _)) = listener.accept().await {
                let handshake = acceptor.accept(stream);
                // The handshake is a future we can await to get an encrypted
                // stream back.
                let tls_stream = handshake.await.unwrap();
                info!("Websocket Insecure: Got connection");
                let server_name_clone = server_name.clone();
                task::spawn(async move {
                    accept_connection(tls_stream, &server_name_clone, ping).await;
                });
            }
        }));
    }

    info!("Intiface CLI Setup finished, running server tasks until all joined.");
    for t in tasks {
        t.await;
    }
    info!("Exiting");
    Ok(())
}
