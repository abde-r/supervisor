use serde::Deserialize;
use std::{collections::HashMap, fs};

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum OneOrMany {
    One(u32),
    Many(Vec<u32>),
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RestartPolicy {
    Always,
    Never,
    Unexpected,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProgramConfig {
    pub cmd: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub numprocs: usize,
    pub umask: Option<String>,
    pub workingdir: Option<String>,
    pub autostart: bool,
    pub autorestart: RestartPolicy,
    pub exitcodes: OneOrMany,
    pub startretries: usize,
    pub starttime: usize,
    pub stopsignal: String,
    pub stoptime: usize,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub env: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub programs: HashMap<String, ProgramConfig>,
}

/*
    @Parser
    Parsing the config file into useful data
*/
pub fn parser(path: &str) -> Result<Config, Box<dyn std::error::Error>> {
    let yaml_file = fs::read_to_string(path)?;
    let parsed_config: Config = serde_yaml::from_str(&yaml_file)?;
    Ok(parsed_config)
}
