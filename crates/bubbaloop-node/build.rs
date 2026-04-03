fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR")?);

    prost_build::Config::new()
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .file_descriptor_set_path(out_dir.join("descriptor.bin"))
        .compile_protos(&["protos/header.proto"], &["protos/"])?;

    println!("cargo:rerun-if-changed=protos/header.proto");

    // Write header.proto to OUT_DIR so dependent node build scripts can use it
    // as a proto include path. Works correctly whether the SDK is a path dep,
    // git dep, or crates.io dep (all have stable OUT_DIR within a build).
    // Consumed via DEP_BUBBALOOP_NODE_PROTOS_DIR (requires links = "bubbaloop-node").
    let proto_src = std::path::Path::new("protos/header.proto");
    let proto_dst = out_dir.join("protos").join("header.proto");
    std::fs::create_dir_all(out_dir.join("protos"))?;
    std::fs::copy(proto_src, &proto_dst)?;
    println!("cargo:protos_dir={}", out_dir.join("protos").display());
    Ok(())
}
