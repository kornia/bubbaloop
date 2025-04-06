use serde::{ser::SerializeStruct, Deserialize, Serialize};

type ImageRgb8 = kornia::image::Image<u8, 3>;
type ImageGray8 = kornia::image::Image<u8, 1>;

#[derive(Clone)]
pub struct ImageRgb8Msg(pub ImageRgb8);

impl std::ops::Deref for ImageRgb8Msg {
    type Target = ImageRgb8;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// TODO: implement in kornia-image
impl std::fmt::Debug for ImageRgb8Msg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ImageRgb8Msg(size: {:?})", self.0.size())
    }
}

// TODO: implement Image::empty()
impl Default for ImageRgb8Msg {
    fn default() -> Self {
        Self(ImageRgb8::new([0, 0].into(), vec![]).unwrap())
    }
}

// TODO: implement in kornia-image
impl bincode::enc::Encode for ImageRgb8Msg {
    fn encode<E: bincode::enc::Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), bincode::error::EncodeError> {
        bincode::Encode::encode(&self.rows(), encoder)?;
        bincode::Encode::encode(&self.cols(), encoder)?;
        bincode::Encode::encode(&self.as_slice(), encoder)?;
        Ok(())
    }
}

// TODO: implement in kornia-image
impl<C> bincode::de::Decode<C> for ImageRgb8Msg {
    fn decode<D: bincode::de::Decoder<Context = C>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        let rows = bincode::Decode::decode(decoder)?;
        let cols = bincode::Decode::decode(decoder)?;
        let data = bincode::Decode::decode(decoder)?;
        let image = ImageRgb8::new([rows, cols].into(), data)
            .map_err(|e| bincode::error::DecodeError::OtherString(e.to_string()))?;
        Ok(Self(image))
    }
}

impl Serialize for ImageRgb8Msg {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut s = serializer.serialize_struct("ImageRgb8Msg", 3)?;
        s.serialize_field("rows", &self.rows())?;
        s.serialize_field("cols", &self.cols())?;
        s.serialize_field("data", &self.as_slice())?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for ImageRgb8Msg {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct ImageData {
            rows: usize,
            cols: usize,
            data: Vec<u8>,
        }

        let data = ImageData::deserialize(deserializer)?;
        Ok(Self(
            ImageRgb8::new([data.rows, data.cols].into(), data.data)
                .map_err(serde::de::Error::custom)?,
        ))
    }
}

#[derive(Clone)]
pub struct ImageGray8Msg(pub ImageGray8);

impl std::ops::Deref for ImageGray8Msg {
    type Target = ImageGray8;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Debug for ImageGray8Msg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ImageGray8Msg(size: {:?})", self.0.size())
    }
}

impl Default for ImageGray8Msg {
    fn default() -> Self {
        Self(ImageGray8::new([0, 0].into(), vec![]).unwrap())
    }
}

impl bincode::enc::Encode for ImageGray8Msg {
    fn encode<E: bincode::enc::Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), bincode::error::EncodeError> {
        bincode::Encode::encode(&self.rows(), encoder)?;
        bincode::Encode::encode(&self.cols(), encoder)?;
        bincode::Encode::encode(&self.as_slice(), encoder)?;
        Ok(())
    }
}

impl<C> bincode::de::Decode<C> for ImageGray8Msg {
    fn decode<D: bincode::de::Decoder<Context = C>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        let rows = bincode::Decode::decode(decoder)?;
        let cols = bincode::Decode::decode(decoder)?;
        let data = bincode::Decode::decode(decoder)?;
        let image = ImageGray8::new([rows, cols].into(), data)
            .map_err(|e| bincode::error::DecodeError::OtherString(e.to_string()))?;
        Ok(Self(image))
    }
}

#[derive(Clone, Debug, Serialize, bincode::Encode, bincode::Decode)]
pub struct EncodedImage {
    pub data: Vec<u8>,
    pub encoding: String,
}

impl Default for EncodedImage {
    fn default() -> Self {
        Self {
            data: vec![],
            encoding: "".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, bincode::Encode, bincode::Decode)]
pub struct PromptResponseMsg {
    pub prompt: String,
    pub response: String,
}

impl Default for PromptResponseMsg {
    fn default() -> Self {
        Self {
            prompt: "".to_string(),
            response: "".to_string(),
        }
    }
}
