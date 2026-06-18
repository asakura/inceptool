//! Validates the embedded `src/config/base.toml` is well-formed TOML at
//! compile time, so a syntax error in it fails the build rather than
//! surfacing only as a runtime `ConfigError::EmbeddedConfig` on every hook
//! invocation (see `src/config/raw_config.rs`'s `parse_base_config`).

use std::error::Error;
use std::fs;

fn main() -> Result<(), Box<dyn Error>> {
    const BASE_CONFIG_PATH: &str = "src/config/base.toml";

    println!("cargo::rerun-if-changed={BASE_CONFIG_PATH}");

    let base_toml = fs::read_to_string(BASE_CONFIG_PATH)?;
    toml::from_str::<toml::Value>(&base_toml)?;

    Ok(())
}
