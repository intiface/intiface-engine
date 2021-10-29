use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineMessage {
  MessageVersion(u32),
  EngineLog(String),
  EngineStarted,
  EngineError(String),
  EngineStopped,
  ClientConnected(String),
  ClientDisconnected,
  DeviceConnected { name: String, index: u32, address: String, display_name: String },
  DeviceDisconnected(u32),
  ClientRejected(String)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IntifaceMessage {
  Stop
}

/*
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineLogMessage {
  
}
*/