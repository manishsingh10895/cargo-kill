use crate::ProjectType;

pub fn get_project_indentifiers(project_type: ProjectType) -> (&'static str, &'static str) {
    match project_type {
        ProjectType::Npm => ("package.json", "node_modules"),
        ProjectType::Cargo => ("Cargo.toml", "target"),
    }
}
