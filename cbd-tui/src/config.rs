use std::{
    fs::{create_dir_all, read_to_string, File},
    io::Write,
    path::Path,
};

use clap_serde_derive::{
    clap::{self, Parser},
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

pub fn init() -> Config {
    if let Some(config_dir) = dirs::config_dir() {
        let dir = Path::new(&config_dir).join("crabidy");
        if !dir.is_dir() {
            create_dir_all(&dir);
        }
        let config_file_path = dir.join("cbd-tui.toml");
        if !config_file_path.is_file() {
            let config = Config::default().merge_clap();
            let content = toml::to_string_pretty(&config).expect("Could not serialize config");
            let mut config_file =
                File::create(config_file_path).expect("Could not open config file for writing");
            config_file
                .write_all(content.as_bytes())
                .expect("Failed to write to file");
            config_file.flush().ok();
            return config;
        } else {
            let content = read_to_string(config_file_path).expect("Could not read config file");
            let parsed = toml::from_str::<<Config as ClapSerde>::Opt>(&content).unwrap();
            let config: Config = Config::from(parsed).merge_clap();
            return config;
        }
    }
    Config::default().merge_clap()
}
