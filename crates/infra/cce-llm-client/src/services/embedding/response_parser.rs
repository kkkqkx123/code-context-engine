//! Response parsing types for embedder

use serde::{Deserialize, Serialize};

/// Token usage information
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TokenUsage {
    /// Prompt tokens used
    pub prompt_tokens: u64,
    /// Total tokens used
    pub total_tokens: u64,
}

/// Standard embedding response data
#[derive(Debug, Clone, Deserialize)]
pub struct StandardEmbeddingData {
    /// Embedding vector
    #[serde(deserialize_with = "deserialize_embedding_value")]
    pub embedding: Vec<f32>,
    /// Index in the input batch
    pub index: usize,
}

/// Deserialize embedding value that can be either a float array or base64 string
fn deserialize_embedding_value<'de, D>(deserializer: D) -> Result<Vec<f32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    use std::fmt;

    struct EmbeddingVisitor;

    impl<'de> Visitor<'de> for EmbeddingVisitor {
        type Value = Vec<f32>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a float array or base64 string")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            decode_base64_embedding(value)
                .map_err(|_| E::custom("failed to decode base64 embedding"))
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let mut vec = Vec::new();
            while let Some(value) = seq.next_element()? {
                vec.push(value);
            }
            Ok(vec)
        }
    }

    deserializer.deserialize_any(EmbeddingVisitor)
}

/// Standard OpenAI-compatible embedding response
#[derive(Debug, Clone, Deserialize)]
pub struct StandardEmbeddingResponse {
    pub data: Vec<StandardEmbeddingData>,
    #[serde(default)]
    pub usage: Option<TokenUsage>,
}

use crate::core::LlmError;

/// Decode base64 embedding to float vector
fn decode_base64_embedding(input: &str) -> Result<Vec<f32>, LlmError> {
    let bytes = decode_base64(input)?;

    if bytes.len() % std::mem::size_of::<f32>() != 0 {
        return Err(LlmError::invalid_response(format!(
            "Decoded embedding byte length {} is not divisible by 4",
            bytes.len()
        )));
    }

    let count = bytes.len() / 4;
    let mut result = Vec::with_capacity(count);

    for chunk in bytes.chunks_exact(4) {
        let arr: [u8; 4] = chunk
            .try_into()
            .map_err(|_| LlmError::invalid_response("Invalid base64 chunk".to_string()))?;
        result.push(f32::from_le_bytes(arr));
    }

    Ok(result)
}

/// Decode base64 string to bytes
fn decode_base64(input: &str) -> Result<Vec<u8>, LlmError> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut decode_table = [0xFFu8; 256];
    for (i, &c) in ALPHABET.iter().enumerate() {
        decode_table[c as usize] = i as u8;
    }

    let input_bytes = input.as_bytes();
    let padding = input_bytes.iter().rev().take_while(|&&c| c == b'=').count();

    if input_bytes.len() % 4 != 0 {
        return Err(LlmError::invalid_response(
            "Base64 input length must be divisible by 4",
        ));
    }
    if padding > 2 || input_bytes[..input_bytes.len().saturating_sub(padding)].contains(&b'=') {
        return Err(LlmError::invalid_response("Invalid base64 padding"));
    }

    let output_len = input_bytes
        .len()
        .checked_mul(3)
        .and_then(|length| length.checked_div(4))
        .and_then(|length| length.checked_sub(padding))
        .ok_or_else(|| LlmError::invalid_response("Invalid base64 length"))?;
    let mut result = Vec::with_capacity(output_len);

    let mut buffer = 0u32;
    let mut bits = 0;

    for &c in input_bytes {
        if c == b'=' {
            break;
        }

        let val = decode_table[c as usize];
        if val == 0xFF {
            return Err(LlmError::invalid_response(format!(
                "Invalid base64 character: {}",
                c as char
            )));
        }

        buffer = (buffer << 6) | (val as u32);
        bits += 6;

        if bits >= 8 {
            bits -= 8;
            result.push((buffer >> bits) as u8);
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_padding_without_panicking() {
        let result = decode_base64_embedding("====");
        assert!(result.is_err());
    }

    #[test]
    fn rejects_decoded_length_that_is_not_a_float_array() {
        let result = decode_base64_embedding("YQ==");
        assert!(result.is_err());
    }

    #[test]
    fn decodes_little_endian_float_array() {
        let result =
            decode_base64_embedding("AACAPwAAAMA=").expect("valid float embedding should decode");
        assert_eq!(result, vec![1.0, -2.0]);
    }
}
