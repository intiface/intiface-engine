use async_std::{
  prelude::*,
  sync::{channel, Sender},
  task,
  fs::File,
};
#[cfg(not(target_os="windows"))]
use async_std::os::unix::io::FromRawFd;
#[cfg(target_os="windows")]
use async_std::os::windows::io::FromRawHandle;

use prost::Message;

pub mod intiface_gui {
  include!(concat!(env!("OUT_DIR"), "/intiface_gui_protocol.rs"));
}

#[derive(Clone)]
pub struct FrontendPBufSender {
  sender: Option<Sender<intiface_gui::ServerProcessMessage>>
}

impl Default for FrontendPBufSender {
  fn default() -> Self {
    Self {
      sender: None
    }
  }
}

impl FrontendPBufSender {
  pub fn new(sender: Sender<intiface_gui::ServerProcessMessage>) -> Self {
    Self {
      sender: Some(sender)
    }
  }

  pub fn is_active(&self) -> bool {
    self.sender.is_some()
  }

  pub async fn send(&self, msg: intiface_gui::server_process_message::Msg) {
    if let Some(send) = &self.sender {
      let server_msg = intiface_gui::ServerProcessMessage {
        msg: Some(msg)
      };
      send.send(server_msg).await;
    }
  }
}

pub fn run_frontend_task() -> FrontendPBufSender {
  // TODO check static here to make sure we haven't run already.
  let (sender, mut receiver) = channel::<intiface_gui::ServerProcessMessage>(256);
  task::spawn(async move {
    // Due to stdout being wrapped by a linewriter in the standard library, we
    // need to handle writing ourselves here. This requires unsafe code,
    // unfortunately.
    let mut out;
    #[cfg(not(target_os="windows"))]
    unsafe {
      out = File::from_raw_fd(1);
    }
    #[cfg(target_os="windows")]
    unsafe {
      let h = kernel32::GetStdHandle(winapi::um::winbase::STD_OUTPUT_HANDLE);
      out = File::from_raw_handle(h);
    }
    loop {
      match receiver.next().await {
        Some(msg) => {
          let mut buf = vec![];
          msg.encode_length_delimited(&mut buf).unwrap();
          out.write_all(&buf).await.unwrap();
          out.flush().await.unwrap();
        }
        None => break,
      }
    }
  });
  FrontendPBufSender::new(sender)
}
