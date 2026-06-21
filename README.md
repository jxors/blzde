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

# License
`blzde` is [licensed under the MPLv2.0](./LICENSE).