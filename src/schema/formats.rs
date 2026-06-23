use crate::schema::storage::FormatStorage;
use bytemuck::Pod;
use std::{
    collections::{HashSet, VecDeque},
    fmt::{Debug, Display},
    io::{Read, Write},
    marker::PhantomData,
    ops::Index,
    string::FromUtf8Error,
};

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
    Format(FormatParseError),

    /// Occurs when an error occurs reading or writing from the input or output.
    Io(std::io::Error),

    /// Occurs when a symbol fails to parse.
    Symbol(FromUtf8Error),

    /// Occurs when an unknown version number is encountered.
    UnknownVersion,
}

impl Display for SchemaParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchemaParseError::Io(e) => write!(f, "i/o error: {e}"),
            SchemaParseError::Symbol(e) => write!(f, "invalid symbol: {e}"),
            SchemaParseError::Format(e) => write!(f, "{e}"),
            SchemaParseError::UnknownVersion => write!(f, "unknown schema version"),
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

impl FormatId {
    pub const INVALID: FormatId = FormatId(u32::MAX);

    fn write(&self, data: &mut impl Write) -> std::io::Result<()> {
        data.write_u32(self.0)
    }

    #[inline(always)]
    fn read(read: &mut impl Read) -> Result<FormatId, SchemaParseError> {
        let id = read.read_u32()?;
        Ok(FormatId(id))
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
    U16,
    U32,
    U64,
    AdjustedU32(u64),
}

impl Primitive {
    fn write(&self, data: &mut impl Write) -> std::io::Result<()> {
        data.write_u8(match self {
            Primitive::U8 => 0,
            Primitive::U16 => 1,
            Primitive::U32 => 2,
            Primitive::U64 => 3,
            Primitive::AdjustedU32(_) => 4,
        })?;

        match self {
            Primitive::U64 | Primitive::U32 | Primitive::U16 | Primitive::U8 => (),
            Primitive::AdjustedU32(val) => data.write_u64(*val)?,
        }

        Ok(())
    }

    #[inline(always)]
    fn read(read: &mut impl Read) -> Result<Primitive, SchemaParseError> {
        Ok(match read.read_u8()? {
            0 => Primitive::U8,
            1 => Primitive::U16,
            2 => Primitive::U32,
            3 => Primitive::U64,
            4 => Primitive::AdjustedU32(read.read_u64()?),
            _ => return Err(FormatParseError::Corrupt.into()),
        })
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Format<'buf> {
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
        fields: View<'buf, FormatId>,
    },
    Map {
        len: Primitive,
        key: FormatId,
        value: FormatId,
    },
    Struct {
        fields: View<'buf, NamedFormat>,
    },
    Variants {
        variant_index: Primitive,
        variants: View<'buf, NamedFormat>,
    },
    U128,
}

impl<'buf> Format<'buf> {
    pub fn make_static(&self) -> Format<'static> {
        match *self {
            Format::Primitive(primitive) => Format::Primitive(primitive),
            Format::String { len } => Format::String { len },
            Format::Bytes { len } => Format::Bytes { len },
            Format::Option { inner } => Format::Option { inner },
            Format::Unit => Format::Unit,
            Format::Sequence { len, inner } => Format::Sequence { len, inner },
            Format::Tuple { .. } => Format::Tuple { fields: View::empty() },
            Format::Map { len, key, value } => Format::Map { len, key, value },
            Format::Struct { .. } => Format::Struct { fields: View::empty() },
            Format::Variants { variant_index, .. } => Format::Variants {
                variant_index,
                variants: View::empty(),
            },
            Format::U128 => Format::U128,
        }
    }

    fn write(&self, write: &mut impl Write) -> std::io::Result<()> {
        write.write_u8(match self {
            Format::Primitive(_) => 0,
            Format::String { .. } => 1,
            Format::Bytes { .. } => 2,
            Format::Option { .. } => 3,
            Format::Unit => 4,
            Format::Sequence { .. } => 5,
            Format::Tuple { .. } => 6,
            Format::Map { .. } => 7,
            Format::Struct { .. } => 8,
            Format::Variants { .. } => 9,
            Format::U128 => 10,
        })?;

        match self {
            Format::Primitive(len) | Format::String { len } | Format::Bytes { len } => len.write(write)?,
            Format::Option { inner } => inner.write(write)?,
            Format::Unit => (),
            Format::Sequence { len, inner } => {
                len.write(write)?;
                inner.write(write)?;
            },
            Format::Tuple { fields } => fields.write(write)?,
            Format::Map { len, key, value } => {
                len.write(write)?;
                key.write(write)?;
                value.write(write)?;
            },
            Format::Struct { fields } => fields.write(write)?,
            Format::Variants { variant_index, variants } => {
                variant_index.write(write)?;
                variants.write(write)?;
            },
            Format::U128 => (),
        }

        Ok(())
    }

    #[inline(always)]
    fn read(read: &mut impl Read, storage: &'buf FormatStorage) -> Result<Format<'buf>, SchemaParseError> {
        let kind = read.read_u8()?;
        Ok(match kind {
            0 => {
                let primitive = Primitive::read(read)?;
                Format::Primitive(primitive)
            },
            1 => {
                let len = Primitive::read(read)?;
                Format::String { len }
            },
            2 => {
                let len = Primitive::read(read)?;
                Format::Bytes { len }
            },
            3 => {
                let inner = FormatId::read(read)?;
                Format::Option { inner }
            },
            4 => Format::Unit,
            5 => {
                let len = Primitive::read(read)?;
                let inner = FormatId::read(read)?;
                Format::Sequence { len, inner }
            },
            6 => {
                let fields = View::read(read, storage)?;
                Format::Tuple { fields }
            },
            7 => {
                let len = Primitive::read(read)?;
                let key = FormatId::read(read)?;
                let value = FormatId::read(read)?;
                Format::Map { len, key, value }
            },
            8 => {
                let fields = View::read(read, storage)?;
                Format::Struct { fields }
            },
            9 => {
                let variant_index = Primitive::read(read)?;
                let variants = View::read(read, storage)?;
                Format::Variants { variant_index, variants }
            },
            10 => Format::U128,
            _ => return Err(FormatParseError::Corrupt.into()),
        })
    }
}

#[derive(Copy, Clone, PartialEq)]
pub struct View<'buf, T>(&'buf [T], PhantomData<T>);

impl<T: Pod> View<'static, T> {
    #[inline(always)]
    pub fn empty() -> Self {
        Self(&[], PhantomData)
    }
}

impl<'buf, T: Pod> View<'buf, T> {
    #[inline(always)]
    pub fn new(data: &'buf [T]) -> Self {
        Self(data, PhantomData)
    }

    #[inline(always)]
    pub fn items(&self) -> &'buf [T] {
        &self.0
    }

    fn write(&self, write: &mut impl Write) -> std::io::Result<()> {
        let len = self.0.len();
        write.write_u16(len.try_into().unwrap())?;
        write.write_all(&bytemuck::cast_slice(&self.0))?;

        Ok(())
    }

    fn read(read: &mut impl Read, storage: &'buf FormatStorage) -> Result<View<'buf, T>, SchemaParseError> {
        let len = read.read_u16()? as usize;
        let data = storage.alloc::<T, SchemaParseError>(len, |_| {
            let mut val = T::zeroed();
            read.read_exact(bytemuck::cast_slice_mut(std::slice::from_mut(&mut val)))?;
            Ok(val)
        })?;

        Ok(Self(data, PhantomData))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl<T: Pod + Debug> Debug for View<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().finish_non_exhaustive()
    }
}

#[derive(PartialEq)]
pub struct SchemaFormats<'buf> {
    formats: Vec<Format<'buf>>,
    symbols: Vec<String>,
    root: FormatId,
}

impl<'buf> SchemaFormats<'buf> {
    const SCHEMA_V0: u8 = 0;

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

    pub fn root(&self) -> Format<'buf> {
        assert_ne!(self.root, FormatId::INVALID);
        self.format(self.root)
    }

    pub fn try_get_format(&self, id: FormatId) -> Result<Format<'buf>, FormatParseError> {
        match self.formats.get(id.0 as usize) {
            Some(format) => Ok(*format),
            None => Err(FormatParseError::FormatReferenceOutOfBounds),
        }
    }

    #[inline(always)]
    pub fn format(&self, id: FormatId) -> Format<'buf> {
        self.try_get_format(id)
            .expect("format should have been fully validated after parsing")
    }

    pub fn root_id(&self) -> FormatId {
        self.root
    }

    pub fn write_into(&self, write: &mut impl Write) -> Result<(), std::io::Error> {
        write.write_u8(Self::SCHEMA_V0)?;
        write.write_u32(self.root.as_usize().try_into().unwrap())?;
        write.write_u32(self.formats.len().try_into().unwrap())?;
        for format in self.formats.iter() {
            format.write(write)?;
        }

        write.write_u32(self.symbols.len().try_into().unwrap())?;

        for symbol in self.symbols.iter() {
            write.write_u16(symbol.len().try_into().unwrap())?;
            write.write_all(symbol.as_bytes())?;
        }

        Ok(())
    }

    pub fn read_from(read: &mut impl Read, storage: &'buf FormatStorage) -> Result<SchemaFormats<'buf>, SchemaParseError> {
        let version = read.read_u8()?;
        if version != Self::SCHEMA_V0 {
            return Err(SchemaParseError::UnknownVersion);
        }
        let root = read.read_u32()?;
        let num_formats = read.read_u32()?;
        let formats = (0..num_formats)
            .map(|_| Format::read(read, storage))
            .collect::<Result<Vec<_>, _>>()?;

        let num_symbols = read.read_u32()?;
        let symbols = (0..num_symbols)
            .map(|_| {
                let len = read.read_u16()?;
                let mut data = vec![0; len as usize];
                read.read_exact(&mut data)?;

                Ok(String::from_utf8(data)?)
            })
            .collect::<Result<_, SchemaParseError>>()?;

        let result = Self {
            formats,
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

trait ReadExt {
    fn read_u64(&mut self) -> std::io::Result<u64>;
    fn read_u32(&mut self) -> std::io::Result<u32>;
    fn read_u16(&mut self) -> std::io::Result<u16>;
    fn read_u8(&mut self) -> std::io::Result<u8>;
}

impl<R: Read> ReadExt for R {
    fn read_u64(&mut self) -> std::io::Result<u64> {
        let mut buf = [0; 8];
        self.read_exact(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }

    fn read_u32(&mut self) -> std::io::Result<u32> {
        let mut buf = [0; 4];
        self.read_exact(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    fn read_u16(&mut self) -> std::io::Result<u16> {
        let mut buf = [0; 2];
        self.read_exact(&mut buf)?;
        Ok(u16::from_le_bytes(buf))
    }

    fn read_u8(&mut self) -> std::io::Result<u8> {
        let mut buf = [0; 1];
        self.read_exact(&mut buf)?;
        Ok(buf[0])
    }
}

trait WriteExt {
    fn write_u64(&mut self, val: u64) -> std::io::Result<()>;
    fn write_u32(&mut self, val: u32) -> std::io::Result<()>;
    fn write_u16(&mut self, val: u16) -> std::io::Result<()>;
    fn write_u8(&mut self, val: u8) -> std::io::Result<()>;
}

impl<W: Write> WriteExt for W {
    fn write_u64(&mut self, val: u64) -> std::io::Result<()> {
        self.write_all(&val.to_le_bytes())
    }

    fn write_u32(&mut self, val: u32) -> std::io::Result<()> {
        self.write_all(&val.to_le_bytes())
    }

    fn write_u16(&mut self, val: u16) -> std::io::Result<()> {
        self.write_all(&val.to_le_bytes())
    }

    fn write_u8(&mut self, val: u8) -> std::io::Result<()> {
        self.write_all(&val.to_le_bytes())
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
    use crate::schema::formats::{Format, FormatStorage, SchemaFormats};
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
        let storage = FormatStorage::new();
        let new_format = SchemaFormats::read_from(&mut Cursor::new(&bytes), &storage).unwrap();

        assert_eq!(*format, new_format);
    }
}
