use std::sync::Arc;
use futures::{Stream, StreamExt};
use buttplug::{server::{device::ServerDeviceManager}, core::{message::{Error, serializer::{ButtplugServerJSONSerializer, ButtplugMessageSerializer, ButtplugSerializedMessage}, ButtplugMessageSpecVersion, ButtplugServerMessage}, errors::{ButtplugMessageError, ButtplugError}}};

// Allows direct access to the Device Manager of a running ButtplugServer. Bypasses requirements for
// client handshake, ping, etc...
pub struct BackdoorServer {
  // The device manager of the server.
  device_manager: Arc<ServerDeviceManager>,
  // Unlike clients, which can vary their serializers (but currently don't), we don't expect outside
  // access to a BackdoorServer, so we can hardcode to using JSON.
  serializer: ButtplugServerJSONSerializer
}

impl BackdoorServer {
  pub fn new(device_manager: Arc<ServerDeviceManager>) -> Self {
    let serializer = ButtplugServerJSONSerializer::default();
    serializer.force_message_version(&ButtplugMessageSpecVersion::Version3);
    Self {
      device_manager,
      serializer
    }
  }

  pub fn event_stream(&self) -> impl Stream<Item = String> + '_ {
    // Unlike the client API, we can expect anyone using the server to pin this
    // themselves.
    self.device_manager.event_stream().map(|x| self.serialize_msg(&x))
  }

  fn serialize_msg(&self, msg: &ButtplugServerMessage) -> String {
    let serialized_message = self.serializer.serialize(&[msg.clone()]);
    if let ButtplugSerializedMessage::Text(text_msg) = serialized_message {
      return text_msg;
    } else {
      panic!("We've hardcoded to use a JSON serializer so we shouldn't get binary back.");
    }
  }

  // This has to act like both a message parser *and* a connector, as we're wrapping the serializer
  // here. So it will only take strings, and only output strings.
  pub async fn parse_message(&self, msg: &str) -> String {
    info!("BACKDOOR SERVER GOT NEW MESSAGE");
    let messages = match self
      .serializer
      .deserialize(&ButtplugSerializedMessage::Text(msg.to_owned()))
    {
      Ok(msg) => msg,
      Err(e) => return self.serialize_msg(&Error::from(ButtplugError::from(ButtplugMessageError::MessageSerializationError(e))).into())
    };
    let device_manager = self.device_manager.clone();
    match device_manager.parse_message(messages[0].clone()).await {
      Ok(outgoing_msg) => self.serialize_msg(&outgoing_msg),
      Err(e) => self.serialize_msg(&Error::from(e).into())
    }
  }
}