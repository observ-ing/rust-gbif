//! Rust client for the GBIF web services.
//!
//! Two clients are generated at build time from the OpenAPI specifications
//! published at <https://techdocs.gbif.org/openapi/>:
//!
//! - [`occurrence`] — the GBIF Occurrence API (search, downloads, maps).
//! - [`checklistbank`] — the GBIF Species / Checklistbank API (taxonomic
//!   names, name matching, name usages).
//!
//! Each module exposes a `Client` struct plus all request/response types
//! defined by the upstream spec. The default base URL points at the public
//! GBIF production endpoint (<https://api.gbif.org/v1>); pass any other URL
//! to [`occurrence::Client::new`] / [`checklistbank::Client::new`] to talk
//! to a staging environment.
//!
//! ```no_run
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let client = gbif_api::occurrence::Client::new("https://api.gbif.org/v1");
//! let response = client.search_occurrence().limit(5).send().await?;
//! println!("matched {} records", response.count.unwrap_or(0));
//! # Ok(()) }
//! ```

#![allow(clippy::all)]
#![allow(rustdoc::broken_intra_doc_links)]
#![allow(rustdoc::bare_urls)]

/// The official base URL of the GBIF v1 API.
pub const DEFAULT_BASE_URL: &str = "https://api.gbif.org/v1";

/// Re-export of the `uuid::Uuid` type used throughout the generated clients
/// (dataset keys, organization keys, network keys, …). Re-exported so
/// downstream callers can construct/parse these IDs without a separate
/// `uuid` crate dependency.
pub use uuid::Uuid;

/// Generated client for the GBIF Occurrence API.
pub mod occurrence {
    include!(concat!(env!("OUT_DIR"), "/occurrence.rs"));
}

/// Generated client for the GBIF Species / Checklistbank API.
pub mod checklistbank {
    include!(concat!(env!("OUT_DIR"), "/checklistbank.rs"));
}
