# `blzde` - "<ins>bl</ins>a<ins>z</ins>ingly" fast `serde` <ins>de</ins>serialization
`blzde` is a **self-describing** serialization format for `serde` that aims for **fast deserialization** of big files, at the cost of slower serialization.
Serializing takes two steps: first, a schema format specific to the provided data is computed.
Then, the data is serialized according to this format.
This allows the actual data to be serialized without any identifiers, type markers, or other non-data.

```rust
let bytes = blzde::to_vec(&Data {
    field1: 0,
    field2: 27,
    field3: None,
}).unwrap();

let data = blzde::from_slice(&bytes).unwrap();
```

# Benchmark
This benchmark is based on [rust_serialization_benchmark](https://github.com/djkoloski/rust_serialization_benchmark).
Each row shows the *deserialization* times for four different datasets.
Only crates that support serde and use a self-describing format are shown.

| Crate | `log` | `mesh` | `minecraft_savedata` | `mk48` |
|---|--:|--:|--:|--:|
| **blzde** | **2.4620 ms** | **12.317 ms** | **2.5026 ms** | **7.8396 ms** |
| [rmp-serde 1.3.0][rmp-serde] | 3.0835 ms | 15.039 ms | 2.5811 ms | 9.6017 ms |
| [nachricht-serde 0.4.0][nachricht-serde] | 3.8647 ms | 23.692 ms | 3.3899 ms | 14.616 ms |
| [serde-brief 0.1.1][serde-brief] | 4.5388 ms | 29.015 ms | 4.7763 ms | 19.952 ms |
| [serde_cbor 0.11.2][serde_cbor] | 5.3286 ms | 35.779 ms | 4.7109 ms | 21.294 ms |
| [cbor4ii 1.0.0][cbor4ii] | 5.5056 ms | 47.811 ms | 4.5252 ms | 19.276 ms |
| [flexon 0.4.5][flexon] | 4.3801 ms | 51.445 ms | 4.3161 ms | 23.554 ms |
| [pot 3.0.1][pot] | 6.3750 ms | 65.024 ms | 5.5240 ms | 29.142 ms |
| [flexbuffers 25.2.10][flexbuffers] | 7.1275 ms | 65.566 ms | 5.8424 ms | 30.865 ms |
| [serde_json 1.0.140][serde_json] | 7.0753 ms | 100.48 ms | 6.4211 ms | 32.733 ms |
| [simd-json 0.15.1][simd-json] | 4.5302 ms | 106.44 ms | 4.1368 ms | 37.881 ms |
| [ciborium 0.2.2][ciborium] | 10.044 ms | 96.798 ms | 8.1513 ms | 42.084 ms |

# License
`blzde` is [licensed under the MPLv2.0](./LICENSE).

[cbor4ii]: https://crates.io/crates/cbor4ii/1.0.0
[ciborium]: https://crates.io/crates/ciborium/0.2.2
[flexbuffers]: https://crates.io/crates/flexbuffers/25.2.10
[flexon]: https://crates.io/crates/flexon/0.4.5
[nachricht-serde]: https://crates.io/crates/nachricht-serde/0.4.0
[pot]: https://crates.io/crates/pot/3.0.1
[rmp-serde]: https://crates.io/crates/rmp-serde/1.3.0
[serde-brief]: https://crates.io/crates/serde-brief/0.1.1
[serde_cbor]: https://crates.io/crates/serde_cbor/0.11.2
[serde_json]: https://crates.io/crates/serde_json/1.0.140
[simd-json]: https://crates.io/crates/simd-json/0.15.1