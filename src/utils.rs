use super::IntifaceCLIErrorEnum;
use rcgen::generate_simple_self_signed;
use std::{
  path::PathBuf,
  fs::File,
  process,
  io::Write,
};

pub fn generate_certificate(path: String) -> Result<(), IntifaceCLIErrorEnum> {
  debug!("Generate cert command used, creating cert and exiting.");
  let subject_alt_names = vec!["localhost".to_string()];
  let cert = generate_simple_self_signed(subject_alt_names).unwrap();
  let mut base_path = PathBuf::new();
  base_path.push(&path);
  if !base_path.is_dir() {
      println!(
          "Certificate write path {} does not exist or is not a directory.",
          path
      );
      process::exit(1);
  }
  base_path.set_file_name("cert.pem");
  let mut pem_out = File::create(&base_path).map_err(|x| IntifaceCLIErrorEnum::from(x))?;
  base_path.set_file_name("key.pem");
  let mut key_out = File::create(&base_path).map_err(|x| IntifaceCLIErrorEnum::from(x))?;
  write!(pem_out, "{}", cert.serialize_pem().unwrap())
      .map_err(|x| IntifaceCLIErrorEnum::from(x))?;
  write!(key_out, "{}", cert.serialize_private_key_pem())
      .map_err(|x| IntifaceCLIErrorEnum::from(x))?;
  return Ok(());
}