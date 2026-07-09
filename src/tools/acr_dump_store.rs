use audiocontrol::helpers::security_store::SecurityStore;
use clap::Parser;
use log::{error, info};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Path to the security_store.json file
    #[clap(short, long, value_parser)]
    store_path: Option<PathBuf>,

    /// Encryption key to decrypt the store. If not provided, values will be masked.
    #[clap(short, long)]
    key: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = Args::parse();

    let store_path = args.store_path.unwrap_or_else(|| {
        info!("No store path provided, using default: secrets/security_store.json");
        PathBuf::from("secrets/security_store.json")
    });

    if !store_path.exists() {
        error!("Security store file not found at: {}", store_path.display());
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Security store file not found",
        )));
    }

    let encryption_key_to_use: Option<String> = args.key;

    if let Some(enc_key) = encryption_key_to_use {
        info!("Attempting to decrypt store with provided key...");
        match SecurityStore::initialize(&enc_key, Some(store_path.clone())) {
            Ok(_) => {
                info!("SecurityStore initialized successfully.");
                match SecurityStore::get_all_keys() {
                    Ok(keys) => {
                        if keys.is_empty() {
                            info!("Security store is empty.");
                        } else {
                            info!("Found {} keys. Decrypted values:", keys.len());
                            for key_name in keys {
                                match SecurityStore::get(&key_name) {
                                    Ok(value) => println!("{}: {}", key_name, value),
                                    Err(e) => {
                                        error!("Failed to get/decrypt key '{}': {}", key_name, e)
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to get keys from security store: {}", e);
                        println!("Could not retrieve keys. The store might be corrupted or the key incorrect.");
                        return Err(Box::new(std::io::Error::other(format!(
                            "Failed to retrieve keys from security store: {}",
                            e
                        ))));
                    }
                }
            }
            Err(e) => {
                error!("Failed to initialize SecurityStore with key: {}", e);
                println!("Could not initialize the security store. Is the key correct?");
                // Fallback to dumping raw if initialization fails with a key
                dump_raw_store(&store_path)?;
            }
        }
    } else {
        info!("No encryption key provided. Dumping keys with masked values...");
        dump_raw_store(&store_path)?;
    }

    Ok(())
}

fn extract_raw_store_keys(json_data: &Value) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let values = json_data
        .get("values")
        .and_then(|v| v.as_object())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Missing or invalid 'values' map in security store JSON",
            )
        })?;

    Ok(values.keys().cloned().collect())
}

fn dump_raw_store(store_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let content = fs::read_to_string(store_path)?;
    let json_data: Value = serde_json::from_str(&content)?;

    let keys = match extract_raw_store_keys(&json_data) {
        Ok(keys) => keys,
        Err(e) => {
            error!("Could not find 'values' map in the security store JSON: {}", e);
            println!("The store file format seems incorrect or does not contain a 'values' map.");
            return Err(e);
        }
    };

    if keys.is_empty() {
        info!("Security store (raw) is empty.");
    } else {
        info!("Found {} keys (raw). Values are masked:", keys.len());
        for key in keys {
            println!("{}: ***", key);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn regression_extract_raw_store_keys_rejects_missing_values_map() {
        let json = serde_json::json!({ "version": 1 });
        let err = extract_raw_store_keys(&json).unwrap_err();
        assert!(err.to_string().contains("Missing or invalid 'values' map"));
    }

    #[test]
    fn regression_extract_raw_store_keys_rejects_non_object_values() {
        let json = serde_json::json!({ "values": ["not", "an", "object"] });
        let err = extract_raw_store_keys(&json).unwrap_err();
        assert!(err.to_string().contains("Missing or invalid 'values' map"));
    }

    #[test]
    fn integration_extract_raw_store_keys_returns_all_keys() {
        let json = serde_json::json!({
            "values": {
                "api_key": "encrypted_1",
                "secret": "encrypted_2"
            }
        });

        let keys = extract_raw_store_keys(&json).unwrap();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"api_key".to_string()));
        assert!(keys.contains(&"secret".to_string()));
    }

    #[test]
    fn regression_dump_raw_store_errors_on_invalid_structure() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let store_path = temp_dir.path().join("security_store.json");

        fs::write(&store_path, r#"{"version":1,"last_updated":0}"#)
            .expect("Failed to write test store file");

        let result = dump_raw_store(&store_path);
        assert!(result.is_err());
    }

    #[test]
    fn integration_dump_raw_store_accepts_empty_values_map() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let store_path = temp_dir.path().join("security_store.json");

        fs::write(&store_path, r#"{"values":{},"version":1,"last_updated":0}"#)
            .expect("Failed to write test store file");

        let result = dump_raw_store(&store_path);
        assert!(result.is_ok());
    }
}
