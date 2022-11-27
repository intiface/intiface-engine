use crate::EngineOptions;
use buttplug::server::{
  device::hardware::communication::{
    btleplug::BtlePlugCommunicationManagerBuilder,
    lovense_connect_service::LovenseConnectServiceCommunicationManagerBuilder,
    websocket_server::websocket_server_comm_manager::WebsocketServerDeviceCommunicationManagerBuilder,
  },
  ButtplugServerBuilder,
};
// Device communication manager setup gets its own module because the includes and platform
// specifics are such a mess.

pub fn setup_server_device_comm_managers(
  args: &EngineOptions,
  server_builder: &mut ButtplugServerBuilder,
) {
  if args.use_bluetooth_le() {
    info!("Including Bluetooth LE (btleplug) Device Comm Manager Support");
    server_builder.comm_manager(BtlePlugCommunicationManagerBuilder::default());
  }
  if args.use_lovense_connect() {
    info!("Including Lovense Connect App Support");
    server_builder.comm_manager(LovenseConnectServiceCommunicationManagerBuilder::default());
  }
#[cfg(not(any(target_os="android", target_os="ios")))]
  {
    use buttplug::server::device::hardware::communication::{
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
