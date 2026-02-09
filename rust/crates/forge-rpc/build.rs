use std::error::Error;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    // crates/forge-rpc -> crates -> rust -> repo root -> proto
    let proto_root = manifest_dir.join("../../../proto");
    let proto_file = proto_root.join("forged/v1/forged.proto");

    tonic_build::configure().compile_protos(&[proto_file], &[proto_root])?;

    Ok(())
}
