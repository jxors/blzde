use std::{
    collections::{HashSet, VecDeque},
    fmt::{Debug, Display},
    io::{Read, Write},
    ops::Index,
    string::FromUtf8Error,
};

use bytemuck::Pod;

#[derive(Debug)]
pub enum FormatParseError {
    UnexpectedEnd,
    FormatReferenceOutOfBounds,
    Corrupt,
}

impl Display for FormatParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormatParseError::UnexpectedEnd => write!(f, "schema format ended unexpectedly"),
            FormatParseError::FormatReferenceOutOfBounds => write!(f, "format reference is out of bounds"),
            FormatParseError::Corrupt => write!(f, "schema is corrupt"),
        }
    }
}

#[derive(Debug)]
pub enum SchemaParseError {
    TooBig(u32),
    Format(FormatParseError),
    Io(std::io::Error),
    Symbol(FromUtf8Error),
}

impl Display for SchemaParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchemaParseError::TooBig(bytes) => write!(f, "schema format is too big: {} MiB (maximum 128 MiB)", bytes >> 20),
            SchemaParseError::Io(e) => write!(f, "i/o error: {e}"),
            SchemaParseError::Symbol(e) => write!(f, "invalid symbol: {e}"),
            SchemaParseError::Format(e) => write!(f, "{e}"),
        }
    }
}

impl From<std::io::Error> for SchemaParseError {
    fn from(value: std::io::Error) -> Self {
        SchemaParseError::Io(value)
    }
}

impl From<FromUtf8Error> for SchemaParseError {
    fn from(value: FromUtf8Error) -> Self {
        SchemaParseError::Symbol(value)
    }
}

impl From<FormatParseError> for SchemaParseError {
    fn from(value: FormatParseError) -> Self {
        SchemaParseError::Format(value)
    }
}


#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct FormatId(u32);

impl Debug for FormatId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "${}", self.0)
    }
}

pub const MAX_SCHEMA_BYTE_SIZE: usize = 128 << 20;

impl FormatId {
    pub const INVALID: FormatId = FormatId(u32::MAX);

    fn write(&self, data: &mut Vec<u32>) {
        data.push(self.0);
    }

    #[inline(always)]
    fn read(data: &[u32]) -> Result<(FormatId, &[u32]), FormatParseError> {
        let (&id, data) = data.split_first().ok_or(FormatParseError::UnexpectedEnd)?;
        Ok((FormatId(id), data))
    }

    pub fn as_usize(&self) -> usize {
        self.0 as usize
    }
}

#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct NamedFormat(pub SymbolId, pub FormatId);

#[derive(Copy, Clone, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(transparent)]
pub struct SymbolId(u32);

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Primitive {
    U8,
    U64,
    AdjustedU32(u64),
}

impl Primitive {
    fn identifier(&self) -> u32 {
        match self {
            Primitive::U8 => 0x0000_0000,
            Primitive::U64 => 0x0100_0000,
            Primitive::AdjustedU32(_) => 0x0200_0000,
        }
    }

    fn write(&self, data: &mut Vec<u32>) {
        match self {
            Primitive::U64 | Primitive::U8 => (),
            Primitive::AdjustedU32(val) => {
                data.push(*val as u32);
                data.push((val >> 32) as u32);
            },
        }
    }

    #[inline(always)]
    fn from_identifier(identifier: u32, data: &[u32]) -> Result<(Primitive, &[u32]), FormatParseError> {
        Ok(match identifier & 0x0f00_0000 {
            0x0000_0000 => (Primitive::U8, data),
            0x0100_0000 => (Primitive::U64, data),
            0x0200_0000 => {
                let (&[low_offset, high_offset], data) = data.split_first_chunk().ok_or(FormatParseError::UnexpectedEnd)?;
                (Primitive::AdjustedU32(low_offset as u64 | ((high_offset as u64) << 32)), data)
            },
            _ => return Err(FormatParseError::Corrupt),
        })
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Format<'a> {
    Primitive(Primitive),
    String {
        len: Primitive,
    },
    Bytes {
        len: Primitive,
    },
    Option {
        inner: FormatId,
    },
    Unit,
    Sequence {
        len: Primitive,
        inner: FormatId,
    },
    Tuple {
        fields: View<'a, FormatId>,
    },
    Map {
        len: Primitive,
        key: FormatId,
        value: FormatId,
    },
    Struct {
        fields: View<'a, NamedFormat>,
    },
    Variants {
        variant_index: Primitive,
        variants: View<'a, NamedFormat>,
    },
    U128,
}

impl Format<'_> {
    fn identifier(&self) -> u32 {
        match self {
            Format::Primitive(p) => 0x0000_0000 | p.identifier(),
            Format::String { len } => 0x1000_0000 | len.identifier(),
            Format::Bytes { len } => 0x2000_0000 | len.identifier(),
            Format::Option { .. } => 0x3000_0000,
            Format::Unit => 0x4000_0000,
            Format::Sequence { len, .. } => 0x5000_0000 | len.identifier(),
            Format::Tuple { fields } => 0x6000_0000 | fields.identifier(),
            Format::Map { len, .. } => 0x7000_0000 | len.identifier(),
            Format::Struct { fields } => 0x8000_0000 | fields.identifier(),
            Format::Variants { variant_index, variants } => 0x9000_0000 | variant_index.identifier() | variants.identifier(),
            Format::U128 => 0xA000_0000,
        }
    }

    pub fn make_static(&self) -> Format<'static> {
        match *self {
            Format::Primitive(primitive) => Format::Primitive(primitive),
            Format::String { len } => Format::String { len },
            Format::Bytes { len } => Format::Bytes { len },
            Format::Option { inner } => Format::Option { inner },
            Format::Unit => Format::Unit,
            Format::Sequence { len, inner } => Format::Sequence { len, inner },
            Format::Tuple { .. } => Format::Tuple { fields: View::new(&[]) },
            Format::Map { len, key, value } => Format::Map { len, key, value },
            Format::Struct { .. } => Format::Struct { fields: View::new(&[]) },
            Format::Variants { variant_index, .. } => Format::Variants {
                variant_index,
                variants: View::new(&[]),
            },
            Format::U128 => Format::U128,
        }
    }

    fn write(&self, data: &mut Vec<u32>) {
        data.push(self.identifier());
        match self {
            Format::Primitive(len) | Format::String { len } | Format::Bytes { len } => len.write(data),
            Format::Option { inner } => inner.write(data),
            Format::Unit => (),
            Format::Sequence { len, inner } => {
                len.write(data);
                inner.write(data);
            },
            Format::Tuple { fields } => fields.write(data),
            Format::Map { len, key, value } => {
                len.write(data);
                key.write(data);
                value.write(data);
            },
            Format::Struct { fields } => fields.write(data),
            Format::Variants { variant_index, variants } => {
                variant_index.write(data);
                variants.write(data);
            },
            Format::U128 => (),
        }
    }

    #[inline(always)]
    fn read(data: &[u32]) -> Result<(Format<'_>, &[u32]), FormatParseError> {
        let (&identifier, data) = data.split_first().ok_or(FormatParseError::UnexpectedEnd)?;
        Ok(match identifier & 0xf000_0000 {
            0x0000_0000 => {
                let (primitive, data) = Primitive::from_identifier(identifier, data)?;
                (Format::Primitive(primitive), data)
            },
            0x1000_0000 => {
                let (len, data) = Primitive::from_identifier(identifier, data)?;
                (Format::String { len }, data)
            },
            0x2000_0000 => {
                let (len, data) = Primitive::from_identifier(identifier, data)?;
                (Format::Bytes { len }, data)
            },
            0x3000_0000 => {
                let (inner, data) = FormatId::read(data)?;
                (Format::Option { inner }, data)
            },
            0x4000_0000 => (Format::Unit, data),
            0x5000_0000 => {
                let (len, data) = Primitive::from_identifier(identifier, data)?;
                let (inner, data) = FormatId::read(data)?;
                (Format::Sequence { len, inner }, data)
            },
            0x6000_0000 => {
                let (fields, data) = View::from_identifier(identifier, data)?;
                (Format::Tuple { fields }, data)
            },
            0x7000_0000 => {
                let (len, data) = Primitive::from_identifier(identifier, data)?;
                let (key, data) = FormatId::read(data)?;
                let (value, data) = FormatId::read(data)?;
                (Format::Map { len, key, value }, data)
            },
            0x8000_0000 => {
                let (fields, data) = View::from_identifier(identifier, data)?;
                (Format::Struct { fields }, data)
            },
            0x9000_0000 => {
                let (variant_index, data) = Primitive::from_identifier(identifier, data)?;
                let (variants, data) = View::from_identifier(identifier, data)?;
                (Format::Variants { variant_index, variants }, data)
            },
            0xA000_0000 => (Format::U128, data),
            _ => return Err(FormatParseError::Corrupt),
        })
    }
}

#[derive(Copy, Clone, PartialEq)]
pub struct View<'a, T>(&'a [T]);

impl<'a, T: Pod> View<'a, T> {
    #[inline(always)]
    pub fn new(data: &'a [T]) -> Self {
        Self(data)
    }

    #[inline(always)]
    pub fn items(&self) -> &'a [T] {
        self.0
    }

    fn write(&self, data: &mut Vec<u32>) {
        data.extend_from_slice(bytemuck::cast_slice(self.0));
    }

    fn from_identifier(identifier: u32, data: &'a [u32]) -> Result<(View<'a, T>, &'a [u32]), FormatParseError> {
        let len = (identifier & 0xffff) as usize;
        if len > data.len() {
            return Err(FormatParseError::Corrupt);
        }

        let len = len * (std::mem::size_of::<T>() / 4);
        let (view, rest) = data
            .split_at_checked(len)
            .ok_or(FormatParseError::FormatReferenceOutOfBounds)?;
        let view = bytemuck::cast_slice(view);
        Ok((View(view), rest))
    }

    fn identifier(&self) -> u32 {
        self.0.len() as u32
    }
}

impl<T: Pod + Debug> Debug for View<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.items()).finish()
    }
}

#[derive(Clone, PartialEq)]
pub struct FormatStorage {
    data: Vec<u32>,
}

impl FormatStorage {
    pub fn new(num_items: usize) -> Self {
        Self {
            data: vec![0; num_items],
        }
    }

    pub fn read_from(mut r: impl Read) -> Result<Self, SchemaParseError> {
        let total_words = {
            let mut buf = [0; 4];
            r.read_exact(&mut buf)?;
            u32::from_le_bytes(buf)
        };

        if total_words as usize > MAX_SCHEMA_BYTE_SIZE / 4 {
            return Err(SchemaParseError::TooBig(total_words.saturating_mul(4)));
        }

        let rest = {
            let mut rest = vec![0u32; total_words as usize];
            r.read_exact(bytemuck::cast_slice_mut(&mut rest))?;
            rest
        };

        Ok(Self {
            data: rest,
        })
    }

    pub fn create_bump_alloc(&mut self) -> BumpAlloc<'_> {
        BumpAlloc(&mut self.data[..])
    }
}

pub struct BumpAlloc<'buf>(&'buf mut [u32]);

impl<'buf> BumpAlloc<'buf> {
    pub fn make_buffer(self, len: usize) -> (&'buf mut [u32], BumpAlloc<'buf>) {
        let (first, rest) = self.0.split_at_mut(len);
        (first, BumpAlloc(rest))
    }
}

#[derive(Clone, PartialEq)]
pub struct SchemaFormats<'buf> {
    formats: Vec<Format<'buf>>,
    symbols: Vec<String>,
    root: FormatId,
}

impl<'buf> SchemaFormats<'buf> {
    pub fn new() -> Self {
        Self {
            formats: Vec::new(),
            symbols: Vec::new(),
            root: FormatId::INVALID,
        }
    }

    pub fn set_root(&mut self, root: FormatId) {
        self.root = root;
    }

    pub fn make_symbol(&mut self, name: &str) -> SymbolId {
        match self.symbols.iter().position(|s| s == name) {
            Some(pos) => SymbolId(pos as u32),
            None => {
                self.symbols.push(name.to_string());
                SymbolId(self.symbols.len() as u32 - 1)
            },
        }
    }

    pub fn make_format(&mut self, format: Format<'buf>) -> FormatId {
        let id = self.formats.len();
        self.formats.push(format);
        FormatId(id as u32)
    }

    pub fn root(&self) -> Format<'_> {
        assert_ne!(self.root, FormatId::INVALID);
        self.format(self.root)
    }

    pub fn try_get_format(&self, id: FormatId) -> Result<Format<'_>, FormatParseError> {
        match self.formats.get(id.0 as usize) {
            Some(format) => Ok(*format),
            None => Err(FormatParseError::FormatReferenceOutOfBounds),
        }
    }

    #[inline(always)]
    pub fn format(&self, id: FormatId) -> Format<'_> {
        self.try_get_format(id).expect("format should have been fully validated after parsing")
    }

    pub fn root_id(&self) -> FormatId {
        self.root
    }

    pub fn write_into(&self, w: &mut impl Write) -> Result<(), std::io::Error> {
        let mut data = Vec::new();
        for format in self.formats.iter() {
            format.write(&mut data);
        }
        
        let total_byte_len = (3 + data.len()) * 4 + self.symbols.iter().map(|s| 4 + s.len()).sum::<usize>();

        if total_byte_len > MAX_SCHEMA_BYTE_SIZE {
            panic!("schema too large");
        }

        let padding = 7 & 0usize.wrapping_sub(total_byte_len);
        debug_assert!((total_byte_len + padding).is_multiple_of(8));
        w.write_all(&((total_byte_len + padding) as u32 / 4).to_le_bytes())?;
        w.write_all(&(self.root.0 as u32).to_le_bytes())?;
        w.write_all(&(data.len() as u32).to_le_bytes())?;
        w.write_all(bytemuck::cast_slice(&data))?;
        w.write_all(&(self.symbols.len() as u32).to_le_bytes())?;

        for symbol in self.symbols.iter() {
            w.write_all(&(symbol.len() as u32).to_le_bytes())?;
            w.write_all(symbol.as_bytes())?;
        }

        w.write_all(&[0; 32][..padding])?;

        Ok(())
    }

    pub fn read_from(storage: &'buf FormatStorage) -> Result<SchemaFormats<'buf>, SchemaParseError> {
        let rest = &storage.data[..];
        let (&root, rest) = rest.split_first().ok_or(FormatParseError::UnexpectedEnd)?;
        let (&num_words, rest) = rest.split_first().ok_or(FormatParseError::UnexpectedEnd)?;
        let (mut data, rest) = rest
            .split_at_checked(num_words as usize)
            .ok_or(FormatParseError::UnexpectedEnd)?;
        let (&num_symbols, rest) = rest.split_first().ok_or(FormatParseError::UnexpectedEnd)?;
        let mut rest: &[u8] = bytemuck::cast_slice(rest);
        let symbols = (0..num_symbols)
            .map(|_| {
                let len;
                let data;

                (len, rest) = rest.split_at_checked(4).ok_or(FormatParseError::UnexpectedEnd)?;
                let len = u32::from_le_bytes(len.try_into().unwrap());
                (data, rest) = rest.split_at_checked(len as usize).ok_or(FormatParseError::UnexpectedEnd)?;

                Ok(String::from_utf8(data.to_vec())?)
            })
            .collect::<Result<_, SchemaParseError>>()?;

        let mut formats = Vec::new();
        while !data.is_empty() {
            let (format, rest) = Format::read(data)?;
            data = rest;
            formats.push(format);
        }

        let result = Self {
            formats: formats.to_vec(), // TODO
            root: FormatId(root),
            symbols,
        };

        result.integrity_check()?;

        Ok(result)
    }

    pub fn len(&self) -> usize {
        self.formats.len()
    }

    pub fn get_symbol(&self, id: SymbolId) -> &str {
        &*self.symbols[id.0 as usize]
    }

    fn integrity_check(&self) -> Result<(), SchemaParseError> {
        if self.root == FormatId::INVALID {
            return Ok(());
        }

        let mut seen = HashSet::new();
        let mut frontier = VecDeque::new();
        frontier.push_back(self.root);

        while let Some(id) = frontier.pop_front() {
            let format = self.try_get_format(id)?;
            match format {
                Format::Option { inner } | Format::Sequence { inner, .. } => {
                    if seen.insert(inner) {
                        frontier.push_back(inner);
                    }
                },
                Format::Tuple { fields } => {
                    for &field in fields.items() {
                        if seen.insert(field) {
                            frontier.push_back(field);
                        }
                    }
                },
                Format::Map { key, value, .. } => {
                    if seen.insert(key) {
                        frontier.push_back(key);
                    }

                    if seen.insert(value) {
                        frontier.push_back(value);
                    }
                },
                Format::Struct { fields } | Format::Variants { variants: fields, .. } => {
                    for &field in fields.items() {
                        if seen.insert(field.1) {
                            frontier.push_back(field.1);
                        }
                    }
                },
                _ => (),
            }
        }

        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.root == FormatId::INVALID
    }

    pub fn try_get_symbol(&self, variant: SymbolId) -> Option<&str> {
        self.symbols.get(variant.0 as usize).map(|s| s.as_str())
    }
}

impl Debug for SchemaFormats<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut f = f.debug_map();
        let mut seen = HashSet::new();
        let mut frontier = VecDeque::new();

        if self.root != FormatId::INVALID {
            frontier.push_back(self.root);
        }

        while let Some(id) = frontier.pop_front() {
            let format = self.format(id);
            f.entry(&id, &format);

            match format {
                Format::Option { inner } | Format::Sequence { inner, .. } => {
                    if seen.insert(inner) {
                        frontier.push_back(inner);
                    }
                },
                Format::Tuple { fields } => {
                    for &field in fields.items() {
                        if seen.insert(field) {
                            frontier.push_back(field);
                        }
                    }
                },
                Format::Map { key, value, .. } => {
                    if seen.insert(key) {
                        frontier.push_back(key);
                    }

                    if seen.insert(value) {
                        frontier.push_back(value);
                    }
                },
                Format::Struct { fields } | Format::Variants { variants: fields, .. } => {
                    for &field in fields.items() {
                        if seen.insert(field.1) {
                            frontier.push_back(field.1);
                        }
                    }
                },
                _ => (),
            }
        }

        f.entry(&"symbols", &self.symbols);
        f.finish()
    }
}

impl Index<SymbolId> for SchemaFormats<'_> {
    type Output = str;

    fn index(&self, index: SymbolId) -> &Self::Output {
        &self.symbols[index.0 as usize]
    }
}

#[cfg(test)]
mod tests {
    use crate::schema::formats::{FormatStorage, Format, SchemaFormats};
    use std::io::Cursor;

    #[test]
    fn test_empty_format_roundtrip() {
        test_format_roundtrip(&SchemaFormats::new());
    }

    #[test]
    fn test_unit_format_roundtrip() {
        let mut f = SchemaFormats::new();
        f.make_format(Format::Unit);
        test_format_roundtrip(&f);
    }

    fn test_format_roundtrip(format: &SchemaFormats) {
        let mut bytes = Vec::new();
        format.write_into(&mut Cursor::new(&mut bytes)).unwrap();
        println!("Format: {format:#?}");

        println!("Serialized into: {bytes:02X?}");
        println!("As words: {:08X?}", bytemuck::cast_slice::<_, u32>(&bytes));
        let storage = FormatStorage::read_from(&mut Cursor::new(&bytes)).unwrap();
        let new_format = SchemaFormats::read_from(&storage).unwrap();

        assert_eq!(*format, new_format);
    }
}
