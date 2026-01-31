// Types are now fully qualified in the macro registration below

// Utils to convert bubbaloop message to foxglove message
pub(crate) fn extract_timestamp_and_frame_id(
    header: Option<bubbaloop_schemas::Header>,
) -> (Option<foxglove::schemas::Timestamp>, String) {
    header
        .as_ref()
        .map(|h| {
            (
                Some(foxglove::schemas::Timestamp::new(
                    (h.pub_time / 1_000_000_000) as u32,
                    (h.pub_time % 1_000_000_000) as u32,
                )),
                h.frame_id.clone(),
            )
        })
        .unwrap_or((None, String::new()))
}

/// Register message type handlers with topic keyword mapping
/// Usage: register_message_types!(
///     ("compressed" => CompressedImage => FoxgloveCompressedVideo, |msg| { ... }),
///     ("raw" => RawImage => FoxgloveRawImage, |msg| { ... }),
/// );
macro_rules! register_message_types {
    ($(
        ($keyword:tt => $bubbaloop_type:ty => $foxglove_type:ty, |$msg:ident| $converter:expr)
    ),* $(,)?) => {
        // Generate spawn_message_handler macro that inlines handler logic
        macro_rules! spawn_message_handler {
            (
                $topic:expr, $node:expr, $shutdown_rx:expr
            ) => {
                {
                    let topic_lower = $topic.to_lowercase();
                    $(
                        if topic_lower.contains(register_message_types!(@keyword_str $keyword)) {
                            tokio::spawn(async move {
                                register_message_types!(@handle $keyword, $bubbaloop_type, $foxglove_type, $msg, $converter, $node, $topic, $shutdown_rx);
                            })
                        } else
                    )*
                    {
                        log::error!("Cannot infer message type from topic '{}'", $topic);
                        tokio::spawn(async {})
                    }
                }
            };
        }
    };

    // Extract keyword string from token tree (handles string literals)
    (@keyword_str $keyword:literal) => { $keyword };
    (@keyword_str $keyword:tt) => { stringify!($keyword) };

    // Inline handler implementation - no separate function needed
    (@handle $keyword:tt, $bubbaloop_type:ty, $foxglove_type:ty, $msg:ident, $converter:expr, $node:expr, $topic:expr, $shutdown_rx:expr) => {
        use ros_z::Builder;
        let node = $node;
        let topic = $topic;
        let mut shutdown_rx = $shutdown_rx;

        let subscriber = match node
            .create_sub::<$bubbaloop_type>(topic)
            .with_serdes::<ros_z::msg::ProtobufSerdes<$bubbaloop_type>>()
            .build()
        {
            Ok(s) => s,
            Err(e) => {
                log::error!("Failed to subscribe to topic '{}': {}", topic, e);
                return;
            }
        };

        let channel = foxglove::Channel::<$foxglove_type>::new(topic);
        log::info!("Foxglove bridge subscribed to topic: {} ({})", topic, stringify!($bubbaloop_type));

        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => break,
                result = subscriber.async_recv() => {
                    match result {
                        Ok($msg) => {
                            let foxglove_msg = $converter;
                            channel.log(&foxglove_msg);
                        }
                        Err(e) => {
                            log::error!("Error receiving message from topic '{}': {}", topic, e);
                            break;
                        }
                    }
                }
            }
        }

        log::info!("Task for topic '{}' shutting down", topic);
    };
}

// Register all message types
register_message_types!(
    ("compressed" => bubbaloop_schemas::CompressedImage => foxglove::schemas::CompressedVideo, |msg| {
        let (timestamp, frame_id) = crate::messages::extract_timestamp_and_frame_id(msg.header);
        foxglove::schemas::CompressedVideo {
            timestamp,
            frame_id,
            format: msg.format.clone(),
            data: msg.data.into(),
        }
    }),
    ("raw" => bubbaloop_schemas::RawImage => foxglove::schemas::RawImage, |msg| {
        let (timestamp, frame_id) = crate::messages::extract_timestamp_and_frame_id(msg.header);
        foxglove::schemas::RawImage {
            timestamp,
            frame_id,
            width: msg.width,
            height: msg.height,
            encoding: msg.encoding.clone(),
            step: msg.step,
            data: msg.data.into(),
        }
    }),
);
