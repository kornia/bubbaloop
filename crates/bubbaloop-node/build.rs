fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR")?);

    prost_build::Config::new()
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .file_descriptor_set_path(out_dir.join("descriptor.bin"))
        .compile_protos(&["protos/header.proto"], &["protos/"])?;

    println!("cargo:rerun-if-changed=protos/header.proto");

    // Expose the protos directory so dependent node crates can import header.proto
    // without keeping a local copy. Consumed via DEP_BUBBALOOP_NODE_PROTOS_DIR.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")?;
    println!("cargo:protos_dir={}/protos", manifest_dir);
    Ok(())
}
