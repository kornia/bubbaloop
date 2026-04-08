fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR")?);

    // Copy header.proto to OUT_DIR so downstream node build scripts
    // (via bubbaloop-node-build) can resolve `import "header.proto"`.
    // Consumed via DEP_BUBBALOOP_NODE_PROTOS_DIR (requires links = "bubbaloop-node").
    let schema_proto = std::path::Path::new("../bubbaloop-schemas/protos/header.proto");
    let proto_dst = out_dir.join("protos").join("header.proto");
    std::fs::create_dir_all(out_dir.join("protos"))?;
    std::fs::copy(schema_proto, &proto_dst)?;
    println!("cargo:protos_dir={}", out_dir.join("protos").display());
    println!("cargo:rerun-if-changed=../bubbaloop-schemas/protos/header.proto");
    Ok(())
}
