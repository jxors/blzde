#![no_main]

use std::collections::HashMap;

use libfuzzer_sys::fuzz_target;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
enum EverythingEnum {
    Unit,
    NewType(EverythingStruct),
    Tuple(Box<EverythingEnum>, Box<EverythingEnum>, Option<Box<EverythingEnum>>),
    Struct {
        a: Box<EverythingEnum>,
        b: Box<EverythingEnum>,
        c: Vec<EverythingEnum>,
        d: Option<Box<EverythingEnum>>,
    },
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct EverythingStruct {
    unit: (),
    boolean: bool,
    int: i128,
    uint: u128,
    float: f64,
    string: String,
    bytes: Vec<u8>,
    option: Option<String>,
    tuple: (u32, bool),
    vec: Vec<i64>,
    map: HashMap<String, u64>,
    nested: Vec<EverythingEnum>,
    enumeration: Box<EverythingEnum>,
}

// Ser/de roundtrips should give the same results
fuzz_target!(|data: &[u8]| {
    let data1 = blzde::from_slice::<EverythingEnum>(data).ok();
    let serialized = blzde::to_vec(&data1);
    let data2 = blzde::from_slice::<EverythingEnum>(&serialized).ok();

    assert_eq!(data1, data2);
});
