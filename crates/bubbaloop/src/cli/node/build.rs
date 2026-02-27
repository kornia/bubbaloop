//! Node build commands.

use super::{Result, send_command};

pub(crate) async fn build_node(name: &str) -> Result<()> {
    send_command(name, "build").await
}
