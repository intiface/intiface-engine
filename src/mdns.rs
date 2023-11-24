use std::collections::HashMap;

use mdns_sd::{ServiceDaemon, ServiceInfo};
use rand::distributions::{ Alphanumeric, DistString };

use crate::EngineOptions;

pub struct IntifaceMdns {
  mdns_daemon: ServiceDaemon
}

impl IntifaceMdns {
  pub fn new(options: &EngineOptions) -> Self {
    // Create a daemon
    let mdns_daemon = ServiceDaemon::new().expect("Failed to create daemon");
    // Create a service info.
    let service_type = "_intiface_engine._tcp.local.";
    let random_suffix = Alphanumeric.sample_string(&mut rand::thread_rng(), 6);
    let instance_name = format!(
      "intiface_engine_{}_{}",
      options
        .mdns_suffix()
        .as_ref()
        .unwrap_or(&"".to_owned())
        .to_owned(),
      random_suffix
    );
    info!(
      "Bringing up mDNS Advertisment using instance name {}",
      instance_name
    );
    let host_name = format!("{}.local.", instance_name);
    let port = options.websocket_port().unwrap_or(12345);
    let properties: HashMap<String, String> = HashMap::new();
    let mut my_service = ServiceInfo::new(
      service_type,
      &instance_name,
      &host_name,
      "",
      port,
      properties,
    )
    .unwrap();
    my_service = my_service.enable_addr_auto();
    mdns_daemon.register(my_service).unwrap();
    Self {
      mdns_daemon
    }
  }

  pub fn shutdown(&self) {
    if let Err(e) = self.mdns_daemon.shutdown() {
      error!("{:?}", e);
    }
  }

}

impl Drop for IntifaceMdns {
  fn drop(&mut self) {
      self.shutdown()
  }
}