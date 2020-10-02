use async_channel::{bounded, Receiver, Sender};
#[cfg(not(target_os = "windows"))]
use async_std::os::unix::io::FromRawFd;
#[cfg(target_os = "windows")]
use async_std::os::windows::io::FromRawHandle;
use async_std::{fs::File, io::stdin, task};
use intiface_gui::{
  server_process_message::{Msg, ProcessEnded, self},
  ServerControlMessage, ServerProcessMessage,
};
use futures::{select, AsyncReadExt, AsyncWriteExt, FutureExt, StreamExt};

use prost::Message;

pub mod intiface_gui {
  include!(concat!(env!("OUT_DIR"), "/intiface_gui_protocol.rs"));
}

#[derive(Clone)]
pub struct FrontendPBufChannel {
  sender: Sender<ServerProcessMessage>,
  receiver: Receiver<ServerControlMessage>,
}

impl FrontendPBufChannel {
  pub fn new(
    sender: Sender<ServerProcessMessage>,
    receiver: Receiver<ServerControlMessage>,
  ) -> Self {
    Self { sender, receiver }
  }

  pub fn get_receiver(&self) -> Receiver<ServerControlMessage> {
    self.receiver.clone()
  }

  pub async fn send(&self, msg: server_process_message::Msg) {
    let server_msg = ServerProcessMessage { msg: Some(msg) };
    self.sender.send(server_msg).await;
  }
}

pub fn run_frontend_task() -> FrontendPBufChannel {
  // TODO check static here to make sure we haven't run already.
  let (outgoing_sender, mut outgoing_receiver) = bounded::<ServerProcessMessage>(256);
  let (incoming_sender, incoming_receiver) = bounded::<ServerControlMessage>(256);
  task::spawn(async move {
    // Due to stdout being wrapped by a linewriter in the standard library, we
    // need to handle writing ourselves here. This requires unsafe code,
    // unfortunately.
    let mut stdout;
    #[cfg(not(target_os = "windows"))]
    unsafe {
      stdout = File::from_raw_fd(1);
    }
    #[cfg(target_os = "windows")]
    unsafe {
      let out_handle = kernel32::GetStdHandle(winapi::um::winbase::STD_OUTPUT_HANDLE);
      stdout = File::from_raw_handle(out_handle);
    }
    let mut stdin = stdin();
    let mut stdin_buf = [0u8; 1024];
    loop {
      select! {
        outgoing_msg = outgoing_receiver.next().fuse() => {
          match outgoing_msg {
            Some(msg) => {
              let mut buf = vec![];
              msg.encode_length_delimited(&mut buf).unwrap();
              stdout.write_all(&buf).await.unwrap();
              stdout.flush().await.unwrap();
            }
            None => break,
          };
        },
        incoming_result = stdin.read(&mut stdin_buf).fuse() => {
          match incoming_result {
            Ok(size) => {
              let out_msg = ServerProcessMessage::decode_length_delimited(&stdin_buf[0..size]);
              let msg = ServerProcessMessage { msg: Some(Msg::ProcessEnded(ProcessEnded::default())) };
              let mut buf = vec![];
              msg.encode_length_delimited(&mut buf).unwrap();
              stdout.write_all(&buf).await.unwrap();
              stdout.flush().await.unwrap();
              std::process::exit(0);
            },
            Err(err) => break,
          };
        },
      }
    }
  });
  FrontendPBufChannel::new(outgoing_sender, incoming_receiver)
}
