use juniper::{EmptyMutation, EmptySubscription, IntrospectionFormat};
use std::fs::File;
use std::io::prelude::*;

#[path = "src/lib.rs"]
mod lib;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // --- OLD JUNIPER STUFF ---
    // let (res, _errors) = juniper::introspect(
    //     &lib::Schema::new(
    //         lib::ItemList::new(),
    //         lib::Queue::new(),
    //         lib::Subscription::new(),
    //     ),
    //     &(),
    //     IntrospectionFormat::default(),
    // )
    // .unwrap();
    // let mut file = File::create("src/schema.json").unwrap();
    // let json_result = serde_json::to_string_pretty(&res).unwrap();
    // file.write_all(json_result.as_bytes()).unwrap();

    let schema = lib::Schema::new(
        lib::ItemList::new(),
        lib::Mutation::new(),
        lib::Subscription::new(),
    );
    let schema_str = schema.as_schema_language();

    let mut file = File::create("src/schema.graphql").unwrap();
    file.write_all(schema_str.as_bytes()).unwrap();

    // --- NEW TONIC STUFF ---
    tonic_build::compile_protos("proto/crabidy.proto")?;
    Ok(())
}
