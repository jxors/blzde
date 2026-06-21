use crate::schema::buf::{Format, FormatBuffer, FormatId, NamedFormat, Primitive, SchemaParseError, View};
use serde::de::{self, EnumAccess, MapAccess, SeqAccess, VariantAccess};
use std::{error::Error, fmt::Display, io::Read};

#[derive(Debug)]
pub enum DeserializeError {
    UnexpectedFormat { expected: &'static str, found: Format<'static> },
    Custom(String),
    Io(std::io::Error),
    Schema(SchemaParseError),
    EmptyFormat,
    InvalidSymbolReference,
    VariantIndexOutOfBounds,
}

impl Display for DeserializeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeserializeError::UnexpectedFormat { expected, found } => {
                write!(f, "unable to deserialize {found:?} into {expected}")
            },
            DeserializeError::Custom(s) => f.write_str(&s),
            DeserializeError::Io(e) => write!(f, "i/o error: {e}"),
            DeserializeError::Schema(e) => write!(f, "failed to parse schema: {e}"),
            DeserializeError::EmptyFormat => write!(f, "schema format specifies no data"),
            DeserializeError::InvalidSymbolReference => write!(f, "symbol reference is not valid"),
            DeserializeError::VariantIndexOutOfBounds => write!(f, "variant index out of bounds"),
        }
    }
}

impl Error for DeserializeError {}

impl de::Error for DeserializeError {
    fn custom<T>(msg: T) -> Self
    where
        T: Display,
    {
        DeserializeError::Custom(msg.to_string())
    }
}

impl From<std::io::Error> for DeserializeError {
    fn from(value: std::io::Error) -> Self {
        DeserializeError::Io(value)
    }
}

impl From<SchemaParseError> for DeserializeError {
    fn from(value: SchemaParseError) -> Self {
        DeserializeError::Schema(value)
    }
}

pub struct State<'r, 'format, R> {
    buf: &'format FormatBuffer,
    read: &'r mut R,
    field_equivalences: Vec<*const &'static str>,
}

impl<'r, 'format, R: Read> State<'r, 'format, R> {
    pub fn new(buf: &'format FormatBuffer, read: &'r mut R) -> Self {
        Self {
            buf,
            read,
            field_equivalences: vec![std::ptr::null(); buf.len()],
        }
    }
}

pub struct Deserializer<'state, 'r, 'format, R> {
    format: FormatId,
    state: &'state mut State<'r, 'format, R>,
}

impl<'state, 'r, 'format, R: Read> Deserializer<'state, 'r, 'format, R> {
    pub fn new(state: &'state mut State<'r, 'format, R>) -> Self {
        Self {
            format: state.buf.root_id(),
            state,
        }
    }

    fn lookup(self, format: FormatId) -> Self {
        Self {
            format,
            state: &mut *self.state,
        }
    }

    fn read_u64(&mut self) -> std::io::Result<u64> {
        let mut buf = [0; 8];
        self.state.read.read_exact(&mut buf)?;

        Ok(u64::from_le_bytes(buf))
    }

    fn read_u32(&mut self) -> std::io::Result<u32> {
        let mut buf = [0; 4];
        self.state.read.read_exact(&mut buf)?;

        Ok(u32::from_le_bytes(buf))
    }

    fn read_u8(&mut self) -> std::io::Result<u8> {
        let mut buf = [0; 1];
        self.state.read.read_exact(&mut buf)?;

        Ok(buf[0])
    }

    #[inline(always)]
    fn read_oob_primitive(&mut self, primitive: Primitive) -> std::io::Result<u64> {
        Ok(match primitive {
            Primitive::U8 => self.read_u8()? as u64,
            Primitive::U64 => self.read_u64()?,
            Primitive::AdjustedU32(offset) => (self.read_u32()? as u64).wrapping_add(offset),
        })
    }

    #[inline(always)]
    fn format(&self) -> Format<'format> {
        self.state.buf.format(self.format)
    }

    #[inline(always)]
    fn read_primitive(&mut self) -> Result<u64, DeserializeError> {
        let Format::Primitive(primitive) = self.format() else {
            return Err(DeserializeError::UnexpectedFormat {
                expected: "primitive",
                found: self.format().make_static(),
            });
        };

        Ok(self.read_oob_primitive(primitive)?)
    }

    fn read_vec(&mut self, len: usize) -> std::io::Result<Vec<u8>> {
        let mut vec = vec![0; len];
        self.state.read.read_exact(&mut vec)?;
        Ok(vec)
    }

    #[inline(always)]
    fn struct_fields_equivalent(&mut self, expected_fields: &'static [&'static str], fields: View<'_, NamedFormat>) -> bool {
        let ptr = expected_fields.as_ptr();
        if self.state.field_equivalences[self.format.as_usize()] == ptr {
            return true;
        }

        if fields.items().len() == expected_fields.len()
            && fields
                .items()
                .iter()
                .zip(expected_fields)
                .all(|(a, b)| self.state.buf[a.0] == **b)
        {
            self.state.field_equivalences[self.format.as_usize()] = ptr;
            return true;
        }

        false
    }
}

impl<'state, 'format, 'r, 'de, R: Read> de::Deserializer<'de> for Deserializer<'state, 'r, 'format, R> {
    type Error = DeserializeError;
    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        match self.format() {
            Format::Primitive(_) => self.deserialize_u64(visitor),
            Format::String { .. } => self.deserialize_string(visitor),
            Format::Bytes { .. } => self.deserialize_byte_buf(visitor),
            Format::Option { .. } => self.deserialize_option(visitor),
            Format::Unit => self.deserialize_unit(visitor),
            Format::Sequence { .. } => self.deserialize_seq(visitor),
            Format::Tuple { .. } => self.deserialize_tuple(usize::MAX, visitor),
            Format::Map { .. } => self.deserialize_map(visitor),
            Format::Struct { .. } => self.deserialize_struct("any", &[], visitor),
            Format::Variants { .. } => self.deserialize_enum("any", &[], visitor),
            Format::U128 => self.deserialize_u128(visitor),
        }
    }

    fn deserialize_bool<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let v = self.read_primitive()?;
        visitor.visit_bool(v != 0)
    }

    fn deserialize_i8<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let v = self.read_primitive()?;
        visitor.visit_i8(v as i8)
    }

    fn deserialize_i16<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let v = self.read_primitive()?;
        visitor.visit_i16(v as i16)
    }

    fn deserialize_i32<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let v = self.read_primitive()?;
        visitor.visit_i32(v as i32)
    }

    fn deserialize_i64<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let v = self.read_primitive()?;
        visitor.visit_i64(v as i64)
    }

    fn deserialize_u8<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let v = self.read_primitive()?;
        visitor.visit_u8(v as u8)
    }

    fn deserialize_u16<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let v = self.read_primitive()?;
        visitor.visit_u16(v as u16)
    }

    fn deserialize_u32<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let v = self.read_primitive()?;
        visitor.visit_u32(v as u32)
    }

    fn deserialize_u64<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let v = self.read_primitive()?;
        visitor.visit_u64(v)
    }

    fn deserialize_u128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let mut buf = [0; 16];
        self.state.read.read_exact(&mut buf).unwrap();
        visitor.visit_u128(u128::from_le_bytes(buf))
    }

    fn deserialize_i128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let mut buf = [0; 16];
        self.state.read.read_exact(&mut buf).unwrap();
        visitor.visit_i128(i128::from_le_bytes(buf))
    }

    fn deserialize_f32<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let v = self.read_primitive()?;
        visitor.visit_f32(f32::from_bits(v as u32))
    }

    fn deserialize_f64<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let v = self.read_primitive()?;
        visitor.visit_f64(f64::from_bits(v))
    }

    fn deserialize_char<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let v = self.read_primitive()?;
        visitor.visit_char((v as u32).try_into().unwrap())
    }

    fn deserialize_str<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let Format::String { len } = self.format() else {
            return Err(DeserializeError::UnexpectedFormat {
                expected: "string",
                found: self.format().make_static(),
            });
        };
        let len = self.read_oob_primitive(len)?;
        let bytes = self.read_vec(len as usize)?;
        let string = String::from_utf8(bytes).unwrap();
        visitor.visit_string(string)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_bytes<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let Format::Bytes { len } = self.format() else {
            return Err(DeserializeError::UnexpectedFormat {
                expected: "bytes",
                found: self.format().make_static(),
            });
        };
        let len = self.read_oob_primitive(len)?;
        let bytes = self.read_vec(len as usize)?;
        visitor.visit_byte_buf(bytes)
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_option<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let Format::Option { inner } = self.format() else {
            return Err(DeserializeError::UnexpectedFormat {
                expected: "option",
                found: self.format().make_static(),
            });
        };
        let has_option = self.read_u8()? != 0;
        if !has_option {
            visitor.visit_none()
        } else {
            visitor.visit_some(self.lookup(inner))
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let Format::Unit = self.format() else {
            return Err(DeserializeError::UnexpectedFormat {
                expected: "unit",
                found: self.format().make_static(),
            });
        };
        visitor.visit_unit()
    }

    fn deserialize_unit_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let Format::Tuple { fields } = self.format() else {
            return Err(DeserializeError::UnexpectedFormat {
                expected: "tuple (unit struct)",
                found: self.format().make_static(),
            });
        };
        assert!(fields.items().is_empty());
        visitor.visit_unit()
    }

    fn deserialize_newtype_struct<V>(self, name: &'static str, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_tuple_struct(name, 1, visitor)
    }

    fn deserialize_seq<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let Format::Sequence { len, inner } = self.format() else {
            return Err(DeserializeError::UnexpectedFormat {
                expected: "sequence",
                found: self.format().make_static(),
            });
        };
        let len = self.read_oob_primitive(len)?;

        visitor.visit_seq(SizedSequence {
            format: inner,
            state: &mut *self.state,
            remaining: len as usize,
        })
    }

    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let Format::Tuple { fields } = self.format() else {
            return Err(DeserializeError::UnexpectedFormat {
                expected: "tuple",
                found: self.format().make_static(),
            });
        };
        visitor.visit_seq(TupleFields {
            fields: fields.items(),
            state: &mut *self.state,
        })
    }

    fn deserialize_tuple_struct<V>(self, _name: &'static str, len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_tuple(len, visitor)
    }

    fn deserialize_map<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let Format::Map { len, key, value } = self.format() else {
            return Err(DeserializeError::UnexpectedFormat {
                expected: "map",
                found: self.format().make_static(),
            });
        };
        let len = self.read_oob_primitive(len)?;

        visitor.visit_map(SizedMap {
            key,
            value,
            state: &mut *self.state,
            remaining: len as usize,
        })
    }

    #[inline]
    fn deserialize_struct<V>(
        mut self, _name: &'static str, expected_fields: &'static [&'static str], visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let Format::Struct { fields } = self.format() else {
            return Err(DeserializeError::UnexpectedFormat {
                expected: "struct",
                found: self.format().make_static(),
            });
        };
        if self.struct_fields_equivalent(expected_fields, fields) {
            visitor.visit_seq(StructFields {
                fields: fields.items(),
                state: &mut *self.state,
            })
        } else {
            visitor.visit_map(StructFields {
                fields: fields.items(),
                state: &mut *self.state,
            })
        }
    }

    fn deserialize_enum<V>(
        mut self, _name: &'static str, _variants: &'static [&'static str], visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let Format::Variants { variant_index, variants } = self.format() else {
            return Err(DeserializeError::UnexpectedFormat {
                expected: "enum",
                found: self.format().make_static(),
            });
        };
        let index = self.read_oob_primitive(variant_index)?;
        let variant = *variants
            .items()
            .get(index as usize)
            .ok_or(DeserializeError::VariantIndexOutOfBounds)?;

        visitor.visit_enum(EnumVariants {
            variant,
            state: &mut *self.state,
        })
    }

    fn deserialize_identifier<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        return Err(DeserializeError::UnexpectedFormat {
            expected: "identifier",
            found: self.format().make_static(),
        });
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
}

struct StructFields<'state, 'r, 'format, R> {
    fields: &'format [NamedFormat],
    state: &'state mut State<'r, 'format, R>,
}

impl<'state, 'de, 'r, 'format, R: Read> MapAccess<'de> for StructFields<'state, 'r, 'format, R> {
    type Error = DeserializeError;

    #[inline]
    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        if let Some((format, _)) = self.fields.split_first() {
            let item = seed.deserialize(SymbolDeserializer(&self.state.buf[format.0]))?;
            Ok(Some(item))
        } else {
            Ok(None)
        }
    }

    #[inline]
    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let Some((format, rest)) = self.fields.split_first() else {
            unreachable!()
        };
        self.fields = rest;
        seed.deserialize(Deserializer {
            format: format.1,
            state: &mut *self.state,
        })
    }

    #[inline]
    fn next_entry_seed<K, V>(&mut self, kseed: K, vseed: V) -> Result<Option<(K::Value, V::Value)>, Self::Error>
    where
        K: de::DeserializeSeed<'de>,
        V: de::DeserializeSeed<'de>,
    {
        if let Some((format, rest)) = self.fields.split_first() {
            self.fields = rest;
            let key = kseed.deserialize(SymbolDeserializer(&self.state.buf[format.0]))?;
            let value = vseed.deserialize(Deserializer {
                format: format.1,
                state: &mut *self.state,
            })?;
            Ok(Some((key, value)))
        } else {
            Ok(None)
        }
    }

    #[inline(always)]
    fn size_hint(&self) -> Option<usize> {
        Some(self.fields.len())
    }
}

impl<'state, 'de, 'r, 'format, R: Read> SeqAccess<'de> for StructFields<'state, 'r, 'format, R> {
    type Error = DeserializeError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        if let Some((format, rest)) = self.fields.split_first() {
            self.fields = rest;
            seed.deserialize(Deserializer {
                format: format.1,
                state: &mut *self.state,
            })
            .map(Some)
        } else {
            Ok(None)
        }
    }

    #[inline(always)]
    fn size_hint(&self) -> Option<usize> {
        Some(self.fields.len())
    }
}

struct EnumVariants<'state, 'r, 'format, R> {
    variant: NamedFormat,
    state: &'state mut State<'r, 'format, R>,
}

impl<'state, 'de, 'r, 'format, R: Read> EnumAccess<'de> for EnumVariants<'state, 'r, 'format, R> {
    type Error = DeserializeError;
    type Variant = Deserializer<'state, 'r, 'format, R>;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let symbol_name = self
            .state
            .buf
            .try_get_symbol(self.variant.0)
            .ok_or(DeserializeError::InvalidSymbolReference)?;
        let val = seed.deserialize(SymbolDeserializer(symbol_name))?;

        Ok((
            val,
            Deserializer {
                format: self.variant.1,
                state: &mut *self.state,
            },
        ))
    }
}

impl<'state, 'de, 'r, 'format, R: Read> VariantAccess<'de> for Deserializer<'state, 'r, 'format, R> {
    type Error = DeserializeError;

    fn unit_variant(self) -> Result<(), Self::Error> {
        let Format::Unit = self.format() else {
            return Err(DeserializeError::UnexpectedFormat {
                expected: "unit variant",
                found: self.format().make_static(),
            });
        };
        Ok(())
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let Format::Tuple { fields } = self.format() else {
            return Err(DeserializeError::UnexpectedFormat {
                expected: "tuple variant",
                found: self.format().make_static(),
            });
        };
        let mut fields = TupleFields {
            fields: fields.items(),
            state: &mut *self.state,
        };

        Ok(fields.next_element_seed(seed)?.unwrap())
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let Format::Tuple { fields } = self.format() else {
            return Err(DeserializeError::UnexpectedFormat {
                expected: "tuple variant",
                found: self.format().make_static(),
            });
        };
        visitor.visit_seq(TupleFields {
            fields: fields.items(),
            state: &mut *self.state,
        })
    }

    fn struct_variant<V>(self, _fields: &'static [&'static str], visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let Format::Struct { fields } = self.format() else {
            return Err(DeserializeError::UnexpectedFormat {
                expected: "struct variant",
                found: self.format().make_static(),
            });
        };
        visitor.visit_map(StructFields {
            fields: fields.items(),
            state: &mut *self.state,
        })
    }
}

struct TupleFields<'state, 'r, 'format, R> {
    fields: &'format [FormatId],
    state: &'state mut State<'r, 'format, R>,
}

impl<'de, 'state, 'r, 'format, R: Read> SeqAccess<'de> for TupleFields<'state, 'r, 'format, R> {
    type Error = DeserializeError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        if let Some((format, rest)) = self.fields.split_first() {
            self.fields = rest;
            let item = seed.deserialize(Deserializer {
                format: *format,
                state: &mut *self.state,
            })?;
            Ok(Some(item))
        } else {
            Ok(None)
        }
    }

    #[inline(always)]
    fn size_hint(&self) -> Option<usize> {
        Some(self.fields.len())
    }
}

struct SizedSequence<'state, 'r, 'format, R> {
    remaining: usize,
    format: FormatId,
    state: &'state mut State<'r, 'format, R>,
}

impl<'state, 'de, 'r, 'format, R: Read> SeqAccess<'de> for SizedSequence<'state, 'r, 'format, R> {
    type Error = DeserializeError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        if let Some(remaining) = self.remaining.checked_sub(1) {
            self.remaining = remaining;
            let item = seed.deserialize(Deserializer {
                format: self.format,
                state: &mut *self.state,
            })?;

            Ok(Some(item))
        } else {
            Ok(None)
        }
    }

    #[inline(always)]
    fn size_hint(&self) -> Option<usize> {
        Some(self.remaining)
    }
}

struct SizedMap<'state, 'r, 'format, R> {
    remaining: usize,
    key: FormatId,
    value: FormatId,
    state: &'state mut State<'r, 'format, R>,
}

impl<'state, 'de, 'r, 'format, R: Read> MapAccess<'de> for SizedMap<'state, 'r, 'format, R> {
    type Error = DeserializeError;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        if let Some(remaining) = self.remaining.checked_sub(1) {
            self.remaining = remaining;
            seed.deserialize(Deserializer {
                format: self.key,
                state: &mut *self.state,
            })
            .map(Some)
        } else {
            Ok(None)
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        seed.deserialize(Deserializer {
            format: self.value,
            state: &mut *self.state,
        })
    }

    #[inline(always)]
    fn size_hint(&self) -> Option<usize> {
        Some(self.remaining)
    }
}

struct SymbolDeserializer<'s>(&'s str);

// Skip formatting to keep `unreachable!()` fns compact
#[rustfmt::skip]
impl<'de, 's> de::Deserializer<'de> for SymbolDeserializer<'s> {
    type Error = DeserializeError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de> {
        visitor.visit_str(self.0)
    }
    
    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de> {
        visitor.visit_str(self.0)
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de> {
        visitor.visit_str(self.0)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de> {
        visitor.visit_str(self.0)
    }

    fn deserialize_bool<V>(self, _visitor: V) -> Result<V::Value, Self::Error> where V: de::Visitor<'de> { unreachable!() }
    fn deserialize_i8<V>(self, _visitor: V) -> Result<V::Value, Self::Error> where V: de::Visitor<'de> { unreachable!() }
    fn deserialize_i16<V>(self, _visitor: V) -> Result<V::Value, Self::Error> where V: de::Visitor<'de> { unreachable!() }
    fn deserialize_i32<V>(self, _visitor: V) -> Result<V::Value, Self::Error> where V: de::Visitor<'de> { unreachable!() }
    fn deserialize_i64<V>(self, _visitor: V) -> Result<V::Value, Self::Error> where V: de::Visitor<'de> { unreachable!() }
    fn deserialize_u8<V>(self, _visitor: V) -> Result<V::Value, Self::Error> where V: de::Visitor<'de> { unreachable!() }
    fn deserialize_u16<V>(self, _visitor: V) -> Result<V::Value, Self::Error> where V: de::Visitor<'de> { unreachable!() }
    fn deserialize_u32<V>(self, _visitor: V) -> Result<V::Value, Self::Error> where V: de::Visitor<'de> { unreachable!() }
    fn deserialize_u64<V>(self, _visitor: V) -> Result<V::Value, Self::Error> where V: de::Visitor<'de> { unreachable!() }
    fn deserialize_f32<V>(self, _visitor: V) -> Result<V::Value, Self::Error> where V: de::Visitor<'de> { unreachable!() }
    fn deserialize_f64<V>(self, _visitor: V) -> Result<V::Value, Self::Error> where V: de::Visitor<'de> { unreachable!() }
    fn deserialize_char<V>(self, _visitor: V) -> Result<V::Value, Self::Error> where V: de::Visitor<'de> { unreachable!() }
    fn deserialize_string<V>(self, _visitor: V) -> Result<V::Value, Self::Error> where V: de::Visitor<'de> { unreachable!() }
    fn deserialize_bytes<V>(self, _visitor: V) -> Result<V::Value, Self::Error> where V: de::Visitor<'de> { unreachable!() }
    fn deserialize_byte_buf<V>(self, _visitor: V) -> Result<V::Value, Self::Error> where V: de::Visitor<'de> { unreachable!() }
    fn deserialize_option<V>(self, _visitor: V) -> Result<V::Value, Self::Error> where V: de::Visitor<'de> { unreachable!() }
    fn deserialize_unit<V>(self, _visitor: V) -> Result<V::Value, Self::Error> where V: de::Visitor<'de> { unreachable!() }
    fn deserialize_unit_struct<V>(self, _name: &'static str, _visitor: V) -> Result<V::Value, Self::Error> where V: de::Visitor<'de> { unreachable!() }
    fn deserialize_newtype_struct<V>(self, _name: &'static str, _visitor: V) -> Result<V::Value, Self::Error> where V: de::Visitor<'de> { unreachable!() }
    fn deserialize_seq<V>(self, _visitor: V) -> Result<V::Value, Self::Error> where V: de::Visitor<'de> { unreachable!() }
    fn deserialize_tuple<V>(self, _len: usize, _visitor: V) -> Result<V::Value, Self::Error> where V: de::Visitor<'de> { unreachable!() }
    fn deserialize_tuple_struct<V>(self, _name: &'static str, _len: usize, _visitor: V) -> Result<V::Value, Self::Error> where V: de::Visitor<'de> { unreachable!() }
    fn deserialize_map<V>(self, _visitor: V) -> Result<V::Value, Self::Error> where V: de::Visitor<'de> { unreachable!() }
    fn deserialize_struct<V>(self, _name: &'static str, _fields: &'static [&'static str], _visitor: V) -> Result<V::Value, Self::Error> where V: de::Visitor<'de> { unreachable!() }
    fn deserialize_enum<V>(self, _name: &'static str, _variants: &'static [&'static str], _visitor: V) -> Result<V::Value, Self::Error> where V: de::Visitor<'de> { unreachable!() }
}
