use serde::Deserialize;
use std::{collections::HashMap, fs};

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RestartPolicy {
    Always,
    Never,
    Unexpected,
}

fn default_exitcodes() -> OneOrMany<u32> { OneOrMany::One(0) }
fn default_autostart() -> bool { true }
fn default_autorestart() -> RestartPolicy { RestartPolicy::Never }


#[derive(Debug, Deserialize, Clone)]
pub struct ProgramConfig {
    pub cmd: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub numprocs: usize,
    #[serde(default)]
    pub umask: Option<String>,
    pub workingdir: Option<String>,
    #[serde(default = "default_autostart")]
    pub autostart: bool,
    #[serde(default = "default_autorestart")]
    pub autorestart: RestartPolicy,
    #[serde(default = "default_exitcodes")]
    pub exitcodes: OneOrMany<u32>,
    #[serde(default)]
    pub startretries: usize,
    #[serde(default)]
    pub starttime: usize,
    #[serde(default)]
    pub stopsignal: String,
    #[serde(default)]
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
    @@@
    @parser();
    . Reads the content of config.yml into a String. Any I/O error (file not found, permission denied, etc.) is returned as an Err.
    . Hands the raw YAML text to serde_yaml, which parses and to map it into config struct. If the YAML is malformed, an error is returned.
*/
pub fn parser(path: &str) -> Result<Config, Box<dyn std::error::Error>> {
    let yaml_file = fs::read_to_string(path)?;
    let parsed_config: Config = serde_yaml::from_str(&yaml_file)?;
    Ok(parsed_config)
}
