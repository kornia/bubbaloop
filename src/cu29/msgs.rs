use serde::{ser::SerializeStruct, Deserialize, Serialize};

type ImageRgb8 = kornia::image::Image<u8, 3>;

#[derive(Clone)]
pub struct ImageRgb8Msg {
    pub stamp_ns: u64,
    pub channel_id: u8,
    pub image: ImageRgb8,
}

// TODO: implement in kornia-image
impl std::fmt::Debug for ImageRgb8Msg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ImageRgb8Msg(stamp_ns: {}, channel_id: {}, size: {:?})",
            self.stamp_ns,
            self.channel_id,
            self.image.size()
        )
    }
}

// TODO: implement Image::empty()
impl Default for ImageRgb8Msg {
    fn default() -> Self {
        Self {
            stamp_ns: 0,
            channel_id: 0,
            image: ImageRgb8::new([0, 0].into(), vec![]).unwrap(),
        }
    }
}

// TODO: implement in kornia-image
impl bincode::enc::Encode for ImageRgb8Msg {
    fn encode<E: bincode::enc::Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), bincode::error::EncodeError> {
        bincode::Encode::encode(&self.stamp_ns, encoder)?;
        bincode::Encode::encode(&self.channel_id, encoder)?;
        // TODO: support image encoding in kornia_rs::Image
        bincode::Encode::encode(&self.image.rows(), encoder)?;
        bincode::Encode::encode(&self.image.cols(), encoder)?;
        bincode::Encode::encode(&self.image.as_slice(), encoder)?;
        Ok(())
    }
}

// TODO: implement in kornia-image
impl<C> bincode::de::Decode<C> for ImageRgb8Msg {
    fn decode<D: bincode::de::Decoder<Context = C>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        let stamp_ns = bincode::Decode::decode(decoder)?;
        let channel_id = bincode::Decode::decode(decoder)?;
        // TODO: support image encoding in kornia_rs::Image
        let rows = bincode::Decode::decode(decoder)?;
        let cols = bincode::Decode::decode(decoder)?;
        let data = bincode::Decode::decode(decoder)?;
        let image = ImageRgb8::new([rows, cols].into(), data)
            .map_err(|e| bincode::error::DecodeError::OtherString(e.to_string()))?;
        Ok(Self {
            stamp_ns,
            channel_id,
            image,
        })
    }
}

impl Serialize for ImageRgb8Msg {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut s = serializer.serialize_struct("ImageRgb8Msg", 3)?;
        s.serialize_field("stamp_ns", &self.stamp_ns)?;
        s.serialize_field("channel_id", &self.channel_id)?;
        // TODO: support image encoding in kornia_rs::Image
        s.serialize_field("rows", &self.image.rows())?;
        s.serialize_field("cols", &self.image.cols())?;
        s.serialize_field("data", &self.image.as_slice())?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for ImageRgb8Msg {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct SerializedImage {
            stamp_ns: u64,
            channel_id: u8,
            rows: usize,
            cols: usize,
            data: Vec<u8>,
        }

        let data = SerializedImage::deserialize(deserializer)?;
        Ok(Self {
            stamp_ns: data.stamp_ns,
            channel_id: data.channel_id,
            // TODO: support image encoding in kornia_rs::Image
            image: ImageRgb8::new([data.rows, data.cols].into(), data.data)
                .map_err(serde::de::Error::custom)?,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub struct EncodedImage {
    pub stamp_ns: u64,
    pub channel_id: u8,
    pub data: Vec<u8>,
    pub encoding: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub struct PromptResponseMsg {
    pub stamp_ns: u64,
    pub channel_id: u8,
    pub prompt: String,
    pub response: String,
}
