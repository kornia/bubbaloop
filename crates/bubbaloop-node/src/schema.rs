use std::sync::Arc;
use zenoh::Wait as _;

use crate::error::{NodeError, Result};

/// Declare a Zenoh queryable that serves the node's protobuf FileDescriptorSet.
///
/// Responds to queries on `bubbaloop/global/{machine_id}/{node_name}/schema`.
/// Does NOT use `.complete(true)` — that would block wildcard queries
/// like `bubbaloop/global/**/schema` used by the dashboard for discovery.
pub async fn declare_schema_queryable(
    session: &Arc<zenoh::Session>,
    machine_id: &str,
    node_name: &str,
    descriptor: &'static [u8],
) -> Result<zenoh::query::Queryable<()>> {
    let schema_key = format!("bubbaloop/global/{}/{}/schema", machine_id, node_name);

    let queryable = session
        .declare_queryable(&schema_key)
        .callback({
            let descriptor = descriptor.to_vec();
            move |query| {
                log::debug!("Schema query received");
                let key = query.key_expr().clone();
                if let Err(e) = query.reply(key, descriptor.as_slice()).wait() {
                    log::warn!("Failed to reply to schema query: {}", e);
                }
            }
        })
        .await
        .map_err(NodeError::SchemaQueryable)?;

    log::info!("Schema queryable: {}", schema_key);
    Ok(queryable)
}
