fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR")?);

    // Single source of truth: header.proto lives in bubbaloop-schemas
    let schema_proto = std::path::Path::new("../bubbaloop-schemas/protos/header.proto");

    prost_build::Config::new()
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .file_descriptor_set_path(out_dir.join("descriptor.bin"))
        .compile_protos(&[schema_proto], &[schema_proto.parent().unwrap()])?;

    println!("cargo:rerun-if-changed=../bubbaloop-schemas/protos/header.proto");

    // Copy to OUT_DIR so dependent node build scripts can resolve imports.
    // Consumed via DEP_BUBBALOOP_NODE_PROTOS_DIR (requires links = "bubbaloop-node").
    let proto_dst = out_dir.join("protos").join("header.proto");
    std::fs::create_dir_all(out_dir.join("protos"))?;
    std::fs::copy(schema_proto, &proto_dst)?;
    println!("cargo:protos_dir={}", out_dir.join("protos").display());
    Ok(())
}
