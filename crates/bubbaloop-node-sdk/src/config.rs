use std::path::Path;

/// Load and deserialize a YAML config file.
pub fn load_config<C: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<C> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read config '{}': {}", path.display(), e))?;
    let config: C = serde_yaml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse config '{}': {}", path.display(), e))?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct TestConfig {
        name: String,
        rate_hz: f64,
    }

    #[test]
    fn test_load_valid_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "name: test\nrate_hz: 10.0\n").unwrap();
        let config: TestConfig = load_config(&path).unwrap();
        assert_eq!(config.name, "test");
        assert!((config.rate_hz - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_load_missing_file() {
        let result: anyhow::Result<TestConfig> = load_config(Path::new("/nonexistent.yaml"));
        assert!(result.is_err());
    }

    #[test]
    fn test_load_invalid_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.yaml");
        std::fs::write(&path, "not: [valid: yaml: {{").unwrap();
        let result: anyhow::Result<TestConfig> = load_config(&path);
        assert!(result.is_err());
    }
}
