use super::{process_messages::EngineMessage, options::frontend_pipe};
use futures::{select, FutureExt};
#[cfg(target_os = "windows")]
use tokio::net::windows::named_pipe;
use tokio::{
  self,
  io::AsyncWriteExt,
  io::Interest,
  sync::mpsc::{channel, Sender},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug)]
pub struct FrontendPBufChannel {
  sender: Option<Sender<EngineMessage>>,
}

impl FrontendPBufChannel {
  pub fn create(frontend_cancellation_token: CancellationToken, process_ended_token: CancellationToken) -> Self {
    let sender = if let Some(pipe_name) = frontend_pipe() {
      let (outgoing_sender, mut outgoing_receiver) = channel::<EngineMessage>(256);
      tokio::spawn(async move {
        #[cfg(target_os="windows")]
        let mut client = named_pipe::ClientOptions::new()
          .open(pipe_name)
          .unwrap();

        #[cfg(not(target_os="windows"))]
        unimplemented!("Implement domain sockets!");

        loop {
          select! {
            outgoing_msg = outgoing_receiver.recv().fuse() => {
              match outgoing_msg {
                Some(msg) => {
                  if let Err(e) = client.write_all(&serde_json::to_vec(&msg).unwrap()).await {
                    error!("{:?}", e);
                    break;
                  }
                }
                None => return,
              };
            },
            incoming_result = client.ready(Interest::READABLE).fuse() => {
              match incoming_result {
                Ok(_) => {
                  info!("Got incoming data, shutting down process.");
                  frontend_cancellation_token.cancel();
                },
                Err(_) => return,
              };
            },
            _ = process_ended_token.cancelled().fuse() => {
              break;
            }
          }
        }
      });
      Some(outgoing_sender)
    } else {
      None
    };
    Self {
      sender
    }
  }

  pub fn has_frontend(&self) -> bool {
    self.sender.is_some()
  }

  pub async fn send(&self, msg: EngineMessage) {
    if let Some(sender) = &self.sender {
      sender.send(msg).await.unwrap();
    }
  }
}
