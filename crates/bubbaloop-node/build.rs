fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR")?);

    prost_build::Config::new()
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .file_descriptor_set_path(out_dir.join("descriptor.bin"))
        .compile_protos(&["protos/header.proto"], &["protos/"])?;

    println!("cargo:rerun-if-changed=protos/header.proto");
    Ok(())
}
