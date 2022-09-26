//! Pickle chin ahh boi

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use regex::Regex;
use lazy_static::lazy_static;

use super::Row;

lazy_static! {
    // Example input: {% include "included.html" %}
    // The first capture: included.html
    static ref TERA_INCLUDE_REGEXP: Regex = Regex::new(r#"\{% include "(.*?)"(?: ignore missing |\s)%\}"#).unwrap();
    // Example input: {% import "macros.html" as macros %}
    // The first capture: macros.html
    static ref TERA_IMPORT_REGEXP: Regex = Regex::new(r#"\{% import "(.*?)" as .* %\}"#).unwrap();
    // Example input: {% extends "base.html" %}
    // The first capture: base.html
    static ref TERA_EXTENDS_REGEXP: Regex = Regex::new(r#"\{% extends "(.*?)" %\}"#).unwrap();
}

#[derive(Ord, PartialOrd, Debug, Clone, Eq, PartialEq, Hash)]
struct DependencyName<'a>(&'a str);

#[derive(Ord, PartialOrd, Debug, Clone, Eq, PartialEq, Hash)]
struct DependencyId<'a>(&'a str); 

type DependencySet<'a>  = Vec<DependencyName<'a>>;
type DependencyMap<'a> = HashMap<DependencyName<'a>, DependencySet<'a>>;

pub fn compute_id_map<'a>(templates: &'a [Row]) -> HashMap<String, String> {
    // We need two different dependency maps to work around mutable reference exclusivity.
    // I think this is low-to-zero cost, since map access is ~O(1) and they only contain references.
    let mut direct_deps = DependencyMap::new();
    let mut trans_deps = DependencyMap::new();

    // This lets us map names back to IDs for hashing once dependency sets are built.
    let mut id_map = HashMap::<DependencyName, DependencyId>::new();
    // Return type - key is cloned from `templates,` while the value is a formatted hash fn output.
    let mut result_map = HashMap::<String, String>::new();

    // For each Row in templates:
    // 1. Get its template-dir-relative path.
    // 2. Find its direct dependencies using regexp 
    // 3. Insert its name and dependencies into direct_deps
    // 4. Insert its name and ID into id_map
    for row in templates {
        let trimmed_path = row.path
            .trim_start_matches(crate::prepare::SITE_SRC_DIRECTORY)
            .trim_start_matches("templates/");
        
        let d_deps = find_direct_dependencies(&row);
        log::trace!("Matched template {} for direct dependencies, found {} results.", trimmed_path, d_deps.len());

        direct_deps.insert(DependencyName(trimmed_path), d_deps);
        id_map.insert(DependencyName(trimmed_path), DependencyId(&row.id));
    }

    // For each key in id_map:
    // 1. Recursively look up its transitive dependencies.
    // 2. Insert the result into trans_deps.
    for v in id_map.keys() {
        let t_deps = find_transitive_dependencies(v, &direct_deps);
        log::trace!("Recursed into transitive dependencies for template {}, found {} results.", v.0, t_deps.len());
        trans_deps.insert(v.clone(), t_deps);
    }

    // For each key in id_map:
    // 1. Fetch its corresponding value (ID).
    // 2. Fetch its direct and transitive dependencies, and merge their slices into a single Vec of refs.
    // 3. Spin up a SeaHash instance and feed in own_id as well as the IDs of every dependency.
    // 4. Format hasher result as a 16-character hex string and insert it into result_map.
    for v in id_map.keys() {
        let own_id = id_map.get(v).unwrap();
        let deps = {
            let d = direct_deps.get(v).unwrap();
            let t = trans_deps.get(v).unwrap();
            let mut v = Vec::new();
            v.extend_from_slice(d);
            v.extend_from_slice(t);
            v
        };

        let mut hasher = seahash::SeaHasher::default();
        own_id.hash(&mut hasher);
        for d in &deps {
            let id = id_map.get(d);
            id.hash(&mut hasher);
        }
        let t_id = format!("{:016x}", hasher.finish());
        log::trace!("Finished evaluating dependencies for template {} - {} total, ID {}", v.0, deps.len(), t_id);
        result_map.insert(v.0.to_owned(), t_id);
    }

    result_map
}

/// Parse the contents of the given [`Row`] for its direct dependencies using the `TERA_INCLUDE_*` regular expressions.
fn find_direct_dependencies(item: &Row) -> DependencySet {
    let mut dependencies: Vec<&str> = Vec::new();
    
    let mut capture = |regexp: &Regex | {
        regexp.captures_iter(&item.contents)
            .filter_map(|cap| cap.get(1) )
            .map(|found| found.as_str() )
            .map(|text| dependencies.push(text) )
            .for_each(drop)
    };

    capture(&TERA_INCLUDE_REGEXP);
    capture(&TERA_IMPORT_REGEXP);
    capture(&TERA_EXTENDS_REGEXP);

    dependencies.sort_unstable();
    dependencies.dedup();
    dependencies.into_iter()
        .map(|x| DependencyName(x) )
        .collect()
}

/// Using the recursive [`traverse_set`] function, ascertain the deduplicated transitive dependencies of the provided dependency.
fn find_transitive_dependencies<'a>(dep: &'a DependencyName, map: &'a DependencyMap) -> DependencySet<'a> {
    let direct_deps = map.get(&dep).unwrap();

    let mut transitives = 
    direct_deps.into_iter()
        .map(|dep| traverse_set(dep, map) )
        .fold(Vec::new(), |mut acc, set| {
            for dep in set {
                acc.push(dep);
            }
            acc
        });

    transitives.sort_unstable();
    transitives.dedup();
    transitives
}

// Note: currently this is capable of blowing the stack if the input templates contain a dependency loop.
// Tera does not catch stuff like include loops during parsing, so some sort of "escape counter" might be wise.
/// Recurses into the dependencies of the given dependency, bubbling up a [`DependencySet`].
fn traverse_set<'a>(dep: &'a DependencyName, map: &'a DependencyMap) -> DependencySet<'a> {
    let deps = map.get(&dep).unwrap();

    let mut acc = deps.iter()
        .map(|dep| {
            traverse_set(dep, map)
        } )
        .fold(Vec::new(), |mut acc, set| {
            for dep in set {
                acc.push(dep);
            }
            acc
        });
    
    acc.extend_from_slice(&deps);
    acc
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn id_computation() {
        assert!(true)
    }
}