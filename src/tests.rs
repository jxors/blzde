use crate::{de::Deserializer, schema::Schema, ser::Serializer};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{collections::HashMap, fmt::Debug, io::Cursor};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
struct AllPrimitives {
    unit: (),
    f0: bool,
    f1: u8,
    f2: i8,
    f3: u16,
    f4: i16,
    f5: u32,
    f6: i32,
    f7: u64,
    f8: i64,
    f9: u128,
    f10: i128,
    f11: f32,
    f12: f64,
    f13: char,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
struct UnitStruct;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
struct NewtypeStruct(u8);

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
struct Bytes {
    #[serde(with = "serde_bytes")]
    bytes: Vec<u8>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Triangle {
    v0: Vector3,
    v1: Vector3,
    v2: Vector3,
    normal: Vector3,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Vector3 {
    x: u32,
    y: u32,

    #[serde(default)]
    z: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Vector2 {
    x: u32,
    y: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Mesh {
    triangles: Vec<Triangle>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct OptionalFields {
    a: u64,
    b: Option<u64>,
    c: Option<Vector3>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
enum Choice {
    Default,
    NewType(bool),
    Tuple(i16, u64),
    Struct { value_a: u64, value_b: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct StringFields {
    pub identity: String,
    pub userid: String,
    pub date: String,
    pub request: String,
    pub code: u16,
    pub size: u64,
}

#[test]
fn test_field_remove() {
    test_serde_transmute(&Vector3 { x: 10, y: 20, z: 30 }, Vector2 { x: 10, y: 20 });
}

#[test]
fn test_default_field_added() {
    test_serde_transmute(&Vector2 { x: 10, y: 20 }, Vector3 { x: 10, y: 20, z: 0 });
}

#[test]
fn test_all_primitives_roundtrip() {
    test_serde_roundtrip(&AllPrimitives {
        unit: (),
        f0: true,
        f1: 123,
        f2: 123,
        f3: 123,
        f4: 123,
        f5: 123,
        f6: 123,
        f7: 123,
        f8: 123,
        f9: 123,
        f10: 123,
        f11: 123.,
        f12: 123.,
        f13: 'a',
    });
}

#[test]
fn test_map_roundtrip() {
    let mut m = HashMap::new();
    m.insert(vec![0u8, 1, 2], String::from("Hello, World"));
    m.insert(vec![123], String::from("Test"));
    m.insert(vec![], String::from("Life is like a hurricane"));
    test_serde_roundtrip(&m);
}

#[test]
fn test_bytes_roundtrip() {
    test_serde_roundtrip(&Bytes {
        bytes: vec![0u8, 1, 2, 3, 4, 5],
    });
}

#[test]
fn test_struct_edge_cases_roundtrip() {
    test_serde_roundtrip(&UnitStruct);
    test_serde_roundtrip(&NewtypeStruct(0));
}

#[test]
fn test_triangle_roundtrip() {
    test_serde_roundtrip(&Mesh {
        triangles: vec![
            Triangle::default(),
            Triangle::default(),
            Triangle::default(),
            Triangle {
                v0: Vector3 {
                    x: 0x11223344,
                    y: 0x44556677,
                    z: 0x3333333,
                },
                ..Default::default()
            },
        ],
    });
}

#[test]
fn test_optional_fields_roundtrip() {
    test_serde_roundtrip(&vec![
        OptionalFields {
            a: 10,
            b: None,
            c: Some(Vector3::default()),
        },
        OptionalFields { a: 10, b: None, c: None },
        OptionalFields {
            a: 10,
            b: Some(20),
            c: Some(Vector3::default()),
        },
    ]);
}

#[test]
fn test_tuple_roundtrip() {
    test_serde_roundtrip(&([0x11223344u32; 4], Vector3::default()));
}

#[test]
fn test_nested_vec_roundtrip() {
    test_serde_roundtrip(&vec![vec![0, 1, 2], vec![3], vec![], vec![4, 5, 6, 7, 8, 9, 10]]);
}

#[test]
fn test_string_fields_roundtrip() {
    test_serde_roundtrip(&vec![StringFields {
        identity: String::from("abc"),
        userid: String::from("test"),
        date: String::from("0000-11-22 33:44:55.6677"),
        request: String::from("GET / HTTP/1.1"),
        code: 200,
        size: 1234,
    }]);
}

#[test]
fn test_enum() {
    test_serde_roundtrip(&vec![
        Choice::Default,
        Choice::Tuple(0, 1),
        Choice::NewType(false),
        Choice::Struct {
            value_a: 0xff,
            value_b: String::from("Hello, world!"),
        },
        Choice::Default,
        Choice::Default,
        Choice::Default,
    ]);
}

fn test_serde_roundtrip<T: Debug + Serialize + DeserializeOwned + PartialEq>(val: &T) {
    let schema = Schema::of(&val);
    let format = schema.to_format();
    println!("Format: {:#?}", format);

    let output = crate::to_vec(val);
    println!("Serialized: {output:02X?}");

    let new_val = crate::from_slice(&output).unwrap();
    assert_eq!(*val, new_val);
}

fn test_serde_transmute<A: Debug + Serialize, B: Debug + DeserializeOwned + PartialEq>(val: &A, expected: B) -> B {
    let output = crate::to_vec(val);
    println!("Serialized: {output:02X?}");

    let new_val: B = crate::from_slice(&output).unwrap();
    assert_eq!(new_val, expected);
    new_val
}
