/// Build helper for bubbaloop nodes.
///
/// Wraps `prost-build` with `extern_path`, descriptor output, and `header.proto`
/// include resolution pre-configured.
use std::{env, fs, path::{Path, PathBuf}};

/// Embedded `header.proto` — single source of truth in bubbaloop-schemas.
/// Written to OUT_DIR so protoc can resolve `import "header.proto"`.
const HEADER_PROTO: &str = include_str!("../../bubbaloop-schemas/protos/header.proto");

/// Compile node proto files with bubbaloop header types, includes, and descriptor
/// output pre-configured.
pub fn compile_protos(
    protos: &[impl AsRef<Path>],
) -> Result<(), Box<dyn std::error::Error>> {
    configure().compile_protos(protos)
}

/// Returns a [`Builder`] for customised compilation.
pub fn configure() -> Builder {
    Builder::new()
}

/// Builder for node proto compilation with bubbaloop defaults pre-applied.
pub struct Builder {
    config: prost_build::Config,
    extra_includes: Vec<PathBuf>,
}

impl Builder {
    fn new() -> Self {
        let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set — run inside build.rs"));

        let bubbaloop_protos = out_dir.join("bubbaloop_protos");
        fs::create_dir_all(&bubbaloop_protos)
            .expect("failed to create bubbaloop_protos dir in OUT_DIR");
        fs::write(bubbaloop_protos.join("header.proto"), HEADER_PROTO)
            .expect("failed to write header.proto to OUT_DIR");

        let mut config = prost_build::Config::new();
        config
            .extern_path(
                ".bubbaloop.header.v1",
                "::bubbaloop_node::schemas::header::v1",
            )
            .file_descriptor_set_path(out_dir.join("descriptor.bin"));

        Self {
            config,
            extra_includes: vec![bubbaloop_protos],
        }
    }

    /// Add a type attribute (e.g., `#[derive(...)]`) to generated types.
    pub fn type_attribute(mut self, path: &str, attribute: &str) -> Self {
        self.config.type_attribute(path, attribute);
        self
    }

    /// Add an additional `extern_path` mapping.
    pub fn extern_path(mut self, proto_path: &str, rust_path: &str) -> Self {
        self.config.extern_path(proto_path, rust_path);
        self
    }

    /// Add an extra proto include directory.
    pub fn include(mut self, path: impl AsRef<Path>) -> Self {
        self.extra_includes.push(path.as_ref().to_path_buf());
        self
    }

    /// Compile the given proto files. Each file's parent directory is auto-included.
    pub fn compile_protos(
        mut self,
        protos: &[impl AsRef<Path>],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut includes: Vec<PathBuf> = protos
            .iter()
            .filter_map(|p| p.as_ref().parent().map(Path::to_path_buf))
            .collect();
        includes.dedup();
        includes.extend(self.extra_includes);

        self.config.compile_protos(protos, &includes)?;
        Ok(())
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}
