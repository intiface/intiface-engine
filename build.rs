fn main() {
  prost_build::compile_protos(&["src/IntifaceGui.proto"],
                              &["src/"]).unwrap();
}