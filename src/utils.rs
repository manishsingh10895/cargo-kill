use std::path::Path;

use crate::ProjectType;

// Maps an npm dependency name to one or more cache directories it produces.
const NPM_FRAMEWORK_MAP: &[(&str, &[&str])] = &[
    ("next", &[".next"]),
    ("nuxt", &[".nuxt", ".output"]),
    ("nuxt3", &[".nuxt", ".output"]),
    ("@sveltejs/kit", &[".svelte-kit"]),
];

/// File at the project root that identifies the project type.
pub fn project_identifier(project_type: &ProjectType) -> &'static str {
    match project_type {
        ProjectType::Cargo => "Cargo.toml",
        ProjectType::Npm => "package.json",
    }
}

/// All target directory names that should be considered for cleanup at the
/// given project root. The first entry is always the canonical one
/// (`target` / `node_modules`); for npm, framework caches are appended based
/// on the project's `package.json` dependencies.
pub fn target_dirs_for(project_type: &ProjectType, project_root: &Path) -> Vec<&'static str> {
    match project_type {
        ProjectType::Cargo => vec!["target"],
        ProjectType::Npm => {
            let mut dirs = vec!["node_modules"];
            dirs.extend(npm_framework_targets(&project_root.join("package.json")));
            dirs
        }
    }
}

fn npm_framework_targets(package_json: &Path) -> Vec<&'static str> {
    let text = match std::fs::read_to_string(package_json) {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return vec![],
    };

    let mut out: Vec<&'static str> = vec![];
    for section in ["dependencies", "devDependencies"] {
        let Some(obj) = value.get(section).and_then(|v| v.as_object()) else {
            continue;
        };
        for (dep_name, dirs) in NPM_FRAMEWORK_MAP {
            if obj.contains_key(*dep_name) {
                for d in *dirs {
                    if !out.contains(d) {
                        out.push(*d);
                    }
                }
            }
        }
    }
    out
}
