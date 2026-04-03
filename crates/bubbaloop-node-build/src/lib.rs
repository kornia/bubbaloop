/// Build helper for bubbaloop nodes.
///
/// Wraps `prost-build` with `extern_path`, descriptor output, and `header.proto`
/// include resolution pre-configured. Node authors call one function instead of
/// setting up prost-build manually.
///
/// # Usage
///
/// In your node's `Cargo.toml`:
/// ```toml
/// [build-dependencies]
/// bubbaloop-node-build = { git = "https://github.com/kornia/bubbaloop.git", branch = "main" }
/// ```
///
/// In your node's `build.rs`:
/// ```rust,no_run
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     bubbaloop_node_build::compile_protos(&["protos/my_node.proto"])
/// }
/// ```
use std::{env, fs, path::{Path, PathBuf}};

/// `header.proto` embedded at compile time — written to OUT_DIR so protoc can
/// resolve `import "header.proto"` in node-specific proto files.
const HEADER_PROTO: &str = include_str!("../protos/header.proto");

/// Compile node-specific proto files with bubbaloop header types pre-configured.
///
/// Equivalent to `prost_build::compile_protos` but with:
/// - `extern_path` for `bubbaloop.header.v1` → `::bubbaloop_node::schemas::header::v1`
/// - `header.proto` available as an include so `import "header.proto"` resolves
/// - `descriptor.bin` written to `OUT_DIR` for schema queryable registration
pub fn compile_protos(
    protos: &[impl AsRef<Path>],
) -> Result<(), Box<dyn std::error::Error>> {
    configure().compile_protos(protos)
}

/// Returns a [`Builder`] for customised compilation.
pub fn configure() -> Builder {
    Builder::new()
}

/// Builder for node proto compilation.
///
/// Wraps `prost_build::Config` with bubbaloop defaults pre-applied.
/// Chain additional configuration before calling [`Builder::compile_protos`].
pub struct Builder {
    config: prost_build::Config,
    extra_includes: Vec<PathBuf>,
}

impl Builder {
    fn new() -> Self {
        let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set — run inside build.rs"));

        // Write header.proto to OUT_DIR so protoc can resolve imports.
        let bubbaloop_protos = out_dir.join("bubbaloop_protos");
        fs::create_dir_all(&bubbaloop_protos)
            .expect("failed to create bubbaloop_protos dir in OUT_DIR");
        fs::write(bubbaloop_protos.join("header.proto"), HEADER_PROTO)
            .expect("failed to write header.proto to OUT_DIR");

        let mut config = prost_build::Config::new();
        config
            // Header Rust type comes from the SDK — do not regenerate it.
            .extern_path(
                ".bubbaloop.header.v1",
                "::bubbaloop_node::schemas::header::v1",
            )
            // Descriptor used by the schema queryable in bubbaloop_node::run_node.
            .file_descriptor_set_path(out_dir.join("descriptor.bin"));

        Self {
            config,
            extra_includes: vec![bubbaloop_protos],
        }
    }

    /// Add a `#[derive(...)]` or other attribute to generated types.
    ///
    /// `path` follows prost-build path syntax (e.g., `"."` for all types).
    pub fn type_attribute(mut self, path: &str, attribute: &str) -> Self {
        self.config.type_attribute(path, attribute);
        self
    }

    /// Add an additional `extern_path` mapping.
    pub fn extern_path(mut self, proto_path: &str, rust_path: &str) -> Self {
        self.config.extern_path(proto_path, rust_path);
        self
    }

    /// Add an extra proto include directory (beyond the node's `protos/` dir
    /// and the built-in `bubbaloop_protos/` dir).
    pub fn include(mut self, path: impl AsRef<Path>) -> Self {
        self.extra_includes.push(path.as_ref().to_path_buf());
        self
    }

    /// Compile the given proto files.
    ///
    /// The `protos/` directory of each proto file is automatically added as an
    /// include path. `header.proto` is always resolvable via the built-in include.
    pub fn compile_protos(
        mut self,
        protos: &[impl AsRef<Path>],
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Auto-include each proto's parent directory.
        let mut includes: Vec<PathBuf> = protos
            .iter()
            .filter_map(|p| p.as_ref().parent().map(Path::to_path_buf))
            .collect();
        includes.dedup();
        includes.extend(self.extra_includes);

        let include_strs: Vec<_> = includes
            .iter()
            .filter_map(|p| p.to_str())
            .collect();

        self.config.compile_protos(protos, &include_strs)?;
        Ok(())
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}
