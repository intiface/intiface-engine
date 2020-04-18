use super::ServerOptions;
use async_std::sync::Receiver;
use buttplug::{
    self,
    server::{
        comm_managers::btleplug::BtlePlugCommunicationManager,
        wrapper::{ButtplugJSONServerWrapper, ButtplugServerWrapper},
    },
};
use std::sync::Arc;

pub mod intiface_gui {
    include!(concat!(env!("OUT_DIR"), "/intiface_gui_protocol.rs"));
}

pub type ButtplugServerFactory =
    Arc<Box<dyn Fn() -> (ButtplugJSONServerWrapper, Receiver<String>) + Send + Sync>>;

pub fn create_server_factory(server_info: ServerOptions) -> ButtplugServerFactory {
    Arc::new(Box::new(move || {
        let (mut server, receiver) = ButtplugJSONServerWrapper::new(
            &server_info.server_name,
            server_info.max_ping_time as u128,
        );
        server
            .server_ref()
            .add_comm_manager::<BtlePlugCommunicationManager>();
        #[cfg(target_os="windows")]
        server.server_ref().add_comm_manager::<buttplug::server::comm_managers::xinput::XInputDeviceCommunicationManager>();

        // At this point, we should set up a listener to output Intiface messages
        // based on server events.

        (server, receiver)
    }))
}
