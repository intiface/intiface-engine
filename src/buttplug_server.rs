use std::sync::Arc;

use crate::{
  BackdoorServer, ButtplugRemoteServer, ButtplugServerConnectorError, EngineOptions,
  IntifaceEngineError, IntifaceError,
};
use buttplug::{
  core::{
    connector::{
      ButtplugRemoteServerConnector, ButtplugWebsocketClientTransport,
      ButtplugWebsocketServerTransportBuilder,
    },
    message::serializer::ButtplugServerJSONSerializer,
  },
  server::{
    device::{
      configuration::DeviceConfigurationManager,
      hardware::communication::{
        btleplug::BtlePlugCommunicationManagerBuilder,
        lovense_connect_service::LovenseConnectServiceCommunicationManagerBuilder,
        websocket_server::websocket_server_comm_manager::WebsocketServerDeviceCommunicationManagerBuilder,
      },
      ServerDeviceManagerBuilder,
    },
    ButtplugServerBuilder,
  },
  util::device_configuration::load_protocol_configs,
};
use once_cell::sync::OnceCell;
// Device communication manager setup gets its own module because the includes and platform
// specifics are such a mess.

pub fn setup_server_device_comm_managers(
  args: &EngineOptions,
  server_builder: &mut ServerDeviceManagerBuilder,
) {
  if args.use_bluetooth_le() {
    info!("Including Bluetooth LE (btleplug) Device Comm Manager Support");
    let mut command_manager_builder = BtlePlugCommunicationManagerBuilder::default();
    #[cfg(target_os = "ios")]
    command_manager_builder.requires_keepalive(true);
    #[cfg(not(target_os = "ios"))]
    command_manager_builder.requires_keepalive(false);
    server_builder.comm_manager(command_manager_builder);
  }
  if args.use_lovense_connect() {
    info!("Including Lovense Connect App Support");
    server_builder.comm_manager(LovenseConnectServiceCommunicationManagerBuilder::default());
  }
  #[cfg(not(any(target_os = "android", target_os = "ios")))]
  {
    use buttplug::server::device::hardware::communication::{
      hid::HidCommunicationManagerBuilder,
      lovense_dongle::{
        LovenseHIDDongleCommunicationManagerBuilder, LovenseSerialDongleCommunicationManagerBuilder,
      },
      serialport::SerialPortCommunicationManagerBuilder,
    };
    if args.use_lovense_dongle_hid() {
      info!("Including Lovense HID Dongle Support");
      server_builder.comm_manager(LovenseHIDDongleCommunicationManagerBuilder::default());
    }
    if args.use_lovense_dongle_serial() {
      info!("Including Lovense Serial Dongle Support");
      server_builder.comm_manager(LovenseSerialDongleCommunicationManagerBuilder::default());
    }
    if args.use_serial_port() {
      info!("Including Serial Port Support");
      server_builder.comm_manager(SerialPortCommunicationManagerBuilder::default());
    }
    if args.use_hid() {
      info!("Including Hid Support");
      server_builder.comm_manager(HidCommunicationManagerBuilder::default());
    }
    #[cfg(target_os = "windows")]
    {
      use buttplug::server::device::hardware::communication::xinput::XInputDeviceCommunicationManagerBuilder;
      if args.use_xinput() {
        info!("Including XInput Gamepad Support");
        server_builder.comm_manager(XInputDeviceCommunicationManagerBuilder::default());
      }
    }
  }
  if args.use_device_websocket_server() {
    info!("Including Websocket Server Device Support");
    let mut builder =
      WebsocketServerDeviceCommunicationManagerBuilder::default().listen_on_all_interfaces(true);
    if let Some(port) = args.device_websocket_server_port() {
      builder = builder.server_port(port);
    }
    server_builder.comm_manager(builder);
  }
}

pub async fn setup_buttplug_server(
  options: &EngineOptions,
  backdoor_server: &OnceCell<Arc<BackdoorServer>>,
  dcm: &Option<Arc<DeviceConfigurationManager>>,
) -> Result<ButtplugRemoteServer, IntifaceEngineError> {
  let mut dm_builder = if let Some(dcm) = dcm {
    ServerDeviceManagerBuilder::new_with_arc(dcm.clone())
  } else {
    let mut dcm_builder = load_protocol_configs(
      options.device_config_json(),
      options.user_device_config_json(),
      false,
    )
    .map_err(|e| IntifaceEngineError::ButtplugError(e.into()))?;

    dcm_builder.allow_raw_messages(options.allow_raw_messages());

    ServerDeviceManagerBuilder::new(
      dcm_builder
        .finish()
        .map_err(|e| IntifaceEngineError::ButtplugError(e.into()))?,
    )
  };

  setup_server_device_comm_managers(options, &mut dm_builder);

  let mut server_builder = ButtplugServerBuilder::new(
    dm_builder
      .finish()
      .map_err(|e| IntifaceEngineError::ButtplugServerError(e))?,
  );
  server_builder
    .name(options.server_name())
    .max_ping_time(options.max_ping_time());

  let core_server = match server_builder.finish() {
    Ok(server) => server,
    Err(e) => {
      error!("Error starting server: {:?}", e);
      return Err(IntifaceEngineError::ButtplugServerError(e));
    }
  };
  if backdoor_server
    .set(Arc::new(BackdoorServer::new(core_server.device_manager())))
    .is_err()
  {
    Err(
      IntifaceError::new("BackdoorServer already initialized somehow! This should never happen!")
        .into(),
    )
  } else {
    Ok(ButtplugRemoteServer::new(core_server))
  }
}

pub async fn run_server(
  server: &ButtplugRemoteServer,
  options: &EngineOptions,
) -> Result<(), ButtplugServerConnectorError> {
  if let Some(port) = options.websocket_port() {
    server
      .start(ButtplugRemoteServerConnector::<
        _,
        ButtplugServerJSONSerializer,
      >::new(
        ButtplugWebsocketServerTransportBuilder::default()
          .port(port)
          .listen_on_all_interfaces(options.websocket_use_all_interfaces())
          .finish(),
      ))
      .await
  } else if let Some(addr) = options.websocket_client_address() {
    server
      .start(ButtplugRemoteServerConnector::<
        _,
        ButtplugServerJSONSerializer,
      >::new(
        ButtplugWebsocketClientTransport::new_insecure_connector(&addr),
      ))
      .await
  } else {
    panic!("Websocket port not set, cannot create transport. Please specify a websocket port in arguments.");
  }
}
