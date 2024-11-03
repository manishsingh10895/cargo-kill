use crate::ProjectType;

/// Get the project identifiers for the given project type.
/// Example:
///     For a node/npm project, the project identifier are `package.json` and `node_modules`.
pub fn get_project_indentifiers(project_type: ProjectType) -> (&'static str, &'static str) {
    match project_type {
        ProjectType::Npm => ("package.json", "node_modules"),
        ProjectType::Cargo => ("Cargo.toml", "target"),
    }
}
