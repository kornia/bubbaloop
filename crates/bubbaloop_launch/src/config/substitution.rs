//! Substitution engine for $(arg), $(env), $(timestamp) patterns

use regex::{Captures, Regex};
use std::collections::HashMap;
use std::sync::LazyLock;

/// Regex for matching substitution patterns: $(type value)
static SUBSTITUTION_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$\((\w+)\s+([^)]+)\)|\$\((\w+)\)").unwrap());

/// Substitution context containing all available variables
#[derive(Debug, Clone, Default)]
pub struct SubstitutionContext {
    /// Launch file arguments
    pub args: HashMap<String, String>,
    /// Additional environment variables
    pub env: HashMap<String, String>,
}

impl SubstitutionContext {
    /// Create a new substitution context
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an argument
    pub fn with_arg(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.args.insert(name.into(), value.into());
        self
    }

    /// Add multiple arguments
    pub fn with_args(mut self, args: HashMap<String, String>) -> Self {
        self.args.extend(args);
        self
    }

    /// Add an environment variable
    pub fn with_env(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(name.into(), value.into());
        self
    }

    /// Add multiple environment variables
    pub fn with_envs(mut self, envs: HashMap<String, String>) -> Self {
        self.env.extend(envs);
        self
    }

    /// Substitute all patterns in a string
    pub fn substitute(&self, input: &str) -> Result<String, SubstitutionError> {
        let mut result = input.to_string();
        let mut last_result = String::new();

        // Iterate until no more substitutions are made (handles nested substitutions)
        let max_iterations = 10;
        let mut iterations = 0;

        while result != last_result && iterations < max_iterations {
            last_result = result.clone();
            result = self.substitute_once(&result)?;
            iterations += 1;
        }

        if iterations >= max_iterations && result.contains("$(") {
            return Err(SubstitutionError::MaxIterationsExceeded(input.to_string()));
        }

        Ok(result)
    }

    /// Perform a single pass of substitution
    fn substitute_once(&self, input: &str) -> Result<String, SubstitutionError> {
        let mut error: Option<SubstitutionError> = None;

        let result = SUBSTITUTION_PATTERN.replace_all(input, |caps: &Captures| {
            if error.is_some() {
                return String::new();
            }

            match self.resolve_capture(caps) {
                Ok(value) => value,
                Err(e) => {
                    error = Some(e);
                    String::new()
                }
            }
        });

        if let Some(e) = error {
            return Err(e);
        }

        Ok(result.into_owned())
    }

    /// Resolve a single capture group
    fn resolve_capture(&self, caps: &Captures) -> Result<String, SubstitutionError> {
        // Pattern 1: $(type value) - e.g., $(arg camera_config)
        if let (Some(subst_type), Some(value)) = (caps.get(1), caps.get(2)) {
            return self.resolve_typed(subst_type.as_str(), value.as_str().trim());
        }

        // Pattern 2: $(type) - e.g., $(timestamp)
        if let Some(subst_type) = caps.get(3) {
            return self.resolve_typed(subst_type.as_str(), "");
        }

        Err(SubstitutionError::InvalidPattern(
            caps.get(0).map(|m| m.as_str().to_string()).unwrap_or_default(),
        ))
    }

    /// Resolve a typed substitution
    fn resolve_typed(&self, subst_type: &str, value: &str) -> Result<String, SubstitutionError> {
        match subst_type {
            "arg" => self.resolve_arg(value),
            "env" => self.resolve_env(value),
            "timestamp" => Ok(self.generate_timestamp(value)),
            "date" => Ok(self.generate_date(value)),
            _ => Err(SubstitutionError::UnknownType(subst_type.to_string())),
        }
    }

    /// Resolve an argument reference
    fn resolve_arg(&self, name: &str) -> Result<String, SubstitutionError> {
        self.args
            .get(name)
            .cloned()
            .ok_or_else(|| SubstitutionError::UndefinedArg(name.to_string()))
    }

    /// Resolve an environment variable reference
    fn resolve_env(&self, name: &str) -> Result<String, SubstitutionError> {
        // First check our local env overrides
        if let Some(value) = self.env.get(name) {
            return Ok(value.clone());
        }

        // Then check system environment
        std::env::var(name).map_err(|_| SubstitutionError::UndefinedEnv(name.to_string()))
    }

    /// Generate a timestamp
    fn generate_timestamp(&self, format: &str) -> String {
        let now = chrono::Local::now();
        if format.is_empty() {
            // Default format: YYYYMMDD_HHMMSS
            now.format("%Y%m%d_%H%M%S").to_string()
        } else {
            now.format(format).to_string()
        }
    }

    /// Generate a date
    fn generate_date(&self, format: &str) -> String {
        let now = chrono::Local::now();
        if format.is_empty() {
            // Default format: YYYY-MM-DD
            now.format("%Y-%m-%d").to_string()
        } else {
            now.format(format).to_string()
        }
    }
}

/// Errors that can occur during substitution
#[derive(Debug, thiserror::Error)]
pub enum SubstitutionError {
    #[error("Unknown substitution type: {0}")]
    UnknownType(String),

    #[error("Undefined argument: {0}")]
    UndefinedArg(String),

    #[error("Undefined environment variable: {0}")]
    UndefinedEnv(String),

    #[error("Invalid substitution pattern: {0}")]
    InvalidPattern(String),

    #[error("Maximum substitution iterations exceeded for: {0}")]
    MaxIterationsExceeded(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arg_substitution() {
        let ctx = SubstitutionContext::new().with_arg("config", "my_config.yaml");

        let result = ctx.substitute("$(arg config)").unwrap();
        assert_eq!(result, "my_config.yaml");
    }

    #[test]
    fn test_env_substitution() {
        let ctx = SubstitutionContext::new().with_env("MY_VAR", "my_value");

        let result = ctx.substitute("$(env MY_VAR)").unwrap();
        assert_eq!(result, "my_value");
    }

    #[test]
    fn test_timestamp_substitution() {
        let ctx = SubstitutionContext::new();

        let result = ctx.substitute("recording_$(timestamp).mcap").unwrap();
        // Should contain timestamp pattern
        assert!(result.starts_with("recording_"));
        assert!(result.ends_with(".mcap"));
        assert!(result.len() > 20); // recording_ + timestamp + .mcap
    }

    #[test]
    fn test_multiple_substitutions() {
        let ctx = SubstitutionContext::new()
            .with_arg("prefix", "data")
            .with_arg("suffix", "log");

        let result = ctx.substitute("$(arg prefix)_$(timestamp)_$(arg suffix)").unwrap();
        assert!(result.starts_with("data_"));
        assert!(result.ends_with("_log"));
    }

    #[test]
    fn test_undefined_arg_error() {
        let ctx = SubstitutionContext::new();

        let result = ctx.substitute("$(arg undefined)");
        assert!(result.is_err());
    }

    #[test]
    fn test_nested_substitution() {
        let ctx = SubstitutionContext::new()
            .with_arg("outer", "$(arg inner)")
            .with_arg("inner", "resolved");

        let result = ctx.substitute("$(arg outer)").unwrap();
        assert_eq!(result, "resolved");
    }

    #[test]
    fn test_no_substitution_needed() {
        let ctx = SubstitutionContext::new();

        let result = ctx.substitute("plain string").unwrap();
        assert_eq!(result, "plain string");
    }
}
