//! Dependency graph construction and topological sorting

use crate::config::{DependencyCondition, LaunchFile, NodeConfig};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet, VecDeque};

/// A node in the dependency graph with resolved configuration
#[derive(Debug, Clone)]
pub struct ResolvedNode {
    /// Node name
    pub name: String,
    /// Resolved configuration
    pub config: NodeConfig,
    /// Direct dependencies
    pub dependencies: Vec<(String, DependencyCondition)>,
    /// Whether this node is enabled
    pub enabled: bool,
}

/// Dependency graph for launch nodes
#[derive(Debug)]
pub struct DependencyGraph {
    /// Nodes in topological order
    pub nodes: Vec<ResolvedNode>,
    /// Map from node name to index
    pub index_map: HashMap<String, usize>,
}

impl DependencyGraph {
    /// Build a dependency graph from a launch file
    pub fn build(
        launch_file: &LaunchFile,
        enabled_nodes: &HashSet<String>,
    ) -> Result<Self, DependencyError> {
        // First, collect all enabled nodes
        let mut resolved_nodes: IndexMap<String, ResolvedNode> = IndexMap::new();

        for (name, config) in &launch_file.nodes {
            if !enabled_nodes.contains(name) {
                continue;
            }

            let dependencies: Vec<(String, DependencyCondition)> = config
                .depends_on
                .iter()
                .map(|dep| (dep.node_name().to_string(), dep.condition()))
                .collect();

            resolved_nodes.insert(
                name.clone(),
                ResolvedNode {
                    name: name.clone(),
                    config: config.clone(),
                    dependencies,
                    enabled: true,
                },
            );
        }

        // Validate dependencies - all deps must be in enabled set
        for (name, node) in &resolved_nodes {
            for (dep_name, _) in &node.dependencies {
                if !resolved_nodes.contains_key(dep_name) {
                    // Check if the dependency exists but is disabled
                    if launch_file.nodes.contains_key(dep_name) {
                        return Err(DependencyError::DisabledDependency {
                            node: name.clone(),
                            dependency: dep_name.clone(),
                        });
                    }
                    return Err(DependencyError::UnknownDependency {
                        node: name.clone(),
                        dependency: dep_name.clone(),
                    });
                }
            }
        }

        // Perform topological sort using Kahn's algorithm
        let sorted = Self::topological_sort(&resolved_nodes)?;

        // Build index map
        let index_map: HashMap<String, usize> = sorted
            .iter()
            .enumerate()
            .map(|(i, node)| (node.name.clone(), i))
            .collect();

        Ok(Self {
            nodes: sorted,
            index_map,
        })
    }

    /// Topological sort using Kahn's algorithm
    fn topological_sort(
        nodes: &IndexMap<String, ResolvedNode>,
    ) -> Result<Vec<ResolvedNode>, DependencyError> {
        // Build adjacency list and in-degree count
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();

        for (name, node) in nodes {
            in_degree.entry(name.clone()).or_insert(0);
            dependents.entry(name.clone()).or_insert_with(Vec::new);

            for (dep_name, _) in &node.dependencies {
                *in_degree.entry(name.clone()).or_insert(0) += 1;
                dependents
                    .entry(dep_name.clone())
                    .or_insert_with(Vec::new)
                    .push(name.clone());
            }
        }

        // Start with nodes that have no dependencies
        let mut queue: VecDeque<String> = in_degree
            .iter()
            .filter(|(_, &degree)| degree == 0)
            .map(|(name, _)| name.clone())
            .collect();

        let mut sorted: Vec<ResolvedNode> = Vec::with_capacity(nodes.len());

        while let Some(name) = queue.pop_front() {
            if let Some(node) = nodes.get(&name) {
                sorted.push(node.clone());

                // Reduce in-degree for dependents
                if let Some(deps) = dependents.get(&name) {
                    for dep_name in deps {
                        if let Some(degree) = in_degree.get_mut(dep_name) {
                            *degree -= 1;
                            if *degree == 0 {
                                queue.push_back(dep_name.clone());
                            }
                        }
                    }
                }
            }
        }

        // Check for cycles
        if sorted.len() != nodes.len() {
            // Find nodes involved in cycle
            let sorted_names: HashSet<_> = sorted.iter().map(|n| &n.name).collect();
            let cycle_nodes: Vec<_> = nodes
                .keys()
                .filter(|name| !sorted_names.contains(name))
                .cloned()
                .collect();

            return Err(DependencyError::CyclicDependency(cycle_nodes));
        }

        Ok(sorted)
    }

    /// Get nodes that depend on a given node
    pub fn dependents(&self, node_name: &str) -> Vec<&ResolvedNode> {
        self.nodes
            .iter()
            .filter(|node| {
                node.dependencies
                    .iter()
                    .any(|(dep, _)| dep == node_name)
            })
            .collect()
    }

    /// Get the launch order (reverse of shutdown order)
    pub fn launch_order(&self) -> impl Iterator<Item = &ResolvedNode> {
        self.nodes.iter()
    }

    /// Get the shutdown order (reverse of launch order)
    pub fn shutdown_order(&self) -> impl Iterator<Item = &ResolvedNode> {
        self.nodes.iter().rev()
    }
}

/// Errors that can occur when building the dependency graph
#[derive(Debug, thiserror::Error)]
pub enum DependencyError {
    #[error("Node '{node}' depends on unknown node '{dependency}'")]
    UnknownDependency { node: String, dependency: String },

    #[error("Node '{node}' depends on disabled node '{dependency}'")]
    DisabledDependency { node: String, dependency: String },

    #[error("Cyclic dependency detected involving nodes: {}", .0.join(", "))]
    CyclicDependency(Vec<String>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LaunchFile;

    #[test]
    fn test_simple_dependency_order() {
        let yaml = r#"
nodes:
  a:
    executable: "bin/a"
  b:
    executable: "bin/b"
    depends_on:
      - a
  c:
    executable: "bin/c"
    depends_on:
      - b
"#;
        let launch_file = LaunchFile::from_yaml(yaml).unwrap();
        let enabled: HashSet<_> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let graph = DependencyGraph::build(&launch_file, &enabled).unwrap();

        let order: Vec<_> = graph.launch_order().map(|n| n.name.as_str()).collect();
        assert_eq!(order, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_diamond_dependency() {
        let yaml = r#"
nodes:
  a:
    executable: "bin/a"
  b:
    executable: "bin/b"
    depends_on:
      - a
  c:
    executable: "bin/c"
    depends_on:
      - a
  d:
    executable: "bin/d"
    depends_on:
      - b
      - c
"#;
        let launch_file = LaunchFile::from_yaml(yaml).unwrap();
        let enabled: HashSet<_> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        let graph = DependencyGraph::build(&launch_file, &enabled).unwrap();

        let order: Vec<_> = graph.launch_order().map(|n| n.name.as_str()).collect();
        // a must come first, d must come last, b and c can be in any order
        assert_eq!(order[0], "a");
        assert_eq!(order[3], "d");
        assert!(order[1] == "b" || order[1] == "c");
        assert!(order[2] == "b" || order[2] == "c");
    }

    #[test]
    fn test_cyclic_dependency_detection() {
        let yaml = r#"
nodes:
  a:
    executable: "bin/a"
    depends_on:
      - c
  b:
    executable: "bin/b"
    depends_on:
      - a
  c:
    executable: "bin/c"
    depends_on:
      - b
"#;
        let launch_file = LaunchFile::from_yaml(yaml).unwrap();
        let enabled: HashSet<_> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let result = DependencyGraph::build(&launch_file, &enabled);

        assert!(matches!(result, Err(DependencyError::CyclicDependency(_))));
    }

    #[test]
    fn test_disabled_dependency_error() {
        let yaml = r#"
nodes:
  a:
    executable: "bin/a"
  b:
    executable: "bin/b"
    depends_on:
      - a
"#;
        let launch_file = LaunchFile::from_yaml(yaml).unwrap();
        // Only enable b, not a
        let enabled: HashSet<_> = ["b"].iter().map(|s| s.to_string()).collect();
        let result = DependencyGraph::build(&launch_file, &enabled);

        assert!(matches!(
            result,
            Err(DependencyError::DisabledDependency { .. })
        ));
    }
}
