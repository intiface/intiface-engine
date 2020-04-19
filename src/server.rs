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
use std::sync::Arc;

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
