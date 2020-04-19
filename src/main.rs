/// command line interface for intiface/buttplug.
///

#[macro_use]
extern crate log;

mod frontend;
mod options;
mod server;
mod utils;
mod websocket;

use buttplug::{
    core::errors::ButtplugError,
};
use frontend::intiface_gui::{
    server_process_message::{Msg, ProcessStarted, ProcessEnded, ProcessLog},
};
use env_logger;
use std::{
    error::Error,
    fmt,
};

#[derive(Default, Clone)]
pub struct ConnectorOptions {
    ws_listen_on_all_interfaces: bool,
    ws_insecure_port: Option<u16>,
    ws_secure_port: Option<u16>,
    ws_cert_file: Option<String>,
    ws_priv_file: Option<String>,
    ipc_pipe_name: Option<String>,
}

#[derive(Default, Clone)]
pub struct ServerOptions {
    server_name: String,
    max_ping_time: u32,
    stay_open: bool,
    use_frontend_pipe: bool,
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
    let mut env_log = None;
    if !frontend_sender.is_active() {
        env_log = Some(env_logger::builder().is_test(true).try_init());
    }

    frontend_sender.send(Msg::ProcessLog(ProcessLog {
        message: "Testing message".to_string()
    })).await;
    frontend_sender.send(Msg::ProcessStarted(ProcessStarted::default())).await;
    
    // Parse options, get back our connection information and a curried server
    // factory closure.
    let (connector_opts, server_factory) = match options::parse_options(frontend_sender.clone()) {
        Ok(opts) => {
            match opts {
                Some(o) => o,
                None => return Ok(())
            }
        },
        Err(e) => return Err(e)
    };

    // Spin up our listeners.
    let tasks = match websocket::create_websocket_listeners(connector_opts, server_factory) {
        Ok(t) => t,
        Err(e) => return Err(e)
    };

    // Hang out until those listeners get sick of listening.
    info!("Intiface CLI Setup finished, running server tasks until all joined.");
    for t in tasks {
        t.await;
    }
    info!("Exiting");
    frontend_sender.send(Msg::ProcessEnded(ProcessEnded::default())).await;
    Ok(())
}
