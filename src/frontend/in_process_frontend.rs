//use flutter_rust_bridge::StreamSink;
use super::{
  process_messages::EngineMessage,
  Frontend
};
use crate::error::IntifaceError;
use async_trait::async_trait;


use tokio::{
  self,
  sync::{
    Notify,
  },
};
use tokio_util::sync::CancellationToken;
use std::sync::Arc;


#[derive(Clone)]
pub struct InProcessFrontend {
  //sink: StreamSink<String>,
  disconnect_notifier: Arc<Notify>,
  cancellation_token: CancellationToken,
}

impl InProcessFrontend {
  pub fn new(
    //sink: StreamSink<String>,
    cancellation_token: CancellationToken
  ) -> Self {
    Self { disconnect_notifier: Arc::new(Notify::new()), cancellation_token }
  }
}

#[async_trait]
impl Frontend for InProcessFrontend {
  async fn connect(&self) -> Result<(), IntifaceError> {
    Ok(())
  }

  async fn send(&self, msg: EngineMessage) {
    //self.sink.add(serde_json::to_string(&msg).unwrap());
  }

  fn disconnect(self) {
    self.disconnect_notifier.notify_waiters();
  }
}
