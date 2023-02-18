use anyhow::Result;
use std::{
  env,
  fs::File,
  io::{BufWriter, Write},
  path::Path,
};
use vergen::{vergen, Config, ShaKind};

fn main() -> Result<()> {
  let out_dir = env::var("OUT_DIR")?;
  let dest_path = Path::new(&out_dir).join("sentry_api_key.txt");
  let mut f = BufWriter::new(File::create(dest_path)?);
  // If we have an API key available, save it to a file so we can build it in. If not, leave it
  // blank.
  if let Ok(api_key) = env::var("SENTRY_API_KEY") {
    write!(f, "{}", api_key)?;
  }

  let mut config = Config::default();
  // Change the SHA output to the short variant
  *config.git_mut().sha_kind_mut() = ShaKind::Short;
  // Generate the default 'cargo:' instruction output
  let _ = vergen(config);
  Ok(())
}
