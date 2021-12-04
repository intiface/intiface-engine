use futures::{select, FutureExt};
use intiface_gui::{
  server_process_message::{self, Msg},
  ServerProcessMessage,
};
use tokio::{
  self,
  io::{stdin, stdout},
  io::{AsyncReadExt, AsyncWriteExt},
  sync::mpsc::{channel, Sender},
};
use tokio_util::sync::CancellationToken;

use prost::Message;

pub mod intiface_gui {
  include!(concat!(env!("OUT_DIR"), "/intiface_gui_protocol.rs"));
}

#[derive(Clone)]
pub struct FrontendPBufChannel {
  sender: Sender<ServerProcessMessage>,
}

impl FrontendPBufChannel {
  pub fn new(sender: Sender<ServerProcessMessage>) -> Self {
    Self { sender }
  }

  pub async fn send(&self, msg: server_process_message::Msg) {
    let server_msg = ServerProcessMessage { msg: Some(msg) };
    self.sender.send(server_msg).await.unwrap();
  }
}

pub fn run_frontend_task(token: CancellationToken) -> FrontendPBufChannel {
  // TODO check static here to make sure we haven't run already.
  let (outgoing_sender, mut outgoing_receiver) = channel::<ServerProcessMessage>(256);
  tokio::spawn(async move {
    let mut stdout = stdout();
    let mut stdin = stdin();
    let mut stdin_buf = [0u8; 1024];
    loop {
      select! {
        outgoing_msg = outgoing_receiver.recv().fuse() => {
          match outgoing_msg {
            Some(msg) => {
              let mut buf = vec![];
              msg.encode_length_delimited(&mut buf).unwrap();
              stdout.write_all(&buf).await.unwrap();
              stdout.flush().await.unwrap();
              // ProcessEnded is the last thing we send before exiting, so if we just sent that, bail.
              if let Some(Msg::ProcessEnded(_)) = msg.msg {
                return;
              }
            }
            None => return,
          };
        },
        incoming_result = stdin.read(&mut stdin_buf).fuse() => {
          match incoming_result {
            Ok(_) => {
              info!("Got incoming data, shutting down process.");
              token.cancel();
            },
            Err(_) => return,
          };
        },
      }
    }
  });
  FrontendPBufChannel::new(outgoing_sender)
}
