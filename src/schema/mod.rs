use std::{collections::HashMap, fmt::Display};

use serde::Serialize;

use crate::schema::{
    formats::{Format, FormatId, NamedFormat, Primitive, SchemaFormats},
    ser::SchemaSerializer,
    storage::FormatStorage,
};

pub mod formats;
mod ser;
pub mod storage;

#[derive(Clone, Debug)]
pub struct UnionError {
    lhs: Schema,
    rhs: Schema,
}

impl Display for UnionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unable to union {:?} with {:?}", self.lhs, self.rhs)
    }
}

#[derive(Clone, Debug)]
pub struct FieldSchema {
    name: String,
    value: Schema,
}

#[derive(Clone, Debug)]
pub struct VariantSchema {
    name: String,
    data: VariantData,
}

#[derive(Clone, Debug)]
pub enum VariantData {
    Unit,
    Tuple { fields: Vec<Schema> },
    Struct { fields: Vec<FieldSchema> },
}

impl VariantData {
    fn make_format<'buf>(&self, f: &mut SchemaFormats<'buf>, storage: &'buf FormatStorage) -> FormatId {
        match self {
            VariantData::Unit => f.make_format(Format::Unit),
            VariantData::Tuple { fields } => {
                let fields = fields.iter().map(|field| field.make_format(f, storage)).collect::<Vec<_>>();
                let fields = storage.make_view(&fields);

                f.make_format(Format::Tuple { fields })
            },
            VariantData::Struct { fields } => {
                let fields = fields
                    .iter()
                    .map(|field| {
                        let format = field.value.make_format(f, storage);
                        NamedFormat(f.make_symbol(&field.name), format)
                    })
                    .collect::<Vec<_>>();
                let fields = storage.make_view(&fields);

                f.make_format(Format::Struct { fields })
            },
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct ValueRange<T> {
    min: T,
    max_inclusive: T,
}

impl<T: Copy> ValueRange<T> {
    pub fn single(val: T) -> Self {
        Self {
            min: val,
            max_inclusive: val,
        }
    }

    fn new(min: T, max_inclusive: T) -> Self {
        Self { min, max_inclusive }
    }
}

impl ValueRange<u64> {
    fn size(&self) -> u32 {
        let len = self.max_inclusive - self.min;
        64 - len.leading_zeros()
    }

    fn to_primitive(&self) -> Primitive {
        if self.min <= u8::MAX as u64 && self.max_inclusive <= u8::MAX as u64 {
            Primitive::U8
        } else if self.min <= u16::MAX as u64 && self.max_inclusive <= u16::MAX as u64 {
            Primitive::U16
        } else if self.min <= u32::MAX as u64 && self.max_inclusive <= u32::MAX as u64 {
            Primitive::U32
        } else if self.size() <= 32 {
            Primitive::AdjustedU32(self.min)
        } else {
            Primitive::U64
        }
    }
}

impl From<ValueRange<i64>> for ValueRange<u64> {
    fn from(value: ValueRange<i64>) -> Self {
        ValueRange {
            min: value.min as u64,
            max_inclusive: value.max_inclusive as u64,
        }
    }
}

impl<T: Copy + Ord> ValueRange<T> {
    fn union_with(&mut self, b: ValueRange<T>) {
        if b.min < self.min {
            self.min = b.min;
        }

        if b.max_inclusive > self.max_inclusive {
            self.max_inclusive = b.max_inclusive;
        }
    }
}

#[derive(Clone, Debug)]
pub enum Schema {
    Struct {
        name: String,
        fields: Vec<FieldSchema>,
    },
    U64(ValueRange<u64>),
    I64(ValueRange<i64>),
    U128(ValueRange<u128>),
    I128(ValueRange<i128>),
    Seq {
        len: ValueRange<u64>,
        item: Box<Schema>,
    },
    Bytes {
        len: ValueRange<u64>,
    },
    Option {
        item: Box<Schema>,
    },
    Str {
        len: ValueRange<u64>,
    },
    Unit,
    TupleStruct {
        name: String,
        fields: Vec<Schema>,
    },
    Tuple {
        fields: Vec<Schema>,
    },
    Enum {
        name: String,
        variants: HashMap<u32, VariantSchema>,
    },
    Map {
        len: ValueRange<u64>,
        key: Box<Schema>,
        value: Box<Schema>,
    },
    Never,
}

impl Schema {
    pub fn of<T: Serialize>(val: &T) -> Schema {
        let mut result = Schema::Never;
        val.serialize(SchemaSerializer::new(&mut result)).unwrap();
        result
    }

    pub fn to_format<'buf>(&self, storage: &'buf mut FormatStorage) -> SchemaFormats<'buf> {
        let mut formats = SchemaFormats::new();
        let root = self.make_format(&mut formats, storage);
        formats.set_root(root);
        formats
    }

    fn union_with(&mut self, other: Schema) -> Result<(), UnionError> {
        match (self, other) {
            (me @ Schema::Never, other) => *me = other,
            (_, Schema::Never) => (),
            (Schema::U64(a), Schema::U64(b)) => a.union_with(b),
            (Schema::U128(a), Schema::U128(b)) => a.union_with(b),
            (Schema::I128(a), Schema::I128(b)) => a.union_with(b),
            (Schema::I64(a), Schema::I64(b)) => a.union_with(b),
            (Schema::Str { len: len_a }, Schema::Str { len: len_b }) => len_a.union_with(len_b),
            (Schema::Bytes { len: len_a }, Schema::Bytes { len: len_b }) => len_a.union_with(len_b),
            (
                Schema::Seq { len, item },
                Schema::Seq {
                    len: other_len,
                    item: other_item,
                },
            ) => {
                len.union_with(other_len);
                item.union_with(*other_item)?;
            },
            (Schema::Option { item }, Schema::Option { item: other_item }) => item.union_with(*other_item)?,
            (Schema::Tuple { fields }, Schema::Tuple { fields: other_fields }) => {
                assert_eq!(
                    fields.len(),
                    other_fields.len(),
                    "tuples must have same number of fields: {fields:?} vs {other_fields:?}"
                );
                for (a, b) in fields.iter_mut().zip(other_fields.into_iter()) {
                    a.union_with(b)?;
                }
            },
            (Schema::Unit, Schema::Unit) => (),
            (a, b) => return Err(UnionError { lhs: a.clone(), rhs: b }),
        }

        Ok(())
    }

    fn get_or_create(&mut self, other: Schema) -> &mut Self {
        if let Self::Never = self {
            *self = other;
        }

        self
    }

    pub fn make_format<'buf>(&self, f: &mut SchemaFormats<'buf>, storage: &'buf FormatStorage) -> FormatId {
        match self {
            Schema::Struct { fields, .. } => {
                let fields = fields
                    .iter()
                    .map(|field| {
                        let format = field.value.make_format(f, storage);
                        NamedFormat(f.make_symbol(&field.name), format)
                    })
                    .collect::<Vec<_>>();
                let fields = storage.make_view(&fields);

                f.make_format(Format::Struct { fields })
            },
            Schema::U64(range) => f.make_format(Format::Primitive(range.to_primitive())),
            Schema::I64(range) => f.make_format(Format::Primitive(ValueRange::<u64>::from(*range).to_primitive())),
            Schema::U128(_) | Schema::I128(_) => f.make_format(Format::U128),
            Schema::Seq { len, item } => {
                let inner = item.make_format(f, storage);
                f.make_format(Format::Sequence {
                    len: len.to_primitive(),
                    inner,
                })
            },
            Schema::Bytes { len } => f.make_format(Format::Bytes { len: len.to_primitive() }),
            Schema::Option { item } => {
                let inner = item.make_format(f, storage);
                f.make_format(Format::Option { inner })
            },
            Schema::Str { len } => f.make_format(Format::String { len: len.to_primitive() }),
            Schema::Unit => f.make_format(Format::Unit),
            Schema::TupleStruct { fields, .. } | Schema::Tuple { fields } => {
                let fields = fields.iter().map(|field| field.make_format(f, storage)).collect::<Vec<_>>();
                let fields = storage.make_view(&fields);

                f.make_format(Format::Tuple { fields })
            },
            Schema::Enum { variants, .. } => {
                let variants = variants
                    .values()
                    .map(|variant| {
                        let format = variant.data.make_format(f, storage);
                        NamedFormat(f.make_symbol(&variant.name), format)
                    })
                    .collect::<Vec<_>>();
                let variants = storage.make_view(&variants);

                f.make_format(Format::Variants {
                    variant_index: ValueRange::new(0, variants.len() as u64 - 1).to_primitive(),
                    variants,
                })
            },
            Schema::Map { len, key, value } => {
                let len = len.to_primitive();
                let key = key.make_format(f, storage);
                let value = value.make_format(f, storage);
                f.make_format(Format::Map { len, key, value })
            },
            Schema::Never => f.make_format(Format::Unit),
        }
    }
}
