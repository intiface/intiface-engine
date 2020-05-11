use super::{
  ConnectorOptions, 
  IntifaceCLIErrorEnum,
  server::ButtplugServerFactory,
};
use buttplug::server::wrapper::ButtplugServerWrapper;
use futures::prelude::{
  stream::StreamExt,
  sink::SinkExt,
  AsyncRead, 
  AsyncWrite
};
use std::{
    fs::File,
    io::{self, BufReader},
    sync::Arc,
};
use async_std::{
  prelude::FutureExt,
  task::{self, JoinHandle},
  net::{TcpListener},
};
use async_tls::TlsAcceptor;
use rustls::{
  NoClientAuth, 
  ServerConfig,
  internal::pemfile::{certs, pkcs8_private_keys}
};

enum StreamOutput {
    WebsocketMessage(async_tungstenite::tungstenite::Message),
    ButtplugMessage(String),
}

async fn accept_connection<S>(stream: S, server_factory: ButtplugServerFactory)
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let ws_stream = async_tungstenite::accept_async(stream)
        .await
        .expect("Error during the websocket handshake occurred");

    info!("New WebSocket connection.");

    let (mut server, mut receiver) = server_factory();
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
                        async_tungstenite::tungstenite::Message::Ping(_) => {
                            // noop
                            continue;
                        }
                        async_tungstenite::tungstenite::Message::Pong(_) => {
                            // noop
                            continue;
                        }
                        async_tungstenite::tungstenite::Message::Binary(_) => {
                            panic!("Don't know how to handle binary message types!");
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

pub fn create_websocket_listeners(
    connector_opts: ConnectorOptions,
    server_factory: ButtplugServerFactory,
) -> Result<Vec<JoinHandle<()>>, IntifaceCLIErrorEnum> {
    let mut tasks = vec![];

    if let Some(ws_insecure_port) = connector_opts.ws_insecure_port {
        let factory_clone = server_factory.clone();
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
                // TODO Seriously?
                let factory_clone_clone = factory_clone.clone();
                task::spawn(async move {
                    accept_connection(stream, factory_clone_clone).await;
                });
            }
        }));
    }

    if let Some(ws_secure_port) = connector_opts.ws_secure_port {
        let certs =
            certs(&mut BufReader::new(File::open(connector_opts.ws_cert_file.unwrap())?)).map_err(|_| {
                IntifaceCLIErrorEnum::from(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid cert file",
                ))
            })?;

        let mut keys =
            pkcs8_private_keys(&mut BufReader::new(File::open(connector_opts.ws_priv_file.unwrap())?))
                .map_err(|_| {
                    IntifaceCLIErrorEnum::from(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "invalid key file",
                    ))
                })?;

        // we don't use client authentication
        let mut config = ServerConfig::new(NoClientAuth::new());
        config
            // set this server to use one cert together with the loaded private key
            .set_single_cert(certs, keys.remove(0))
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
        let acceptor = TlsAcceptor::from(Arc::new(config));
        let factory_clone = server_factory.clone();
        tasks.push(task::spawn(async move {
            let addr = format!("127.0.0.1:{}", ws_secure_port);
            debug!("Websocket Secure: Trying to listen on {}", addr);
            // Create the event loop and TCP listener we'll accept connections on.
            let try_socket = TcpListener::bind(&addr).await;
            debug!("Websocket Secure: Socket bound.");
            let listener = try_socket.expect("Failed to bind");
            debug!("Websocket Secure: Listening on: {}", addr);

            while let Ok((stream, _)) = listener.accept().await {
                let handshake = acceptor.accept(stream);
                // The handshake is a future we can await to get an encrypted
                // stream back.
                let tls_stream = handshake.await.unwrap();
                info!("Websocket Secure: Got connection");
                // TODO Seriously?
                let factory_clone_clone = factory_clone.clone();
                task::spawn(async move {
                    accept_connection(tls_stream, factory_clone_clone).await;
                });
            }
        }));
    }
    Ok(tasks)
}