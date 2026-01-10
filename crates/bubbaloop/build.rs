fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut prost_build = prost_build::Config::new();
    prost_build.type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]");
    prost_build.compile_protos(&["../../protos/bubbaloop/camera.proto"], &["../../protos/"])?;

    println!("cargo:rerun-if-changed=../../protos/bubbaloop/camera.proto");

    Ok(())
}
