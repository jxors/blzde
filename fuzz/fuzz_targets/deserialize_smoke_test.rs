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

fuzz_target!(|data: &[u8]| {
    // Deserialize should never crash
    blzde::from_slice::<EverythingEnum>(data).ok();
});
