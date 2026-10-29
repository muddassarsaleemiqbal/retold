//! Stage dependencies and invalidation. Images and voice are independent after their
//! prerequisites, while edit waits for both branches.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::EngineError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Graph {
    pub dependencies: BTreeMap<String, BTreeSet<String>>,
}

impl Graph {
    #[must_use]
    pub fn stages() -> Self {
        Self {
            dependencies: [
                ("research", vec![]),
                ("script", vec!["research"]),
                ("storyboard", vec!["script"]),
                ("characters", vec!["storyboard"]),
                ("images", vec!["characters"]),
                ("voice", vec!["script"]),
                ("captions", vec!["voice"]),
                ("depth", vec!["images"]),
                ("voices", vec!["voice", "characters"]),
                ("mouths", vec!["images", "voices"]),
                ("shots", vec!["depth", "mouths"]),
                ("edit", vec!["images", "captions", "shots"]),
                ("deliver", vec!["edit"]),
                ("metrics", vec!["deliver"]),
            ]
            .into_iter()
            .map(|(id, deps)| (id.into(), deps.into_iter().map(str::to_owned).collect()))
            .collect(),
        }
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        for (id, deps) in &self.dependencies {
            if deps.iter().any(|dep| !self.dependencies.contains_key(dep)) {
                return Err(EngineError::Setup(format!(
                    "task `{id}` has an unknown dependency"
                )));
            }
        }
        let mut completed = BTreeSet::new();
        loop {
            let ready = self.ready(&completed);
            if ready.is_empty() {
                break;
            }
            completed.extend(ready);
        }
        if completed.len() != self.dependencies.len() {
            return Err(EngineError::Setup(
                "task graph contains a dependency cycle".into(),
            ));
        }
        Ok(())
    }

    /// Every node that can run now. Sorting makes execution and previews deterministic.
    #[must_use]
    pub fn ready(&self, completed: &BTreeSet<String>) -> Vec<String> {
        self.dependencies
            .iter()
            .filter(|(id, deps)| !completed.contains(*id) && deps.is_subset(completed))
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// `targets` and every stage they depend on, transitively.
    #[must_use]
    pub fn prerequisites(&self, targets: &[&str]) -> BTreeSet<String> {
        let mut needed: BTreeSet<String> = targets.iter().map(|&stage| stage.to_owned()).collect();
        loop {
            let before = needed.len();
            let deeper: Vec<String> = needed
                .iter()
                .filter_map(|stage| self.dependencies.get(stage))
                .flatten()
                .cloned()
                .collect();
            needed.extend(deeper);
            if needed.len() == before {
                return needed;
            }
        }
    }

    #[must_use]
    pub fn affected(&self, changed: &str) -> BTreeSet<String> {
        let mut affected = BTreeSet::from([changed.to_owned()]);
        loop {
            let before = affected.len();
            for (id, deps) in &self.dependencies {
                if !deps.is_disjoint(&affected) {
                    affected.insert(id.clone());
                }
            }
            if affected.len() == before {
                return affected;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_branches_are_ready_and_invalidate_only_descendants() {
        let graph = Graph::stages();
        graph.validate().unwrap();
        let complete = ["research", "script", "storyboard", "characters"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        assert_eq!(graph.ready(&complete), vec!["images", "voice"]);
        assert_eq!(
            graph.affected("voice"),
            [
                "voice", "captions", "voices", "mouths", "shots", "edit", "deliver", "metrics"
            ]
            .into_iter()
            .map(str::to_owned)
            .collect()
        );
        assert!(!graph.affected("images").contains("voice"));
    }

    #[test]
    fn cycles_and_unknown_dependencies_are_rejected() {
        let mut graph = Graph::stages();
        graph
            .dependencies
            .get_mut("research")
            .unwrap()
            .insert("edit".into());
        assert!(graph.validate().is_err());
        graph.dependencies.get_mut("research").unwrap().clear();
        graph
            .dependencies
            .get_mut("edit")
            .unwrap()
            .insert("missing".into());
        assert!(graph.validate().is_err());
    }
}
