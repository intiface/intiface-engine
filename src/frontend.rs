use tokio::{self, sync::mpsc::{channel, Sender}, io::Interest, io::AsyncWriteExt};
use futures::{select, FutureExt};
use tokio_util::sync::CancellationToken;
use super::process_messages::{EngineMessage, IntifaceMessage};
#[cfg(target_os="windows")]
use tokio::net::windows::named_pipe;

#[derive(Clone)]
pub struct FrontendPBufChannel {
  sender: Sender<EngineMessage>,
}

impl FrontendPBufChannel {
  pub fn new(
    sender: Sender<EngineMessage>,
  ) -> Self {
    Self { sender }
  }

  pub async fn send(&self, msg: EngineMessage) {
    self.sender.send(msg).await.unwrap();
  }
}

pub fn run_frontend_task(token: CancellationToken) -> FrontendPBufChannel {
  // TODO check static here to make sure we haven't run already.
  let (outgoing_sender, mut outgoing_receiver) = channel::<EngineMessage>(256);
  tokio::spawn(async move {
    let mut client = named_pipe::ClientOptions::new().open("\\\\.\\pipe\\intiface").unwrap();
    loop {
      select! {
        outgoing_msg = outgoing_receiver.recv().fuse() => {
          match outgoing_msg {
            Some(msg) => {
              // ProcessEnded is the last thing we send before exiting, so if we just sent that, bail.
              if let EngineMessage::EngineStopped = msg {
                return;
              }
              client.write_all(&serde_json::to_vec(&msg).unwrap()).await;
            }
            None => return,
          };
        },
        incoming_result = client.ready(Interest::READABLE).fuse() => {
          match incoming_result {
            Ok(_) => {
              info!("Got incoming data, shutting down process.");
              token.cancel();
            },
            Err(_) => return,
          };
        }
      }
    }
  });
  FrontendPBufChannel::new(outgoing_sender)
}
