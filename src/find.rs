use std::{
    fmt::Display,
    path::{Path, PathBuf},
    time::SystemTime,
};

use crossbeam_channel::Sender;

use crate::{utils, ProjectType};

const EXCLUDE_DIRS: [&str; 4] = [".git", "node_modules", ".vscode", "src"];

/// Folder Details
#[derive(Debug)]
pub struct ProjectTargetAnalysis {
    /// Path of the project
    pub project_path: PathBuf,
    /// Target directory names (relative to `project_path`) that exist and were
    /// included in the analysis. May contain `node_modules` plus framework
    /// caches like `.next`, `.svelte-kit`, etc.
    pub targets: Vec<&'static str>,
    /// Sum of bytes across all `targets`.
    pub size: u64,
    /// Last Modified of the folder
    #[allow(dead_code)]
    last_modified: SystemTime,
}

impl Display for ProjectTargetAnalysis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let size = bytefmt::format(self.size);
        write!(
            f,
            "{0} \t| {1} \t| {2}",
            self.project_path.to_string_lossy(),
            self.targets.join(", "),
            size,
        )
    }
}

struct Job {
    path: PathBuf,
    project_type: ProjectType,
    include_git: bool,
    job_sender: Sender<Job>,
}

impl ProjectTargetAnalysis {
    fn analyze(path: &Path, targets: Vec<&'static str>) -> Self {
        let mut total_size: u64 = 0;
        let mut newest = SystemTime::UNIX_EPOCH;
        for t in &targets {
            let (size, time) = Self::recursive_scan_target(&path.join(t));
            total_size = total_size.saturating_add(size);
            newest = newest.max(time);
        }

        ProjectTargetAnalysis {
            project_path: path.to_path_buf(),
            targets,
            size: total_size,
            last_modified: newest,
        }
    }

    /// Recursive scan `target` folder and
    /// Scan for folder `size` and `last_modified`
    fn recursive_scan_target(path: &Path) -> (u64, SystemTime) {
        let default = (0, SystemTime::UNIX_EPOCH);

        // symlink_metadata does not follow symlinks — needed to skip them
        // explicitly so cycles can't cause infinite recursion and symlinked
        // dirs aren't double-counted toward the parent's size.
        let md = match path.symlink_metadata() {
            Ok(md) => md,
            Err(_) => return default,
        };

        if md.file_type().is_symlink() {
            return default;
        }

        if md.is_file() {
            return (md.len(), md.modified().unwrap_or(default.1));
        }

        let entries = match path.read_dir() {
            Ok(it) => it,
            Err(e) => {
                eprintln!("Skipping {}: {}", path.to_string_lossy(), e);
                return default;
            }
        };

        entries
            .filter_map(|it| it.ok().map(|it| it.path()))
            .map(|path| Self::recursive_scan_target(&path))
            .fold(default, |a, b| (a.0.saturating_add(b.0), a.1.max(b.1)))
    }

    // Analyze size and last_modified of the folders
}

/// Find projects in the given path
/// that match the given `ProjectType`
/// and send the results to the `results` channel
fn find_projects_in_path(
    path: &Path,
    project_type: ProjectType,
    include_git: bool,
    job_sender: Sender<Job>,
    results: Sender<ProjectTargetAnalysis>,
) {
    let project_identifier = utils::project_identifier(&project_type);

    let read_dir = match path.read_dir() {
        Ok(it) => it,
        Err(e) => {
            eprintln!(
                "Error reading directory at {} {}",
                path.to_string_lossy(),
                e
            );
            return;
        }
    };

    let (dirs, files): (Vec<_>, Vec<_>) = read_dir
        .filter_map(|it| it.ok().map(|it| it.path()))
        .partition(|it| it.is_dir());

    let has_project_identifier = files
        .iter()
        .any(|file| file.file_name().unwrap_or_default().to_string_lossy() == project_identifier);

    // Per-project candidate target list (e.g. for npm: node_modules + framework caches
    // declared in package.json). Empty for non-projects.
    let candidate_targets: Vec<&'static str> = if has_project_identifier {
        let mut t = utils::target_dirs_for(&project_type, path);
        if include_git {
            t.push(".git");
        }
        t
    } else {
        vec![]
    };

    let dir_names: Vec<String> = dirs
        .iter()
        .map(|d| d.file_name().unwrap_or_default().to_string_lossy().into_owned())
        .collect();

    // Targets that actually exist on disk for this project, preserving the
    // candidate ordering.
    let found_targets: Vec<&'static str> = candidate_targets
        .iter()
        .copied()
        .filter(|t| dir_names.iter().any(|n| n == *t))
        .collect();

    for (dir, filename) in dirs.iter().zip(dir_names.iter()) {
        if EXCLUDE_DIRS.contains(&filename.as_str()) {
            continue;
        }
        // Don't recurse into this project's own target directories — they're
        // already accounted for and shouldn't be scanned as separate projects.
        if candidate_targets.iter().any(|t| *t == filename) {
            continue;
        }

        job_sender
            .send(Job {
                path: dir.to_path_buf(),
                project_type: project_type.clone(),
                include_git,
                job_sender: job_sender.clone(),
            })
            .unwrap();
    }

    if !found_targets.is_empty() {
        let mut sp = spinners::Spinner::new(
            spinners::Spinners::Dots,
            format!("Analyzing {}", &path.to_string_lossy()),
        );
        results
            .send(ProjectTargetAnalysis::analyze(path, found_targets))
            .unwrap();
        sp.stop_with_symbol("✓");
        println!("\r");
    }
}

/// Traverse and look for `ProjectType` projects
pub fn analyze_all_projects(
    path: &Path,
    mut num_threads: usize,
    project_type: ProjectType,
    include_git: bool,
) -> Vec<ProjectTargetAnalysis> {
    num_threads = std::cmp::min(num_cpus::get(), num_threads);

    println!("Using {} threads", num_threads);

    {
        let (job_sender, job_receiver) = crossbeam_channel::unbounded::<Job>();
        let (result_sender, result_receiver) =
            crossbeam_channel::unbounded::<ProjectTargetAnalysis>();

        (0..num_threads)
            .map(|_| (job_receiver.clone(), result_sender.clone()))
            .for_each(|(jr, rs)| {
                std::thread::spawn(move || {
                    jr.into_iter().for_each(|job| {
                        find_projects_in_path(
                            &job.path,
                            job.project_type,
                            job.include_git,
                            job.job_sender,
                            rs.clone(),
                        )
                    })
                });
            });

        job_sender
            .clone()
            .send(Job {
                path: path.to_path_buf(),
                project_type,
                include_git,
                job_sender,
            })
            .unwrap();

        result_receiver
    }
    .into_iter()
    .collect()
}
