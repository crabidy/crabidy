#![allow(clippy::all, warnings)]
pub struct ItemList;
pub mod item_list {
    #![allow(dead_code)]
    use std::result::Result;
    pub const OPERATION_NAME: &str = "ItemList";
    pub const QUERY: &str = "query ItemList {\n  name\n}\n";
    use super::*;
    use serde::{Deserialize, Serialize};
    #[allow(dead_code)]
    type Boolean = bool;
    #[allow(dead_code)]
    type Float = f64;
    #[allow(dead_code)]
    type Int = i64;
    #[allow(dead_code)]
    type ID = String;
    #[derive(Serialize)]
    pub struct Variables;
    #[derive(Deserialize)]
    pub struct ResponseData {
        pub name: String,
    }
}
impl graphql_client::GraphQLQuery for ItemList {
    type Variables = item_list::Variables;
    type ResponseData = item_list::ResponseData;
    fn build_query(variables: Self::Variables) -> ::graphql_client::QueryBody<Self::Variables> {
        graphql_client::QueryBody {
            variables,
            query: item_list::QUERY,
            operation_name: item_list::OPERATION_NAME,
        }
    }
}
