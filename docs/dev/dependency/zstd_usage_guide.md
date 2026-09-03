# zstd Usage Guide

## Overview

zstd (Zstandard) is a fast lossless compression algorithm. This guide covers usage for the GraphDB project.

## Installation

```toml
[dependencies]
zstd = "0.13"
```

## Core API

### Convenience Functions

```rust
use zstd::{encode_all, decode_all};

// Compress data
let data = b"Hello, World! This is some data to compress.";
let compressed = encode_all(&data[..], 3)?;  // level 3

// Decompress data
let decompressed = decode_all(&compressed[..])?;
assert_eq!(data.to_vec(), decompressed);
```

### Compression Levels

```rust
use zstd::compression_level_range;

// Get valid compression level range
let (min, max) = compression_level_range();
println!("Valid levels: {} to {}", min, max);  // Typically 1 to 22

// Level 0: Default (usually level 3)
// Level 1: Fastest, lowest compression
// Level 3: Good balance (default)
// Level 9-19: Higher compression, slower
// Level 20-22: Maximum compression, slowest
```

| Level | Speed | Ratio | Use Case |
|-------|-------|-------|----------|
| 1 | Fastest | Low | Real-time streaming |
| 3 | Fast | Good | Default, general purpose |
| 6-9 | Medium | Better | Balanced workloads |
| 19 | Slow | High | Archival storage |
| 22 | Slowest | Highest | Maximum compression |

## Streaming API

### Encoder (Compression)

```rust
use zstd::stream::Encoder;
use std::io::Write;

// Create encoder with compression level
let mut encoder = Encoder::new(Vec::new(), 3)?;

// Write data
encoder.write_all(b"Hello, ")?;
encoder.write_all(b"World!")?;

// Finish encoding
let compressed = encoder.finish()?;
```

### Decoder (Decompression)

```rust
use zstd::stream::Decoder;
use std::io::Read;

// Create decoder
let mut decoder = Decoder::new(&compressed[..])?;

// Read decompressed data
let mut decompressed = Vec::new();
decoder.read_to_end(&mut decompressed)?;
```

### Copy Encode/Decode

```rust
use zstd::stream::{copy_encode, copy_decode};
use std::io;

// Compress from reader to writer
copy_encode(io::stdin(), io::stdout(), 3)?;

// Decompress from reader to writer
copy_decode(io::stdin(), io::stdout())?;
```

## Bulk Operations

```rust
use zstd::bulk::{compress, decompress};

// Compress with specific level
let compressed = compress(data, 3)?;

// Decompress (level not needed)
let decompressed = decompress(&compressed, Some(original_size))?;
```

## Dictionary Compression

For small data, dictionary compression provides better ratios:

```rust
use zstd::dict::EncoderDictionary;
use zstd::stream::Encoder;

// Train dictionary (typically done offline)
let dictionary = EncoderDictionary::copy(&dict_data, 3);

// Create encoder with dictionary
let mut encoder = Encoder::with_dictionary(Vec::new(), 3, &dictionary)?;
encoder.write_all(small_data)?;
let compressed = encoder.finish()?;
```

## Advanced Options

### Encoder with Options

```rust
use zstd::stream::Encoder;
use zstd::DEFAULT_COMPRESSION_LEVEL;

let mut encoder = Encoder::new(Vec::new(), DEFAULT_COMPRESSION_LEVEL)?;

// Set pledged source size (improves compression)
encoder.set_pledged_src_size(Some(data_len as u64))?;

// Write data
encoder.write_all(data)?;
let compressed = encoder.finish()?;
```

### Multi-Threaded Compression

```rust
use zstd::stream::write::Encoder;

// Enable multi-threading (requires zstd-safe feature)
let mut encoder = Encoder::new(Vec::new(), 3)?;
encoder.multithread(num_cpus::get() as u32)?;
```

## Integration with rkyv

### Combined Serialization + Compression

```rust
use rkyv::{to_bytes, rancor::Error};
use zstd::{encode_all, decode_all};

pub fn serialize_and_compress<T>(value: &T, level: i32) -> Result<Vec<u8>, Box<dyn std::error::Error>>
where
    T: rkyv::Serialize<rkyv::ser::DefaultSerializer>,
{
    // Serialize with rkyv
    let serialized = to_bytes::<Error>(value)?;
    
    // Compress with zstd
    let compressed = encode_all(&serialized[..], level)?;
    
    Ok(compressed)
}

pub fn decompress_and_deserialize<T>(compressed: &[u8]) -> Result<T, Box<dyn std::error::Error>>
where
    T: rkyv::Archive,
    T::Archived: rkyv::Deserialize<T, rkyv::de::DefaultDeserializer>,
{
    // Decompress
    let decompressed = decode_all(compressed)?;
    
    // Deserialize with rkyv
    let archived = rkyv::access::<T::Archived, rkyv::rancor::Error>(&decompressed)?;
    let value: T = rkyv::deserialize(archived)?;
    
    Ok(value)
}
```

### GraphDB Serialization Helper

```rust
use rkyv::{Archive, Serialize, Deserialize, to_bytes, rancor::Error};
use zstd::{encode_all, decode_all};

pub struct Serializer;

impl Serializer {
    /// Default compression level for GraphDB
    pub const DEFAULT_LEVEL: i32 = 3;
    
    /// Serialize and compress
    pub fn serialize<T>(value: &T) -> Result<Vec<u8>, SerializationError>
    where
        T: Serialize<rkyv::ser::DefaultSerializer>,
    {
        let bytes = to_bytes::<Error>(value)
            .map_err(|e| SerializationError::Serialize(e.to_string()))?;
        
        let compressed = encode_all(&bytes[..], Self::DEFAULT_LEVEL)
            .map_err(|e| SerializationError::Compress(e.to_string()))?;
        
        Ok(compressed)
    }
    
    /// Decompress and deserialize
    pub fn deserialize<T>(data: &[u8]) -> Result<T, SerializationError>
    where
        T: Archive,
        T::Archived: Deserialize<T, rkyv::de::DefaultDeserializer>,
    {
        let decompressed = decode_all(data)
            .map_err(|e| SerializationError::Decompress(e.to_string()))?;
        
        let archived = rkyv::access::<T::Archived, Error>(&decompressed)
            .map_err(|e| SerializationError::Validate(e.to_string()))?;
        
        let value = rkyv::deserialize(archived)
            .map_err(|e| SerializationError::Deserialize(e.to_string()))?;
        
        Ok(value)
    }
    
    /// Zero-copy access (decompresses but doesn't deserialize)
    pub fn access<T>(data: &[u8]) -> Result<T::Archived, SerializationError>
    where
        T: Archive,
    {
        let decompressed = decode_all(data)
            .map_err(|e| SerializationError::Decompress(e.to_string()))?;
        
        let archived = rkyv::access::<T::Archived, Error>(&decompressed)
            .map_err(|e| SerializationError::Validate(e.to_string()))?;
        
        Ok(archived)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SerializationError {
    #[error("Serialization failed: {0}")]
    Serialize(String),
    #[error("Compression failed: {0}")]
    Compress(String),
    #[error("Decompression failed: {0}")]
    Decompress(String),
    #[error("Validation failed: {0}")]
    Validate(String),
    #[error("Deserialization failed: {0}")]
    Deserialize(String),
}
```

## Error Handling

```rust
use zstd::stream::{Encoder, Decoder};

fn compress_data(data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // Encoder creation can fail
    let mut encoder = Encoder::new(Vec::new(), 3)?;
    
    // Write can fail
    std::io::copy(&mut &data[..], &mut encoder)?;
    
    // Finish can fail
    let compressed = encoder.finish()?;
    
    Ok(compressed)
}

fn decompress_data(compressed: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // Decoder creation can fail
    let mut decoder = Decoder::new(compressed)?;
    
    // Read can fail
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed)?;
    
    Ok(decompressed)
}
```

## Performance Tips

### 1. Choose Appropriate Level

```rust
// For real-time: level 1
let compressed = encode_all(data, 1)?;

// For storage: level 3-6
let compressed = encode_all(data, 3)?;

// For archival: level 19+
let compressed = encode_all(data, 19)?;
```

### 2. Reuse Encoders/Decoders

```rust
use zstd::stream::Encoder;

// Create once, reuse multiple times
let mut encoder = Encoder::new(Vec::new(), 3)?;

for data in &data_chunks {
    encoder.write_all(data)?;
    let compressed = encoder.finish()?;
    encoder = Encoder::new(Vec::new(), 3)?;
}
```

### 3. Use Pledged Source Size

```rust
use zstd::stream::Encoder;

let mut encoder = Encoder::new(Vec::new(), 3)?;
encoder.set_pledged_src_size(Some(data.len() as u64))?;
encoder.write_all(data)?;
let compressed = encoder.finish()?;
```

### 4. Dictionary for Small Data

```rust
// Train dictionary once
let dict = zstd::dict::from_continuous(
    &training_data,
    &sample_sizes,
    dict_size,
)?;

// Use for compression
let encoder = Encoder::with_dictionary(
    Vec::new(),
    3,
    &EncoderDictionary::copy(&dict, 3),
)?;
```

## Common Patterns

### File Compression

```rust
use std::fs::File;
use std::io::{self, copy};
use zstd::stream::{Encoder, Decoder};

fn compress_file(input: &str, output: &str, level: i32) -> io::Result<()> {
    let input_file = File::open(input)?;
    let output_file = File::create(output)?;
    
    let mut encoder = Encoder::new(output_file, level)?;
    copy(&mut input_file.open()?, &mut encoder)?;
    encoder.finish()?;
    
    Ok(())
}

fn decompress_file(input: &str, output: &str) -> io::Result<()> {
    let input_file = File::open(input)?;
    let output_file = File::create(output)?;
    
    let mut decoder = Decoder::new(input_file)?;
    copy(&mut decoder, &mut output_file)?;
    
    Ok(())
}
```

### Buffer Compression

```rust
use zstd::{encode_all, decode_all};

fn compress_buffer(data: &[u8]) -> Vec<u8> {
    encode_all(data, 3).expect("Compression failed")
}

fn decompress_buffer(data: &[u8]) -> Vec<u8> {
    decode_all(data).expect("Decompression failed")
}
```

### Streaming with Custom Buffer Size

```rust
use zstd::stream::Encoder;
use std::io::Write;

fn compress_with_buffer(data: &[u8], buf_size: usize) -> Vec<u8> {
    let mut encoder = Encoder::new(Vec::new(), 3).unwrap();
    
    for chunk in data.chunks(buf_size) {
        encoder.write_all(chunk).unwrap();
    }
    
    encoder.finish().unwrap()
}
```

## Feature Flags

```toml
[dependencies]
zstd = { version = "0.13", features = ["zstdmt"] }  # Multi-threading
```

Available features:
- `zstdmt` - Multi-threaded compression support
- `bindgen` - Generate bindings at build time
- `experimental` - Enable experimental APIs
- `pkg-config` - Use pkg-config to find zstd

## Best Practices

1. **Use level 3 as default** - Good balance of speed and compression
2. **Handle errors properly** - All operations can fail
3. **Set pledged source size** when known - Improves compression ratio
4. **Use streaming API** for large data - Avoids loading everything into memory
5. **Consider dictionary** for small repetitive data - Better compression ratios
6. **Reuse encoders** when possible - Reduces allocation overhead

## Troubleshooting

### "Destination buffer too small"
Increase output buffer size or use streaming API.

### "Data corruption detected"
Data is corrupted or not valid zstd format.

### "Unknown frame descriptor"
Not a valid zstd compressed stream.

### Slow compression
- Use lower compression level (1-3)
- Enable multi-threading with `zstdmt` feature
- Use dictionary for small data
