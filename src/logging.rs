use crate::frontend::{EngineMessage, Frontend};
use buttplug::util::logging::ChannelWriter;
use once_cell::sync::OnceCell;
use std::sync::Arc;
use tokio::sync::mpsc::channel;
use tracing::Level;
use tracing_subscriber::{
  filter::{EnvFilter, LevelFilter},
  layer::SubscriberExt,
  util::SubscriberInitExt,
};

static FRONTEND_LOGGING_SET: OnceCell<bool> = OnceCell::new();

pub fn setup_frontend_logging(log_level: Level, frontend: Arc<dyn Frontend>) {
  // Add panic hook for emitting backtraces through the logging system.
  log_panics::init();
  let (bp_log_sender, mut receiver) = channel::<Vec<u8>>(256);
  let log_sender = frontend.clone();
  tokio::spawn(async move {
    // We can log until our receiver disappears at this point.
    while let Some(log) = receiver.recv().await {
      log_sender
        .send(EngineMessage::EngineLog {
          message: std::str::from_utf8(&log).unwrap().to_owned(),
        })
        .await;
    }
  });

  if FRONTEND_LOGGING_SET.get().is_none() {
    FRONTEND_LOGGING_SET.set(true).unwrap();
    tracing_subscriber::registry()
      .with(LevelFilter::from(log_level))
      .with(
        tracing_subscriber::fmt::layer()
          .json()
          //.with_max_level(log_level)
          .with_ansi(false)
          .with_writer(move || ChannelWriter::new(bp_log_sender.clone())),
      )
      .with(sentry_tracing::layer())
      .try_init()
      .unwrap();
  }
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
      .with(
        EnvFilter::try_from_default_env()
          .or_else(|_| EnvFilter::try_new("info"))
          .unwrap(),
      )
      .try_init()
      .unwrap();
  };
  println!("Intiface Server, starting up with stdout output.");
}
