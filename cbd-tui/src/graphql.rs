#[cynic::schema_for_derives(file = r#"schema.graphql"#, module = "schema")]
pub mod queries {
    use super::schema;

    #[derive(cynic::QueryFragment, Debug)]
    #[cynic(graphql_type = "Root")]
    pub struct AllFilmsQuery {
        pub all_films: Option<FilmsConnection>,
    }

    #[derive(cynic::QueryFragment, Debug)]
    pub struct FilmsConnection {
        pub films: Option<Vec<Option<Film>>>,
    }

    #[derive(cynic::QueryFragment, Debug)]
    pub struct Film {
        pub id: cynic::Id,
        pub title: Option<String>,
    }
}

#[allow(non_snake_case, non_camel_case_types)]
mod schema {
    cynic::use_schema!(r#"schema.graphql"#);
}
