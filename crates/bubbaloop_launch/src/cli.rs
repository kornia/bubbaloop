//! Command-line interface for bubbaloop-launch

use argh::FromArgs;
use std::collections::HashMap;

/// ROS2-inspired launch system for bubbaloop services
#[derive(FromArgs, Debug)]
pub struct LaunchArgs {
    /// path to the launch file (default: launch/default.launch.yaml)
    #[argh(positional, default = "String::from(\"launch/default.launch.yaml\")")]
    pub launch_file: String,

    /// override launch arguments (format: key:=value)
    #[argh(option, short = 'a', from_str_fn(parse_arg_override))]
    pub arg: Vec<(String, String)>,

    /// only launch nodes in these groups (comma-separated)
    #[argh(option, short = 'g', from_str_fn(parse_groups))]
    pub groups: Option<Vec<String>>,

    /// explicitly enable these nodes (comma-separated)
    #[argh(option, from_str_fn(parse_nodes))]
    pub enable: Option<Vec<String>>,

    /// explicitly disable these nodes (comma-separated)
    #[argh(option, from_str_fn(parse_nodes))]
    pub disable: Option<Vec<String>>,

    /// show launch plan without executing
    #[argh(switch)]
    pub dry_run: bool,

    /// validate launch file and exit
    #[argh(switch)]
    pub validate: bool,

    /// log level (error, warn, info, debug, trace)
    #[argh(option, short = 'l', default = "String::from(\"info\")")]
    pub log_level: String,
}

/// Parse argument override in format "key:=value"
fn parse_arg_override(s: &str) -> Result<(String, String), String> {
    let parts: Vec<&str> = s.splitn(2, ":=").collect();
    if parts.len() != 2 {
        return Err(format!(
            "Invalid argument format '{}'. Expected 'key:=value'",
            s
        ));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Parse comma-separated group list
fn parse_groups(s: &str) -> Result<Vec<String>, String> {
    Ok(s.split(',').map(|g| g.trim().to_string()).collect())
}

/// Parse comma-separated node list
fn parse_nodes(s: &str) -> Result<Vec<String>, String> {
    Ok(s.split(',').map(|n| n.trim().to_string()).collect())
}

impl LaunchArgs {
    /// Convert argument overrides to a HashMap
    pub fn arg_overrides(&self) -> HashMap<String, String> {
        self.arg.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_arg_override() {
        let result = parse_arg_override("config:=my_config.yaml");
        assert_eq!(result, Ok(("config".to_string(), "my_config.yaml".to_string())));
    }

    #[test]
    fn test_parse_arg_override_with_equals() {
        let result = parse_arg_override("url:=http://localhost:8080");
        assert_eq!(result, Ok(("url".to_string(), "http://localhost:8080".to_string())));
    }

    #[test]
    fn test_parse_arg_override_invalid() {
        let result = parse_arg_override("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_groups() {
        let result = parse_groups("core,perception,visualization");
        assert_eq!(
            result,
            Ok(vec![
                "core".to_string(),
                "perception".to_string(),
                "visualization".to_string()
            ])
        );
    }
}
