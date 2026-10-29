//! Version-pinned benchmark suites and hand-labelled implementation locations.

use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Suite {
    pub name: String,
    pub repository: String,
    pub commit: String,
    pub queries: Vec<QueryCase>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct QueryCase {
    pub id: String,
    pub query: String,
    pub literal_query: String,
    pub symbol_query: String,
    pub expected: Vec<ExpectedLocation>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExpectedLocation {
    pub path: String,
    pub line: usize,
}

pub fn load_suites(directory: &Path) -> Result<Vec<Suite>, String> {
    let mut paths = fs::read_dir(directory)
        .map_err(|error| format!("read cases directory {}: {error}", directory.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect::<Vec<_>>();
    paths.sort();
    let mut suites = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = fs::read(&path)
            .map_err(|error| format!("read benchmark suite {}: {error}", path.display()))?;
        let suite = serde_json::from_slice::<Suite>(&bytes)
            .map_err(|error| format!("parse benchmark suite {}: {error}", path.display()))?;
        validate_suite(&suite)?;
        suites.push(suite);
    }
    if suites.is_empty() {
        return Err(format!(
            "no benchmark suites found in {}",
            directory.display()
        ));
    }
    Ok(suites)
}

fn validate_suite(suite: &Suite) -> Result<(), String> {
    if suite.name.trim().is_empty()
        || suite.repository.trim().is_empty()
        || suite.commit.trim().is_empty()
        || suite.queries.is_empty()
    {
        return Err(format!("suite {:?} is missing required fields", suite.name));
    }
    for query in &suite.queries {
        if query.id.trim().is_empty()
            || query.query.trim().is_empty()
            || query.literal_query.trim().is_empty()
            || query.symbol_query.trim().is_empty()
            || query.expected.is_empty()
        {
            return Err(format!(
                "suite {} contains an incomplete query case",
                suite.name
            ));
        }
        if query
            .expected
            .iter()
            .any(|location| location.path.trim().is_empty() || location.line == 0)
        {
            return Err(format!(
                "suite {} query {} contains an invalid expected location",
                suite.name, query.id
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_locations_without_a_line_number() {
        let suite = Suite {
            name: "fixture".into(),
            repository: "https://example.test/repo".into(),
            commit: "abc".into(),
            queries: vec![QueryCase {
                id: "q1".into(),
                query: "find it".into(),
                literal_query: "find_it result value".into(),
                symbol_query: "findIt".into(),
                expected: vec![ExpectedLocation {
                    path: "src/lib.rs".into(),
                    line: 0,
                }],
            }],
        };
        assert!(validate_suite(&suite).is_err());
    }
}
