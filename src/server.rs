use super::{
    ServerOptions,
    frontend::{
        FrontendPBufSender, 
        intiface_gui::server_process_message::{
            Msg,
            ClientConnected,
        },
    }
};
use async_std::{
    prelude::StreamExt,
    task,
    sync::{channel, Receiver},
};
use buttplug::{
    self,
    core::messages::ButtplugOutMessage,
    server::{
        ButtplugServer,
        comm_managers::btleplug::BtlePlugCommunicationManager,
        wrapper::ButtplugJSONServerWrapper,
    },
};
use std::sync::{Arc, Mutex};
use lazy_static::lazy_static;

lazy_static! {
    static ref server_storage: Arc<Mutex<Option<ButtplugServer>>> = Arc::new(Mutex::new(None));
}

pub type ButtplugServerFactory =
    Arc<Box<dyn Fn() -> (ButtplugJSONServerWrapper, Receiver<String>) + Send + Sync>>;

fn setup_frontend_filter_channel(mut receiver: Receiver<ButtplugOutMessage>, frontend_sender: FrontendPBufSender) -> Receiver<ButtplugOutMessage> {
    let (sender_filtered, recv_filtered) = channel(256);

    task::spawn(async move {
        loop {
            match receiver.next().await {
                Some(msg) => {
                    match msg {
                        ButtplugOutMessage::ServerInfo(_) => {
                            let msg = ClientConnected {
                                client_name: "Unknown Name".to_string()
                            };
                            frontend_sender.send(Msg::ClientConnected(msg)).await;
                        },
                        _ => {}
                    }
                    sender_filtered.send(msg).await;
                },
                None => break,
            }
        }
    });

    recv_filtered
}

pub fn create_server_factory(server_info: ServerOptions, sender: FrontendPBufSender) -> ButtplugServerFactory {
    Arc::new(Box::new(move || {
        let (mut server, receiver) = ButtplugServer::new(&server_info.server_name,
            server_info.max_ping_time as u128);

        let receiver_filtered = setup_frontend_filter_channel(receiver, sender.clone());
        server.add_comm_manager::<BtlePlugCommunicationManager>();
        #[cfg(target_os="windows")]
        server.add_comm_manager::<buttplug::server::comm_managers::xinput::XInputDeviceCommunicationManager>();
    
        let (server_json, receiver_json) = ButtplugJSONServerWrapper::new_with_server(
            server, receiver_filtered
        );

        // At this point, we should set up a listener to output Intiface messages
        // based on server events.

        (server_json, receiver_json)
    }))
}

pub fn store_server(server: ButtplugServer) {
    let mut storage = server_storage.lock().unwrap();
    if storage.is_some() {
        panic!("Should not be able to store a server on top of an already stored server!");
    }
    *storage = Some(server);
}

pub fn create_single_server_factory(server_info: ServerOptions, sender: FrontendPBufSender) -> ButtplugServerFactory {
    Arc::new(Box::new(move || {
        let mut storage = server_storage.lock().unwrap();
        let mut server;
        let receiver;
        if storage.is_some() {
            server = (*storage).take().unwrap();
            receiver = server.get_event_receiver();
        } else {
            let (new_server, new_receiver) = ButtplugServer::new(&server_info.server_name, server_info.max_ping_time as u128);
            server = new_server;
            receiver = new_receiver;
        }

        let receiver_filtered = setup_frontend_filter_channel(receiver, sender.clone());
        server.add_comm_manager::<BtlePlugCommunicationManager>();
        #[cfg(target_os="windows")]
        server.add_comm_manager::<buttplug::server::comm_managers::xinput::XInputDeviceCommunicationManager>();
    
        let (server_json, receiver_json) = ButtplugJSONServerWrapper::new_with_server(
            server, receiver_filtered
        );

        // At this point, we should set up a listener to output Intiface messages
        // based on server events.

        (server_json, receiver_json)
    }))
}
