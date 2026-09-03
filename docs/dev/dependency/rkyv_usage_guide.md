# rkyv Usage Guide

## Overview

rkyv is a zero-copy deserialization framework for Rust. This guide covers the essential usage patterns for GraphDB's migration from bincode.

## Installation

```toml
[dependencies]
rkyv = { version = "0.8", features = ["validation", "strict"] }
```

## Core Concepts

### Three Core Traits

| Trait | Purpose | Derive |
|-------|---------|--------|
| `Archive` | Defines archived representation | `#[derive(Archive)]` |
| `Serialize` | Converts native to archived | `#[derive(Serialize)]` |
| `Deserialize` | Converts archived to native | `#[derive(Deserialize)]` |

## Basic Usage

### Simple Struct

```rust
use rkyv::{Archive, Deserialize, Serialize, rancor::Error};

#[derive(Archive, Deserialize, Serialize, Debug, PartialEq)]
#[rkyv(
    derive(Debug),           // Pass derives to archived type
    compare(PartialEq),      // Enable PartialEq between native/archived
)]
struct User {
    id: u64,
    name: String,
    email: String,
}

fn main() {
    let user = User {
        id: 1,
        name: "Alice".to_string(),
        email: "alice@example.com".to_string(),
    };

    // Serialize to bytes
    let bytes = rkyv::to_bytes::<Error>(&user).unwrap();

    // Zero-copy access (with validation)
    let archived = rkyv::access::<ArchivedUser, Error>(&bytes).unwrap();
    println!("Name: {}", archived.name);  // No deserialization!

    // Full deserialization
    let restored: User = rkyv::deserialize(archived).unwrap();
    assert_eq!(restored, user);
}
```

### One-Step Deserialization

```rust
use rkyv::{from_bytes, to_bytes, rancor::Error};

// Serialize
let bytes = to_bytes::<Error>(&user).unwrap();

// Deserialize with validation in one step
let restored: User = from_bytes::<User, Error>(&bytes).unwrap();
```

## Supported Types

### Native Support (No Extra Work)

```rust
// Primitives
bool, char, u8, u16, u32, u64, u128, i8, i16, i32, i64, i128, f32, f64, usize, isize

// String types
String, str

// Collections (when element type implements Archive)
Vec<T>, HashMap<K, V>, HashSet<T>, Option<T>, Result<T, E>, Box<T>

// Arrays
[T; N] for any N

// Tuples
(A,), (A, B), (A, B, C), etc.
```

### Example with Collections

```rust
#[derive(Archive, Serialize, Deserialize)]
#[rkyv(derive(Debug))]
struct Database {
    users: HashMap<String, User>,
    tags: Vec<String>,
    config: Option<Config>,
}

// Zero-copy HashMap access
let bytes = to_bytes::<Error>(&db).unwrap();
let archived = access::<ArchivedDatabase, Error>(&bytes).unwrap();

// Direct lookup without deserialization
if let Some(user) = archived.users.get("alice") {
    println!("Found: {}", user.name);
}
```

## Advanced Patterns

### Recursive Types

Recursive types (like JSON) require special bounds:

```rust
use rkyv::{Archive, Deserialize, Serialize};

#[derive(Archive, Debug, Deserialize, Serialize)]
#[rkyv(
    serialize_bounds(
        __S: rkyv::ser::Writer + rkyv::ser::Allocator,
        __S::Error: rkyv::rancor::Source,
    ),
    deserialize_bounds(__D::Error: rkyv::rancor::Source),
    bytecheck(bounds(__C: rkyv::validation::ArchiveContext)),
)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(#[rkyv(omit_bounds)] Vec<Value>),  // Recursive - needs omit_bounds
    Object(#[rkyv(omit_bounds)] HashMap<String, Value>),
}
```

Key attributes for recursive types:
- `#[rkyv(omit_bounds)]` - Don't generate bounds for recursive fields
- `serialize_bounds` - Required serializer capabilities
- `deserialize_bounds` - Required deserializer capabilities
- `bytecheck(bounds(...))` - Validation context requirements

### External Types (Remote Derive)

For types from external crates that don't implement rkyv traits:

```rust
use rkyv::{Archive, Deserialize, Serialize};

// External type we can't modify
mod external {
    #[derive(Debug)]
    pub struct Point {
        pub x: f32,
        pub y: f32,
    }
}

// Define wrapper with same structure
#[derive(Archive, Serialize, Deserialize)]
#[rkyv(remote = external::Point)]  // Target external type
#[rkyv(archived = ArchivedPoint)]  // Custom archived type name
struct PointDef {
    x: f32,
    y: f32,
}

// Required: From implementation for deserialization
impl From<PointDef> for external::Point {
    fn from(def: PointDef) -> Self {
        external::Point { x: def.x, y: def.y }
    }
}

// Use in your types
#[derive(Archive, Serialize, Deserialize)]
struct Shape {
    #[rkyv(with = PointDef)]  // Use wrapper
    center: external::Point,
    radius: f32,
}
```

### Custom Field Serialization

Use `#[rkyv(with = ...)]` for custom field handling:

```rust
use rkyv::{Archive, Serialize, Deserialize};
use rkyv::with::AsBox;

#[derive(Archive, Serialize, Deserialize)]
struct LargeData {
    // Store out-of-line (boxed in archive)
    #[rkyv(with = AsBox)]
    data: Vec<u8>,
    
    // Inline storage (default)
    metadata: String,
}
```

### Custom Wrapper Example

```rust
use rkyv::{
    with::{ArchiveWith, DeserializeWith, SerializeWith},
    Archived, Place, Resolver,
    rancor::Fallible,
};

// Wrapper that increments value during serialization
struct Incremented;

impl ArchiveWith<i32> for Incremented {
    type Archived = Archived<i32>;
    type Resolver = Resolver<i32>;

    fn resolve_with(field: &i32, _: (), out: Place<Self::Archived>) {
        (field + 1).resolve((), out);
    }
}

impl<S: Fallible + ?Sized> SerializeWith<i32, S> for Incremented
where
    i32: rkyv::Serialize<S>,
{
    fn serialize_with(field: &i32, s: &mut S) -> Result<Self::Resolver, S::Error> {
        (field + 1).serialize(s)
    }
}

impl<D: Fallible + ?Sized> DeserializeWith<Archived<i32>, i32, D> for Incremented
where
    Archived<i32>: rkyv::Deserialize<i32, D>,
{
    fn deserialize_with(field: &Archived<i32>, d: &mut D) -> Result<i32, D::Error> {
        Ok(field.deserialize(d)? - 1)
    }
}

#[derive(Archive, Serialize, Deserialize)]
struct Example {
    #[rkyv(with = Incremented)]
    value: i32,  // Will be value + 1 in archive
}
```

## Derive Macro Attributes Reference

### Top-Level Attributes

```rust
#[derive(Archive, Serialize, Deserialize)]
#[rkyv(
    // Pass derives to archived type
    derive(Debug, Clone, PartialEq),
    
    // Generate comparison impls
    compare(PartialEq, PartialOrd),
    
    // Custom archived type name
    archived = MyArchivedType,
    
    // Serializer bounds (for recursive types)
    serialize_bounds(
        __S: rkyv::ser::Writer + rkyv::ser::Allocator,
        __S::Error: rkyv::rancor::Source,
    ),
    
    // Deserializer bounds
    deserialize_bounds(__D::Error: rkyv::rancor::Source),
    
    // Validation bounds
    bytecheck(bounds(__C: rkyv::validation::ArchiveContext)),
)]
struct MyStruct { ... }
```

### Field-Level Attributes

```rust
#[derive(Archive, Serialize, Deserialize)]
struct Example {
    // Use custom wrapper
    #[rkyv(with = MyWrapper)]
    field1: ExternalType,
    
    // Store out-of-line
    #[rkyv(with = AsBox)]
    field2: LargeType,
    
    // Skip bounds generation (for recursive types)
    #[rkyv(omit_bounds)]
    field3: RecursiveType,
    
    // Use getter for remote type
    #[rkyv(getter = Type::get_field)]
    field4: Type,
}
```

## Zero-Copy Access Patterns

### Safe Access with Validation

```rust
use rkyv::{access, to_bytes, rancor::Error};

let bytes = to_bytes::<Error>(&data)?;
let archived = access::<ArchivedData, Error>(&bytes)?;
// Validation ensures data integrity
```

### Unsafe Access (Faster, No Validation)

```rust
use rkyv::access_unchecked;

let archived = unsafe { access_unchecked::<ArchivedData>(&bytes) };
// Fast but assumes data is valid
```

### Iterating Collections

```rust
let db: ArchivedDatabase = ...;

// Iterate without allocation
for (key, user) in db.users.iter() {
    println!("{}: {}", key, user.name);
}

// Get by key (HashMap)
if let Some(user) = db.users.get("alice") {
    println!("Found user");
}

// Index into Vec
let first = &db.tags[0];
```

## Error Handling

```rust
use rkyv::rancor::Error;

fn process_data<T>(bytes: &[u8]) -> Result<T, String>
where
    T: Archive,
    T::Archived: Deserialize<T, rkyv::de::DefaultDeserializer>,
{
    // Validation error
    let archived = rkyv::access::<T::Archived, Error>(bytes)
        .map_err(|e| format!("Validation failed: {}", e))?;
    
    // Deserialization error
    let data: T = rkyv::deserialize(archived)
        .map_err(|e| format!("Deserialization failed: {}", e))?;
    
    Ok(data)
}
```

## GraphDB-Specific Patterns

### Value Enum (Recursive)

```rust
#[derive(Archive, Serialize, Deserialize)]
#[rkyv(
    derive(Debug, Clone),
    serialize_bounds(
        __S: rkyv::ser::Writer + rkyv::ser::Allocator,
        __S::Error: rkyv::rancor::Source,
    ),
    deserialize_bounds(__D::Error: rkyv::rancor::Source),
    bytecheck(bounds(__C: rkyv::validation::ArchiveContext)),
)]
pub enum Value {
    Empty,
    Null(NullType),
    Bool(bool),
    Int(i64),
    String(String),
    List(#[rkyv(omit_bounds)] List),
    Map(#[rkyv(omit_bounds)] HashMap<String, Value>),
    Vertex(#[rkyv(omit_bounds)] Box<Vertex>),
    // ... other variants
}
```

### Vertex with Box<Value>

```rust
#[derive(Archive, Serialize, Deserialize)]
#[rkyv(
    derive(Debug, Clone),
    serialize_bounds(
        __S: rkyv::ser::Writer + rkyv::ser::Allocator,
        __S::Error: rkyv::rancor::Source,
    ),
    deserialize_bounds(__D::Error: rkyv::rancor::Source),
    bytecheck(bounds(__C: rkyv::validation::ArchiveContext)),
)]
pub struct Vertex {
    #[rkyv(omit_bounds)]
    pub vid: Box<Value>,
    pub id: i64,
    pub tags: Vec<Tag>,
    #[rkyv(omit_bounds)]
    pub properties: HashMap<String, Value>,
}
```

### External Type: Decimal128Value

```rust
use rkyv::{Archive, Serialize, Deserialize};
use dec::Decimal128;

// String-based representation for external type
#[derive(Archive, Serialize, Deserialize)]
#[rkyv(remote = Decimal128Value)]
#[rkyv(archived = ArchivedDecimal128Value)]
struct Decimal128Def {
    value: String,
}

impl From<Decimal128Def> for Decimal128Value {
    fn from(def: Decimal128Def) -> Self {
        def.value.parse().expect("Invalid decimal")
    }
}

impl From<&Decimal128Value> for Decimal128Def {
    fn from(v: &Decimal128Value) -> Self {
        Self { value: v.to_string() }
    }
}

// In Value enum:
// Decimal128(#[rkyv(with = Decimal128Def)] Decimal128Value),
```

## Best Practices

1. **Always use validation** for untrusted data: `access::<T, Error>()`
2. **Use `#[rkyv(derive(Debug))]`** for easier debugging of archived types
3. **Add `compare(PartialEq)`** when you need to compare native and archived values
4. **Use `omit_bounds`** for recursive types to avoid circular trait bounds
5. **Consider `AsBox`** for large fields to reduce archive size
6. **Use zero-copy access** for read-heavy operations to avoid allocation

## Common Pitfalls

1. **Forgetting bounds for recursive types** - Always add `serialize_bounds`, `deserialize_bounds`, and `bytecheck(bounds(...))`
2. **Not implementing `From`** for remote derive - Required for deserialization
3. **Using wrong error type** - Use `rkyv::rancor::Error` or your own `Source` type
4. **Accessing archived data after buffer drop** - Archived data references the buffer
5. **Confusing `ArchivedX` with `X`** - They are different types with different methods
