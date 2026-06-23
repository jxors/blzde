use crate::schema::formats::{Format, FormatId, NamedFormat, Primitive, SchemaFormats};
use serde::ser::{self, SerializeTuple, SerializeTupleVariant};
use std::{error::Error, fmt::Display, io::Write};

#[derive(Debug)]
pub enum SerializeError {
    UnexpectedFormat { expected: &'static str, found: Format<'static> },
    StructFieldDifference { expected: String, found: &'static str },
    Custom(String),
    Io(std::io::Error),
    FormatTooBig(u32),
}

impl Display for SerializeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SerializeError::UnexpectedFormat { expected, found } => write!(f, "unable to serialize {expected} into {found:?}"),
            SerializeError::StructFieldDifference { expected, found } => {
                write!(f, "tried serializing struct field {found} into {expected}")
            },
            SerializeError::Custom(s) => f.write_str(&s),
            SerializeError::Io(e) => write!(f, "i/o error: {e}"),
            SerializeError::FormatTooBig(bytes) => write!(f, "schema format is too big: {} MiB (maximum 128 MiB)", bytes >> 20),
        }
    }
}

impl Error for SerializeError {}

impl ser::Error for SerializeError {
    fn custom<T>(msg: T) -> Self
    where
        T: Display,
    {
        SerializeError::Custom(msg.to_string())
    }
}

impl From<std::io::Error> for SerializeError {
    fn from(value: std::io::Error) -> Self {
        SerializeError::Io(value)
    }
}

pub struct Serializer<'r, 'format, W> {
    buf: &'format SchemaFormats<'format>,
    format: Format<'format>,
    writer: &'r mut W,
}

impl<'r, 'format, W: Write> Serializer<'r, 'format, W> {
    pub fn new(buf: &'format SchemaFormats, writer: &'r mut W) -> Self {
        Self {
            buf,
            format: buf.root(),
            writer,
        }
    }

    fn lookup(self, format: FormatId) -> Self {
        Self {
            format: self.buf.format(format),
            buf: self.buf,
            writer: self.writer,
        }
    }

    fn write_primitive(&mut self, primitive: Primitive, v: u64) -> std::io::Result<()> {
        match primitive {
            Primitive::U8 => self.writer.write_all(&[v as u8])?,
            Primitive::U64 => self.writer.write_all(&v.to_le_bytes())?,
            Primitive::AdjustedU32(offset) => {
                let v = v.wrapping_sub(offset);
                debug_assert!(u32::try_from(v).is_ok());
                let v = v as u32;
                self.writer.write_all(&v.to_le_bytes())?
            },
        }

        Ok(())
    }
}

impl<'r, 'format, W: Write> ser::Serializer for Serializer<'r, 'format, W> {
    type Ok = ();
    type Error = SerializeError;
    type SerializeSeq = SeqSerializer<'r, 'format, W>;
    type SerializeTuple = TupleSerializer<'r, 'format, W>;
    type SerializeTupleStruct = TupleSerializer<'r, 'format, W>;
    type SerializeTupleVariant = TupleSerializer<'r, 'format, W>;
    type SerializeMap = MapSerializer<'r, 'format, W>;
    type SerializeStruct = StructSerializer<'r, 'format, W>;
    type SerializeStructVariant = StructSerializer<'r, 'format, W>;

    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
        self.serialize_u8(v as u8)
    }

    fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> {
        self.serialize_i64(v as i64)
    }

    fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> {
        self.serialize_i64(v as i64)
    }

    fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> {
        self.serialize_i64(v as i64)
    }

    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
        self.serialize_u64(v as u64)
    }

    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> {
        self.serialize_u64(v as u64)
    }

    fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> {
        self.serialize_u64(v as u64)
    }

    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> {
        self.serialize_u64(v as u64)
    }

    fn serialize_u64(mut self, v: u64) -> Result<Self::Ok, Self::Error> {
        let Format::Primitive(primitive) = self.format else {
            return Err(SerializeError::UnexpectedFormat {
                expected: "primitive",
                found: self.format.make_static(),
            });
        };
        self.write_primitive(primitive, v)?;
        Ok(())
    }

    fn serialize_u128(self, v: u128) -> Result<Self::Ok, Self::Error> {
        let Format::U128 = self.format else {
            return Err(SerializeError::UnexpectedFormat {
                expected: "u128",
                found: self.format.make_static(),
            });
        };
        self.writer.write_all(&v.to_le_bytes()).unwrap();
        Ok(())
    }

    fn serialize_i128(self, v: i128) -> Result<Self::Ok, Self::Error> {
        self.serialize_u128(v as u128)
    }

    fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> {
        self.serialize_u32(v.to_bits())
    }

    fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> {
        self.serialize_u64(v.to_bits())
    }

    fn serialize_char(self, v: char) -> Result<Self::Ok, Self::Error> {
        self.serialize_u32(v as u32)
    }

    fn serialize_str(mut self, v: &str) -> Result<Self::Ok, Self::Error> {
        let Format::String { len } = self.format else {
            return Err(SerializeError::UnexpectedFormat {
                expected: "string",
                found: self.format.make_static(),
            });
        };
        self.write_primitive(len, v.len() as u64)?;
        self.writer.write_all(v.as_bytes()).unwrap();
        Ok(())
    }

    fn serialize_bytes(mut self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        let Format::Bytes { len } = self.format else {
            return Err(SerializeError::UnexpectedFormat {
                expected: "bytes",
                found: self.format.make_static(),
            });
        };
        self.write_primitive(len, v.len() as u64)?;
        self.writer.write_all(v).unwrap();
        Ok(())
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        let Format::Option { .. } = self.format else {
            return Err(SerializeError::UnexpectedFormat {
                expected: "option (none)",
                found: self.format.make_static(),
            });
        };
        self.writer.write_all(&[0]).unwrap();
        Ok(())
    }

    fn serialize_some<T>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        let Format::Option { inner } = self.format else {
            return Err(SerializeError::UnexpectedFormat {
                expected: "option (some)",
                found: self.format.make_static(),
            });
        };
        self.writer.write_all(&[1]).unwrap();
        value.serialize(self.lookup(inner))?;
        Ok(())
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        let Format::Unit = self.format else {
            return Err(SerializeError::UnexpectedFormat {
                expected: "unit",
                found: self.format.make_static(),
            });
        };
        Ok(())
    }

    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok, Self::Error> {
        let s = self.serialize_tuple_struct(name, 0)?;
        ser::SerializeTupleStruct::end(s)?;
        Ok(())
    }

    fn serialize_unit_variant(
        mut self, _name: &'static str, _variant_index: u32, variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        let Format::Variants { variant_index, variants } = self.format else {
            return Err(SerializeError::UnexpectedFormat {
                expected: "unit variant",
                found: self.format.make_static(),
            });
        };
        self.write_primitive(
            variant_index,
            variants.items().iter().position(|item| self.buf[item.0] == *variant).unwrap() as u64,
        )?;
        Ok(())
    }

    fn serialize_newtype_struct<T>(self, name: &'static str, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        let mut s = self.serialize_tuple_struct(name, 1)?;
        s.serialize_element(value)?;
        ser::SerializeTupleStruct::end(s)?;
        Ok(())
    }

    fn serialize_newtype_variant<T>(
        self, name: &'static str, variant_index: u32, variant: &'static str, value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        let mut s = self.serialize_tuple_variant(name, variant_index, variant, 1)?;
        s.serialize_field(value)?;
        ser::SerializeTupleVariant::end(s)
    }

    fn serialize_seq(mut self, seq_len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        let Format::Sequence { len, inner } = self.format else {
            return Err(SerializeError::UnexpectedFormat {
                expected: "sequence",
                found: self.format.make_static(),
            });
        };
        self.write_primitive(len, seq_len.unwrap() as u64)?;
        Ok(SeqSerializer {
            serializer: self.lookup(inner),
        })
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        let Format::Tuple { fields } = self.format else {
            return Err(SerializeError::UnexpectedFormat {
                expected: "tuple",
                found: self.format.make_static(),
            });
        };
        Ok(TupleSerializer {
            fields: fields.items(),
            buf: self.buf,
            writer: &mut *self.writer,
        })
    }

    fn serialize_tuple_struct(self, _name: &'static str, len: usize) -> Result<Self::SerializeTupleStruct, Self::Error> {
        self.serialize_tuple(len)
    }

    fn serialize_tuple_variant(
        mut self, _name: &'static str, _variant_index: u32, variant: &'static str, _len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        let Format::Variants { variant_index, variants } = self.format else {
            return Err(SerializeError::UnexpectedFormat {
                expected: "variants",
                found: self.format.make_static(),
            });
        };
        let Some(index) = variants.items().iter().position(|item| self.buf[item.0] == *variant) else {
            panic!("Cannot find variant {variant} in {variants:?}")
        };
        self.write_primitive(variant_index, index as u64)?;

        let variant_format = self.buf.format(variants.items()[index].1);
        let Format::Tuple { fields } = variant_format else {
            return Err(SerializeError::UnexpectedFormat {
                expected: "tuple variant",
                found: variant_format.make_static(),
            });
        };
        Ok(TupleSerializer {
            fields: fields.items(),
            buf: self.buf,
            writer: &mut *self.writer,
        })
    }

    fn serialize_map(mut self, actual_len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        let Format::Map { len, key, value } = self.format else {
            return Err(SerializeError::UnexpectedFormat {
                expected: "map",
                found: self.format.make_static(),
            });
        };
        self.write_primitive(len, actual_len.unwrap() as u64)?;
        Ok(MapSerializer {
            key: self.buf.format(key),
            value: self.buf.format(value),
            buf: self.buf,
            writer: &mut *self.writer,
        })
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct, Self::Error> {
        let Format::Struct { fields } = self.format else {
            return Err(SerializeError::UnexpectedFormat {
                expected: "struct",
                found: self.format.make_static(),
            });
        };
        Ok(StructSerializer {
            fields: fields.items(),
            buf: self.buf,
            writer: &mut *self.writer,
        })
    }

    fn serialize_struct_variant(
        mut self, _name: &'static str, _variant_index: u32, variant: &'static str, _len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        let Format::Variants { variant_index, variants } = self.format else {
            return Err(SerializeError::UnexpectedFormat {
                expected: "variants",
                found: self.format.make_static(),
            });
        };
        let index = variants.items().iter().position(|item| self.buf[item.0] == *variant).unwrap();
        self.write_primitive(variant_index, index as u64)?;

        let variant_format = self.buf.format(variants.items()[index].1);
        let Format::Struct { fields } = variant_format else {
            return Err(SerializeError::UnexpectedFormat {
                expected: "struct variant",
                found: variant_format.make_static(),
            });
        };
        Ok(StructSerializer {
            fields: fields.items(),
            buf: self.buf,
            writer: &mut *self.writer,
        })
    }
}

pub struct SeqSerializer<'r, 'format, W> {
    serializer: Serializer<'r, 'format, W>,
}

impl<'r, 'format, W: Write> ser::SerializeSeq for SeqSerializer<'r, 'format, W> {
    type Ok = ();
    type Error = SerializeError;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        value.serialize(Serializer {
            buf: self.serializer.buf,
            format: self.serializer.format,
            writer: &mut *self.serializer.writer,
        })?;

        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

pub struct MapSerializer<'r, 'format, W> {
    key: Format<'format>,
    value: Format<'format>,
    buf: &'format SchemaFormats<'format>,
    writer: &'r mut W,
}

impl<'r, 'format, W: Write> ser::SerializeMap for MapSerializer<'r, 'format, W> {
    type Ok = ();
    type Error = SerializeError;

    fn serialize_key<T>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        key.serialize(Serializer {
            buf: self.buf,
            format: self.key,
            writer: &mut *self.writer,
        })
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        value.serialize(Serializer {
            buf: self.buf,
            format: self.value,
            writer: &mut *self.writer,
        })
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

pub struct StructSerializer<'r, 'format, W> {
    fields: &'format [NamedFormat],
    buf: &'format SchemaFormats<'format>,
    writer: &'r mut W,
}

impl<'r, 'format, W: Write> ser::SerializeStruct for StructSerializer<'r, 'format, W> {
    type Ok = ();
    type Error = SerializeError;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        let (&field, rest) = self.fields.split_first().unwrap();
        if self.buf[field.0] != *key {
            return Err(SerializeError::StructFieldDifference {
                expected: self.buf.get_symbol(field.0).to_string(),
                found: key,
            });
        }

        self.fields = rest;
        value.serialize(Serializer {
            buf: self.buf,
            format: self.buf.format(field.1),
            writer: &mut *self.writer,
        })
    }

    fn skip_field(&mut self, key: &'static str) -> Result<(), Self::Error> {
        let (field, rest) = self.fields.split_first().unwrap();
        if self.buf[field.0] != *key {
            return Err(SerializeError::StructFieldDifference {
                expected: self.buf.get_symbol(field.0).to_string(),
                found: key,
            });
        }

        self.fields = rest;
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'r, 'format, W: Write> ser::SerializeStructVariant for StructSerializer<'r, 'format, W> {
    type Ok = ();
    type Error = SerializeError;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        ser::SerializeStruct::serialize_field(self, key, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        ser::SerializeStruct::end(self)
    }
}

pub struct TupleSerializer<'r, 'format, W> {
    fields: &'format [FormatId],
    buf: &'format SchemaFormats<'format>,
    writer: &'r mut W,
}

impl<'r, 'format, W: Write> ser::SerializeTuple for TupleSerializer<'r, 'format, W> {
    type Ok = ();
    type Error = SerializeError;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        let (&field, rest) = self.fields.split_first().unwrap();
        self.fields = rest;
        value.serialize(Serializer {
            buf: self.buf,
            format: self.buf.format(field),
            writer: &mut *self.writer,
        })
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'r, 'format, W: Write> ser::SerializeTupleVariant for TupleSerializer<'r, 'format, W> {
    type Ok = ();
    type Error = SerializeError;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        ser::SerializeTuple::serialize_element(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        ser::SerializeTuple::end(self)
    }
}

impl<'r, 'format, W: Write> ser::SerializeTupleStruct for TupleSerializer<'r, 'format, W> {
    type Ok = ();
    type Error = SerializeError;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        ser::SerializeTuple::serialize_element(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        ser::SerializeTuple::end(self)
    }
}
