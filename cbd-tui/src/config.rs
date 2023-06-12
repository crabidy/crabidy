use crabidy_core::{
    clap::{self},
    clap_serde_derive,
    serde::Serialize,
    ClapSerde,
};

#[derive(ClapSerde, Serialize, Debug)]
#[clap(author, version, about)]
pub struct Config {
    #[clap_serde]
    #[clap(flatten)]
    pub server: ServerConfig,
}

#[derive(ClapSerde, Serialize, Debug)]
pub struct ServerConfig {
    /// Server address
    #[default("http://127.0.0.1:50051".to_string())]
    #[clap(short, long)]
    pub address: String,
}
