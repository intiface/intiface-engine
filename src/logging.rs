use tokio_util::sync::CancellationToken;
use tracing::Level;
use tokio::{
  sync::mpsc::channel,
  select
};
use std::sync::Arc;
use crate::{
  frontend::{Frontend, EngineMessage}
};
use tracing_subscriber::{
  filter::{LevelFilter, EnvFilter},
  layer::SubscriberExt,
  util::SubscriberInitExt
};
use buttplug::util::logging::ChannelWriter;

pub fn setup_frontend_logging(log_level: Level, frontend: Arc<dyn Frontend>, stop_token: CancellationToken) {
// Add panic hook for emitting backtraces through the logging system.
log_panics::init();
let (bp_log_sender, mut receiver) = channel::<Vec<u8>>(256);
let log_sender = frontend.clone();
tokio::spawn(async move {
  log_sender.send(EngineMessage::EngineStarted{}).await;
  loop {
    select! {
      log = receiver.recv() => {
        let log = log.unwrap();
        log_sender
          .send(EngineMessage::EngineLog{message: std::str::from_utf8(&log).unwrap().to_owned()})
          .await;
      },
      _ = stop_token.cancelled() => {
        break;
      }
    }
  }
});

tracing_subscriber::registry()
  .with(LevelFilter::from(log_level))
  .with(tracing_subscriber::fmt::layer()
    .json()
    //.with_max_level(log_level)
    .with_ansi(false)
    .with_writer(move || ChannelWriter::new(bp_log_sender.clone()))
  )
  //.with(sentry_tracing::layer())
  .try_init()
  .unwrap();
}

pub fn setup_console_logging(log_level: Option<Level>) {
  if log_level.is_some() {
    tracing_subscriber::registry()
    .with(tracing_subscriber::fmt::layer())
    //.with(sentry_tracing::layer())
    .with(LevelFilter::from(log_level))
    .try_init()
    .unwrap(); 
  } else {
    tracing_subscriber::registry()
    .with(tracing_subscriber::fmt::layer())
    //.with(sentry_tracing::layer())
    .with(EnvFilter::try_from_default_env()
      .or_else(|_| EnvFilter::try_new("info"))
      .unwrap())
    .try_init()
    .unwrap(); 
  };
  println!("Intiface Server, starting up with stdout output.");
}