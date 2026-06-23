use criterion::{Criterion, criterion_group, criterion_main};
use serde::{Deserialize, Serialize};
use std::{hint::black_box, time::Duration};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
struct Data {
    field1: u32,
    field2: u8,
    field3: Option<bool>,
}

fn bench_struct(c: &mut Criterion) {
    let data = vec![
        Data {
            field1: 5,
            field2: 7,
            field3: Some(true),
        };
        10_000
    ];
    let bytes = blzde::to_vec(&data);

    c.bench_function("simple_struct", |c| {
        c.iter(|| {
            let val: Vec<Data> = blzde::from_slice(black_box(&bytes)).unwrap();
            black_box(val)
        })
    });
}

fn bench_primitive_sequence(c: &mut Criterion) {
    let data: Vec<i32> = vec![0x11223344; 1_000_000];
    let bytes = blzde::to_vec(&data);

    c.bench_function("primitive_sequence", |c| {
        c.iter(|| {
            let val: Vec<i32> = blzde::from_slice(black_box(&bytes)).unwrap();
            black_box(val)
        })
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().measurement_time(Duration::from_secs(5));
    targets = bench_struct, bench_primitive_sequence
}
criterion_main!(benches);
