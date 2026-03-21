use std::sync::Arc;

/// Declare a Zenoh queryable that serves the node's protobuf FileDescriptorSet.
///
/// Responds to queries on `bubbaloop/{scope}/{machine_id}/{node_name}/schema`.
/// Does NOT use `.complete(true)` — that would block wildcard queries
/// like `bubbaloop/**/schema` used by the dashboard for discovery.
pub async fn declare_schema_queryable(
    session: &Arc<zenoh::Session>,
    scope: &str,
    machine_id: &str,
    node_name: &str,
    descriptor: &'static [u8],
) -> anyhow::Result<zenoh::query::Queryable<()>> {
    let schema_key = format!("bubbaloop/{}/{}/{}/schema", scope, machine_id, node_name);

    let queryable = session
        .declare_queryable(&schema_key)
        .callback({
            let descriptor = descriptor.to_vec();
            move |query| {
                log::debug!("Schema query received");
                let descriptor_clone = descriptor.clone();
                tokio::spawn(async move {
                    if let Err(e) = query.reply(&query.key_expr().clone(), descriptor_clone.as_slice()).await {
                        log::warn!("Failed to reply to schema query: {}", e);
                    }
                });
            }
        })
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create schema queryable: {}", e))?;

    log::info!("Schema queryable: {}", schema_key);
    Ok(queryable)
}
