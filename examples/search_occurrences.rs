//! Smoke test against the live GBIF API.
//!
//! Counts the number of GBIF occurrence records for the kingdom Plantae
//! (taxonKey 6) using the generated client. The endpoint returns a primitive
//! integer, which exercises the wire/auth/headers/query-params path of the
//! generated code without depending on the (large, frequently-drifting)
//! Occurrence response schema.
//!
//! Run with: `cargo run --example search_occurrences`

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let occ = gbif::occurrence::Client::new(gbif::DEFAULT_BASE_URL);
    let empty = gbif::occurrence::types::CountQuery(Default::default());

    let count = occ
        .get_occurrence_count(
            &empty,
            None,      // basis_of_record
            None,      // checklist_key
            None,      // country
            None,      // dataset_key
            None,      // is_georeferenced
            None,      // issue
            None,      // protocol
            None,      // publishing_country
            Some("6"), // taxon_key — kingdom Plantae
            None,      // type_status
            None,      // year
        )
        .await?
        .into_inner();

    println!("Plantae occurrences in GBIF: {count}");
    Ok(())
}
