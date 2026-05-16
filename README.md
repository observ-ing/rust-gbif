# gbif

Rust client for the [GBIF](https://www.gbif.org/) Occurrence and Species
(Checklistbank) APIs, generated at build time from the official OpenAPI
specifications published at <https://techdocs.gbif.org/openapi/>.

[![CI](https://github.com/observ-ing/rust-gbif/actions/workflows/ci.yml/badge.svg)](https://github.com/observ-ing/rust-gbif/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/gbif.svg)](https://crates.io/crates/gbif)
[![Docs.rs](https://docs.rs/gbif/badge.svg)](https://docs.rs/gbif)

## Modules

- [`occurrence`](https://docs.rs/gbif/latest/gbif/occurrence/) — the GBIF
  Occurrence API (search, downloads, maps).
- [`checklistbank`](https://docs.rs/gbif/latest/gbif/checklistbank/) — the
  GBIF Species / Checklistbank API (taxonomic names, name matching, name
  usages).

Each module exposes a `Client` struct plus all request/response types
defined by the upstream spec. The default base URL points at the public
GBIF production endpoint (<https://api.gbif.org/v1>); pass any other URL
to `occurrence::Client::new` / `checklistbank::Client::new` to talk to a
staging environment.

## Example

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let occ = gbif::occurrence::Client::new(gbif::DEFAULT_BASE_URL);
    let empty = gbif::occurrence::types::CountQuery(Default::default());

    let count = occ
        .get_occurrence_count(
            &empty,
            None, None, None, None, None, None, None, None,
            Some("6"), // taxon_key — kingdom Plantae
            None, None,
        )
        .await?
        .into_inner();

    println!("Plantae occurrences in GBIF: {count}");
    Ok(())
}
```

See [`examples/search_occurrences.rs`](examples/search_occurrences.rs)
for the runnable form (`cargo run --example search_occurrences`).

## License

Dual-licensed under either of

- [Apache License, Version 2.0](https://www.apache.org/licenses/LICENSE-2.0)
- [MIT license](https://opensource.org/licenses/MIT)

at your option.
