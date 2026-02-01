use std::path::PathBuf;

/// Find all proto files in a directory recursively
fn find_proto_files(dir: &str) -> Vec<String> {
    let mut proto_files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                proto_files.extend(find_proto_files(path.to_str().unwrap()));
            } else if path.extension().and_then(|s| s.to_str()) == Some("proto") {
                proto_files.push(path.to_string_lossy().into_owned());
            }
        }
    }
    proto_files
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
    let protos_dir = PathBuf::from("../bubbaloop-schemas/protos");

    // Bubbaloop protobuf files
    let proto_files = find_proto_files(protos_dir.to_str().unwrap());

    // Compile all proto files
    let mut prost_build = prost_build::Config::new();
    prost_build.type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]");
    prost_build.file_descriptor_set_path(out_dir.join("descriptor.bin"));
    prost_build.compile_protos(&proto_files, &[protos_dir.to_string_lossy().as_ref()])?;

    // Rerun if any proto file changes
    for proto_file in &proto_files {
        println!("cargo:rerun-if-changed={}", proto_file);
    }
    println!("cargo:rerun-if-changed={}", protos_dir.to_string_lossy());

    // Rebuild if any dashboard dist file changes (for rust-embed)
    fn watch_dir(dir: &str) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    watch_dir(path.to_str().unwrap());
                } else {
                    println!("cargo:rerun-if-changed={}", path.display());
                }
            }
        }
    }
    watch_dir("../../dashboard/dist");

    Ok(())
}
