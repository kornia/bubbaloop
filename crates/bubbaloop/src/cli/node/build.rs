//! Node build commands.

use super::{send_command, Result};

pub(crate) async fn build_node(name: &str) -> Result<()> {
    send_command(name, "build").await
}
