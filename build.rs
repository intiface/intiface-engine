use anyhow::Result;
use vergen::{vergen, Config, ShaKind};

fn main() -> Result<()> {
  let mut config = Config::default();
  // Change the SHA output to the short variant
  *config.git_mut().sha_kind_mut() = ShaKind::Short;
  // Generate the default 'cargo:' instruction output
  let _ = vergen(config);
  Ok(())
}
