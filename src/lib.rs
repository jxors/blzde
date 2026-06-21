#![warn(missing_docs)]

//! `fxde` is a **self-describing** serialization format for `serde` that aims for **fast deserialization** of big files, at the cost of slower serialization.
//! Serializing takes two steps: first, a schema format specific to the provided data is computed.
//! Then, the data is serialized according to this format.
//! This allows the actual data to be serialized without any identifiers, type markers, or other non-data.
//! 
//! To serialize data, use [`to_vec`] or [`into_writer`].
//! To deserialize, use [`from_slice`] or [`from_reader`].
//! 
//! There are currently no other configuration options.
//! ```

use crate::{
    de::{Deserializer, State}, schema::{Schema, formats::{FormatStorage, SchemaFormats}}, ser::Serializer,
};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read, Write};

mod de;
mod schema;
mod ser;

pub use de::DeserializeError;
pub use ser::SerializeError;

#[cfg(test)]
mod tests;

const MAGIC_NUMBER: u32 = 0x011B_115au32;

/// Serializes `value` into `writer`.
/// 
/// Writes are not buffered.
/// You should wrap `writer` in a [`std::io::BufWriter`] if this is desired.
pub fn into_writer<T: Serialize>(value: &T, mut writer: impl Write) -> Result<(), SerializeError> {
    let schema = Schema::of(&value);
    let mut storage = schema.make_format_storage();
    let format = schema.to_format(storage.create_bump_alloc());

    writer.write_all(&MAGIC_NUMBER.to_le_bytes())?;
    format.write_into(&mut writer)?;

    let serializer = Serializer::new(&format, &mut writer);
    value.serialize(serializer)?;

    Ok(())
}

/// Deserializes a `T` from `reader`.
/// 
/// Reads are not buffered.
/// You should wrap `reader` in a [`std::io::BufReader`] if this is desired.
/// 
/// The reader is not guaranteed to be placed exactly at the end of the serialized data
/// when this function terminates.
/// When provided with a stream longer than the original serialized data,
/// the deserializer may read past the end of the data.
pub fn from_reader<'de, T: Deserialize<'de>>(mut reader: impl Read) -> Result<T, DeserializeError> {
    let mut magic = [0; 4];
    reader.read_exact(&mut magic)?;
    if u32::from_le_bytes(magic) != MAGIC_NUMBER {
        return Err(DeserializeError::Custom(String::from("invalid magic number")));
    }

    let storage = FormatStorage::read_from(&mut reader)?;
    let format = SchemaFormats::read_from(&storage)?;
    if format.is_empty() {
        return Err(DeserializeError::EmptyFormat);
    }

    let mut state = State::new(&format, &mut reader);
    let deserializer = Deserializer::new(&mut state);
    T::deserialize(deserializer)
}

/// Serializes `value` into a [`Vec`].
pub fn to_vec<T: Serialize>(value: &T) -> Vec<u8> {
    let mut output = Vec::new();
    into_writer(value, Cursor::new(&mut output)).unwrap();
    output
}

/// Deserializes a `T` from `slice`.
/// 
/// Extra bytes are silently ignored.
pub fn from_slice<'de, T: Deserialize<'de>>(slice: &[u8]) -> Result<T, DeserializeError> {
    from_reader(Cursor::new(slice))
}
