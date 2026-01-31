// NOTE: The protos in `protos/` are manually synced from the main bubbaloop repo's
// `protos/bubbaloop/` directory (with import paths changed from "bubbaloop/header.proto"
// to "header.proto" for the flat layout). A CI check should diff header.proto, camera.proto,
// and weather.proto between both locations (stripping import path differences) to prevent drift.

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
    let protos_dir = PathBuf::from("protos");

    let mut proto_files = Vec::new();
    for entry in std::fs::read_dir(&protos_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("proto") {
            proto_files.push(path.to_string_lossy().into_owned());
        }
    }

    if proto_files.is_empty() {
        return Ok(());
    }

    let mut config = prost_build::Config::new();
    config.type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]");
    config.file_descriptor_set_path(out_dir.join("descriptor.bin"));
    config.compile_protos(&proto_files, &[protos_dir.to_string_lossy().as_ref()])?;

    for proto_file in &proto_files {
        println!("cargo:rerun-if-changed={}", proto_file);
    }
    println!("cargo:rerun-if-changed=protos");

    Ok(())
}
