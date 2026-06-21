use serde::ser;
use std::{collections::HashMap, error::Error, fmt::Display};

use crate::schema::{FieldSchema, Schema, UnionError, ValueRange, VariantData, VariantSchema};

#[derive(Debug)]
pub enum SchemaError {
    UnexpectedSchema { expected: &'static str, found: Schema },
    UnexpectedVariant { expected: &'static str, found: VariantData },
    Custom(String),
    Union(UnionError),
}

impl Display for SchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchemaError::UnexpectedSchema { expected, found } => write!(f, "unable to write {expected} into {found:?}"),
            SchemaError::UnexpectedVariant { expected, found } => write!(f, "unable to write {expected} into {found:?}"),
            SchemaError::Custom(msg) => f.write_str(msg),
            SchemaError::Union(e) => write!(f, "{e}"),
        }
    }
}

impl Error for SchemaError {}

impl ser::Error for SchemaError {
    fn custom<T>(msg: T) -> Self
    where
        T: Display,
    {
        SchemaError::Custom(msg.to_string())
    }
}

impl From<UnionError> for SchemaError {
    fn from(value: UnionError) -> Self {
        SchemaError::Union(value)
    }
}

pub struct SchemaSerializer<'s> {
    output_schema: &'s mut Schema,
}

impl<'s> SchemaSerializer<'s> {
    pub fn new(output_schema: &'s mut Schema) -> Self {
        Self { output_schema }
    }
}

impl<'s> ser::Serializer for SchemaSerializer<'s> {
    type Ok = ();
    type Error = SchemaError;
    type SerializeSeq = SeqSchemaSerializer<'s>;
    type SerializeTuple = TupleSchemaSerializer<'s>;
    type SerializeTupleStruct = TupleSchemaSerializer<'s>;
    type SerializeTupleVariant = TupleSchemaSerializer<'s>;
    type SerializeMap = MapSchemaSerializer<'s>;
    type SerializeStruct = StructSchemaSerializer<'s>;
    type SerializeStructVariant = StructSchemaSerializer<'s>;

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
        self.output_schema.union_with(Schema::I64(ValueRange::single(v)))?;
        Ok(())
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

    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        self.output_schema.union_with(Schema::U64(ValueRange::single(v)))?;
        Ok(())
    }

    fn serialize_u128(self, v: u128) -> Result<Self::Ok, Self::Error> {
        self.output_schema.union_with(Schema::U128(ValueRange::single(v)))?;
        Ok(())
    }

    fn serialize_i128(self, v: i128) -> Result<Self::Ok, Self::Error> {
        self.output_schema.union_with(Schema::I128(ValueRange::single(v)))?;
        Ok(())
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

    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        self.output_schema.union_with(Schema::Str {
            len: ValueRange::single(v.len() as u64),
        })?;
        Ok(())
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        self.output_schema.union_with(Schema::Bytes {
            len: ValueRange::single(v.len() as u64),
        })?;
        Ok(())
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        self.output_schema.union_with(Schema::Option {
            item: Box::new(Schema::Never),
        })?;
        Ok(())
    }

    fn serialize_some<T>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        let Schema::Option { item } = self.output_schema.get_or_create(Schema::Option {
            item: Box::new(Schema::Never),
        }) else {
            return Err(SchemaError::UnexpectedSchema {
                expected: "option (some)",
                found: self.output_schema.clone(),
            });
        };
        value.serialize(SchemaSerializer::new(item))?;
        Ok(())
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        self.output_schema.union_with(Schema::Unit)?;
        Ok(())
    }

    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok, Self::Error> {
        self.output_schema.union_with(Schema::TupleStruct {
            name: name.to_string(),
            fields: Vec::new(),
        })?;
        Ok(())
    }

    fn serialize_unit_variant(
        self, name: &'static str, variant_index: u32, variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        let Schema::Enum { variants, .. } = self.output_schema.get_or_create(Schema::Enum {
            name: name.to_string(),
            variants: HashMap::new(),
        }) else {
            return Err(SchemaError::UnexpectedSchema {
                expected: "enum",
                found: self.output_schema.clone(),
            });
        };
        let VariantData::Unit = variants
            .entry(variant_index)
            .or_insert_with(|| VariantSchema {
                name: variant.to_string(),
                data: VariantData::Unit,
            })
            .data
        else {
            return Err(SchemaError::UnexpectedSchema {
                expected: "unit variant",
                found: self.output_schema.clone(),
            });
        };

        Ok(())
    }

    fn serialize_newtype_struct<T>(self, name: &'static str, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        let Schema::TupleStruct { fields, .. } = self.output_schema.get_or_create(Schema::TupleStruct {
            name: name.to_string(),
            fields: vec![Schema::Never],
        }) else {
            return Err(SchemaError::UnexpectedSchema {
                expected: "tuple struct",
                found: self.output_schema.clone(),
            });
        };
        assert_eq!(fields.len(), 1);
        value.serialize(SchemaSerializer::new(&mut fields[0]))?;
        Ok(())
    }

    fn serialize_newtype_variant<T>(
        self, name: &'static str, variant_index: u32, variant: &'static str, value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + serde::Serialize,
    {
        let Schema::Enum { variants, .. } = self.output_schema.get_or_create(Schema::Enum {
            name: name.to_string(),
            variants: HashMap::new(),
        }) else {
            return Err(SchemaError::UnexpectedSchema {
                expected: "enum",
                found: self.output_schema.clone(),
            });
        };
        let VariantData::Tuple { fields: values } = &mut variants
            .entry(variant_index)
            .or_insert_with(|| VariantSchema {
                name: variant.to_string(),
                data: VariantData::Tuple {
                    fields: vec![Schema::Never; 1],
                },
            })
            .data
        else {
            return Err(SchemaError::UnexpectedSchema {
                expected: "newtype variant",
                found: self.output_schema.clone(),
            });
        };
        assert_eq!(values.len(), 1);

        value.serialize(SchemaSerializer::new(&mut values[0]))?;

        Ok(())
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        self.output_schema.union_with(Schema::Seq {
            len: ValueRange::single(len.unwrap_or(0) as u64),
            item: Box::new(Schema::Never),
        })?;

        let Schema::Seq { len, item } = self.output_schema else {
            return Err(SchemaError::UnexpectedSchema {
                expected: "sequence",
                found: self.output_schema.clone(),
            });
        };

        Ok(SeqSchemaSerializer {
            len,
            item,
            current_len: 0,
        })
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        self.output_schema.union_with(Schema::Tuple {
            fields: vec![Schema::Never; len],
        })?;

        let Schema::Tuple { fields } = self.output_schema else {
            return Err(SchemaError::UnexpectedSchema {
                expected: "tuple",
                found: self.output_schema.clone(),
            });
        };

        Ok(TupleSchemaSerializer { fields, field_index: 0 })
    }

    fn serialize_tuple_struct(self, name: &'static str, len: usize) -> Result<Self::SerializeTupleStruct, Self::Error> {
        let fields = match self.output_schema.get_or_create(Schema::TupleStruct {
            name: name.to_string(),
            fields: vec![Schema::Never; len],
        }) {
            Schema::TupleStruct { fields, .. } => fields,
            other => {
                return Err(SchemaError::UnexpectedSchema {
                    expected: "tuple struct",
                    found: other.clone(),
                });
            },
        };

        Ok(TupleSchemaSerializer { fields, field_index: 0 })
    }

    fn serialize_tuple_variant(
        self, name: &'static str, variant_index: u32, variant: &'static str, len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        let variants = match self.output_schema.get_or_create(Schema::Enum {
            name: name.to_string(),
            variants: HashMap::new(),
        }) {
            Schema::Enum { variants, .. } => variants,
            other => {
                return Err(SchemaError::UnexpectedSchema {
                    expected: "enum",
                    found: other.clone(),
                });
            },
        };

        let values = match &mut variants
            .entry(variant_index)
            .or_insert_with(|| VariantSchema {
                name: variant.to_string(),
                data: VariantData::Tuple {
                    fields: vec![Schema::Never; len],
                },
            })
            .data
        {
            VariantData::Tuple { fields: values } => values,
            other => {
                return Err(SchemaError::UnexpectedVariant {
                    expected: "tuple variant",
                    found: other.clone(),
                });
            },
        };

        Ok(TupleSchemaSerializer {
            fields: values,
            field_index: 0,
        })
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        let (len, key, value) = match self.output_schema.get_or_create(Schema::Map {
            len: ValueRange::single(len.unwrap_or(0) as u64),
            key: Box::new(Schema::Never),
            value: Box::new(Schema::Never),
        }) {
            Schema::Map { len, key, value } => (len, key, value),
            other => {
                return Err(SchemaError::UnexpectedSchema {
                    expected: "map",
                    found: other.clone(),
                });
            },
        };

        Ok(MapSchemaSerializer {
            len,
            key,
            value,
            current_len: 0,
        })
    }

    fn serialize_struct(self, name: &'static str, _len: usize) -> Result<Self::SerializeStruct, Self::Error> {
        let fields = match self.output_schema.get_or_create(Schema::Struct {
            name: name.to_string(),
            fields: Vec::new(),
        }) {
            Schema::Struct { fields, .. } => fields,
            other => {
                panic!("tried to serialize struct {name} into {other:?}");
            },
        };

        Ok(StructSchemaSerializer { fields, field_index: 0 })
    }

    fn serialize_struct_variant(
        self, name: &'static str, variant_index: u32, variant: &'static str, _len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        let variants = match self.output_schema.get_or_create(Schema::Enum {
            name: name.to_string(),
            variants: HashMap::new(),
        }) {
            Schema::Enum { variants, .. } => variants,
            other => {
                return Err(SchemaError::UnexpectedSchema {
                    expected: "enum",
                    found: other.clone(),
                });
            },
        };
        let fields = match &mut variants
            .entry(variant_index)
            .or_insert_with(|| VariantSchema {
                name: variant.to_string(),
                data: VariantData::Struct { fields: Vec::new() },
            })
            .data
        {
            VariantData::Struct { fields } => fields,
            other => {
                return Err(SchemaError::UnexpectedVariant {
                    expected: "struct variant",
                    found: other.clone(),
                });
            },
        };

        Ok(StructSchemaSerializer { fields, field_index: 0 })
    }
}

pub struct SeqSchemaSerializer<'s> {
    item: &'s mut Schema,
    len: &'s mut ValueRange<u64>,
    current_len: u64,
}

impl<'s, 'de> ser::SerializeSeq for SeqSchemaSerializer<'s> {
    type Ok = ();
    type Error = SchemaError;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        value.serialize(SchemaSerializer {
            output_schema: self.item,
        })?;
        self.current_len += 1;

        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.len.union_with(ValueRange::single(self.current_len));
        Ok(())
    }
}

pub struct MapSchemaSerializer<'s> {
    key: &'s mut Schema,
    value: &'s mut Schema,
    len: &'s mut ValueRange<u64>,
    current_len: u64,
}

impl<'s> ser::SerializeMap for MapSchemaSerializer<'s> {
    type Ok = ();
    type Error = SchemaError;

    fn serialize_key<T>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        key.serialize(SchemaSerializer::new(self.key))
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        value.serialize(SchemaSerializer::new(self.value))?;
        self.current_len += 1;
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.len.union_with(ValueRange::single(self.current_len));
        Ok(())
    }
}

pub struct StructSchemaSerializer<'s> {
    fields: &'s mut Vec<FieldSchema>,
    field_index: usize,
}

impl StructSchemaSerializer<'_> {
    fn process_field<E>(&mut self, key: &str, f: impl FnOnce(&mut Schema) -> Result<(), E>) -> Result<(), E> {
        let mut tmp = None;
        let field_schema = self.fields.get_mut(self.field_index).unwrap_or_else(|| {
            tmp.get_or_insert(FieldSchema {
                name: key.to_string(),
                value: Schema::Never,
            })
        });
        f(&mut field_schema.value)?;

        if let Some(new) = tmp {
            self.fields.push(new);
        }

        self.field_index += 1;
        Ok(())
    }
}

impl<'s> ser::SerializeStruct for StructSchemaSerializer<'s> {
    type Ok = ();
    type Error = SchemaError;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        self.process_field(key, |output_schema| value.serialize(SchemaSerializer { output_schema }))
    }

    fn skip_field(&mut self, key: &'static str) -> Result<(), Self::Error> {
        self.process_field(key, |_| Ok(()))
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'s, 'de> ser::SerializeStructVariant for StructSchemaSerializer<'s> {
    type Ok = ();
    type Error = SchemaError;

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

pub struct TupleSchemaSerializer<'s> {
    fields: &'s mut Vec<Schema>,
    field_index: usize,
}

impl<'s> ser::SerializeTuple for TupleSchemaSerializer<'s> {
    type Ok = ();
    type Error = SchemaError;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        ser::SerializeTupleStruct::serialize_field(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        ser::SerializeTupleStruct::end(self)
    }
}

impl<'s> ser::SerializeTupleStruct for TupleSchemaSerializer<'s> {
    type Ok = ();
    type Error = SchemaError;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        let mut tmp = None;
        let output_schema = self
            .fields
            .get_mut(self.field_index)
            .unwrap_or_else(|| tmp.get_or_insert(Schema::Never));
        value.serialize(SchemaSerializer { output_schema })?;

        if let Some(new) = tmp {
            self.fields.push(new);
        }

        self.field_index += 1;
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'s, 'de> ser::SerializeTupleVariant for TupleSchemaSerializer<'s> {
    type Ok = ();
    type Error = SchemaError;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        ser::SerializeTupleStruct::serialize_field(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        ser::SerializeTupleStruct::end(self)
    }
}

#[cfg(test)]
mod tests {
    use super::SchemaSerializer;
    use crate::schema::Schema;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Serialize, Deserialize)]
    struct Test {
        field1: u64,
        field2: i16,
    }

    #[test]
    fn test_schema_serialize() {
        let val = Test { field1: 10, field2: -5 };
        let mut schema = Schema::Never;
        val.serialize(SchemaSerializer::new(&mut schema)).unwrap();
        println!("{schema:#?}");
    }

    #[test]
    fn test_vec_of_struct_schema_serialize() {
        let val = vec![
            Test { field1: 10, field2: -5 },
            Test {
                field1: u64::MAX,
                field2: i16::MIN,
            },
            Test {
                field1: u64::MAX,
                field2: i16::MIN,
            },
        ];
        let mut schema = Schema::Never;
        val.serialize(SchemaSerializer::new(&mut schema)).unwrap();
        println!("{schema:#?}");
    }
}
