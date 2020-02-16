use buttplug::{
    server::{ButtplugServer},
    core::messages::{ButtplugMessage, ButtplugMessageUnion},
};
use argh::FromArgs;
use ws::{self, Message, CloseCode, Handler, Handshake};
use async_std::{
    prelude::StreamExt,
    sync::{channel},
    task,
};

/// command line interface for intiface/buttplug.
///
/// Note: Commands are one word to keep compat with C#/JS executables currently.
#[derive(FromArgs)]
struct IntifaceCLIArguments {
    /// name of server to pass to connecting clients.
    #[argh(option)]
    #[argh(default = "\"Buttplug Server\".to_owned()")]
    servername: String,

    /// print version and exit.
    #[argh(switch)]
    serverversion: bool,

    /// generate certificate file at the path specified, then exit.
    #[argh(switch)]
    generatecert: bool,

    /// path to the device configuration file
    #[argh(option)]
    deviceconfig: Option<String>,

    /// path to user device configuration file
    #[argh(option)]
    userdeviceconfig: Option<String>,

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

    /// run ipc server
    #[argh(switch)]
    ipcserver: bool,

    /// pipe name for ipc server
    #[argh(option)]
    ipcpipe: Option<String>,

    /// if passed, output protobufs for parent process, instead of strings.
    #[argh(switch)]
    frontendpipe: bool,

    /// ping timeout maximum for server (in milliseconds)
    #[argh(option)]
    #[argh(default = "0")]
    pingtime: u32,

    /// if passed, server will stay running after client disconnection
    #[argh(switch)]
    stayopen: bool,

    /// if specified, print logs at level and higher to console
    #[argh(option)]
    log: Option<String>,
}

struct Server {
    out: ws::Sender,
    server: ButtplugServer
}

impl Server {
    pub fn new(out: ws::Sender, name: &str, max_ping_time: u32) -> Self{
        let (sender, mut receiver) = channel(256);
        let mut server = ButtplugServer::new(name, max_ping_time as u128, sender);
        server.add_comm_manager::<buttplug::server::comm_managers::btleplug::BtlePlugCommunicationManager>();
        let out_clone = out.clone();
        task::spawn(async move {
            loop {
                match receiver.next().await {
                    Some(msg) => {
                        let out_msg = msg.as_protocol_json();
                        println!("{}", &out_msg);
                        out_clone.send(out_msg).unwrap();
                    },
                    None => {
                        break;
                    }
                }
            }
        });
        Self {
            out,
            server
        }
    }
}

impl Handler for Server {

    fn on_open(&mut self, shake: Handshake) -> ws::Result<()> {
        println!("Connection opened");
        Ok(())
    }

    fn on_message(&mut self, msg: Message) -> ws::Result<()> {
        let msg_str = &msg.into_text().unwrap();
        println!("Got message!");
        println!("{}", &msg_str);
        let union: ButtplugMessageUnion = ButtplugMessageUnion::try_deserialize(&msg_str).unwrap();
        task::block_on(async {
            let ret = self.server.parse_message(&union).await.unwrap();
            let out_msg = ret.as_protocol_json(); //ret.try_serialize();
            println!("{}", &out_msg);
            self.out.send(out_msg).unwrap();
        });

        Ok(())
    }

    fn on_close(&mut self, code: CloseCode, reason: &str) {
        // The WebSocket protocol allows for a utf8 reason for the closing state after the
        // close code. WS-RS will attempt to interpret this data as a utf8 description of the
        // reason for closing the connection. I many cases, `reason` will be an empty string.
        // So, you may not normally want to display `reason` to the user,
        // but let's assume that we know that `reason` is human-readable.
        match code {
            CloseCode::Normal => println!("The client is done with the connection."),
            CloseCode::Away   => println!("The client is leaving the site."),
            _ => println!("The client encountered an error: {}", reason),
        }
    }
}

fn main() {
    let _ = env_logger::builder().is_test(true).try_init();
    let args: IntifaceCLIArguments = argh::from_env();
    if let Some(port) = args.wsinsecureport {
        ws::listen(format!("127.0.0.1:{}", port), |out| Server::new(out, &args.servername, args.pingtime)).unwrap();
    }
}
