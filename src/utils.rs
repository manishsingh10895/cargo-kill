use std::path::Path;

/// A kind of project the scanner can detect. Add new entries to
/// [`PROJECT_KINDS`] to support more ecosystems (flutter, gradle, etc.).
pub struct ProjectKind {
    /// Short name shown to the user, e.g. "cargo", "npm".
    pub name: &'static str,
    /// File at the project root that identifies this kind.
    pub identifier: &'static str,
    /// Returns the candidate target directory names (relative to the project
    /// root) for this kind. Called once per detected project; allowed to
    /// inspect files at `project_root` (e.g. parse package.json).
    pub targets: fn(&Path) -> Vec<&'static str>,
}

pub const PROJECT_KINDS: &[ProjectKind] = &[
    ProjectKind {
        name: "cargo",
        identifier: "Cargo.toml",
        targets: cargo_targets,
    },
    ProjectKind {
        name: "npm",
        identifier: "package.json",
        targets: npm_targets,
    },
];

fn cargo_targets(_root: &Path) -> Vec<&'static str> {
    vec!["target"]
}

fn npm_targets(root: &Path) -> Vec<&'static str> {
    let mut dirs = vec!["node_modules"];
    dirs.extend(npm_framework_targets(&root.join("package.json")));
    dirs
}

// Maps an npm dependency name to one or more cache directories it produces.
const NPM_FRAMEWORK_MAP: &[(&str, &[&str])] = &[
    ("next", &[".next"]),
    ("nuxt", &[".nuxt", ".output"]),
    ("nuxt3", &[".nuxt", ".output"]),
    ("@sveltejs/kit", &[".svelte-kit"]),
];

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
