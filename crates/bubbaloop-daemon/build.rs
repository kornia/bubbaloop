use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
    let protos_dir = PathBuf::from("../../protos");

    // Compile daemon proto
    let mut prost_build = prost_build::Config::new();
    prost_build.type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]");
    prost_build.file_descriptor_set_path(out_dir.join("daemon_descriptor.bin"));
    prost_build.compile_protos(
        &["../../protos/bubbaloop/daemon.proto"],
        &[protos_dir.to_string_lossy().as_ref()],
    )?;

    println!("cargo:rerun-if-changed=../../protos/bubbaloop/daemon.proto");

    Ok(())
}
