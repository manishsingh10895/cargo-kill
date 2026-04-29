use std::{
    fmt::Display,
    path::{Path, PathBuf},
    time::SystemTime,
};

use crossbeam_channel::Sender;

use crate::utils::PROJECT_KINDS;

const EXCLUDE_DIRS: [&str; 4] = [".git", "node_modules", ".vscode", "src"];

/// Per-project analysis result.
#[derive(Debug)]
pub struct ProjectTargetAnalysis {
    /// Path of the project root.
    pub project_path: PathBuf,
    /// Detector names that matched (e.g. `["cargo"]`, `["npm"]`,
    /// `["cargo", "npm"]`, or `["git"]` for a `.git`-only directory).
    pub kinds: Vec<&'static str>,
    /// Target directory names (relative to `project_path`) that exist and
    /// were included in the analysis.
    pub targets: Vec<&'static str>,
    /// Sum of bytes across all `targets`.
    pub size: u64,
    /// Newest mtime seen across all `targets`.
    #[allow(dead_code)]
    last_modified: SystemTime,
}

impl Display for ProjectTargetAnalysis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let size = bytefmt::format(self.size);
        write!(
            f,
            "{0} \t| {1} \t| {2} \t| {3}",
            self.project_path.to_string_lossy(),
            self.kinds.join(", "),
            self.targets.join(", "),
            size,
        )
    }
}

struct Job {
    path: PathBuf,
    include_git: bool,
    job_sender: Sender<Job>,
}

impl ProjectTargetAnalysis {
    fn analyze(path: &Path, kinds: Vec<&'static str>, targets: Vec<&'static str>) -> Self {
        let mut total_size: u64 = 0;
        let mut newest = SystemTime::UNIX_EPOCH;
        for t in &targets {
            let (size, time) = Self::recursive_scan_target(&path.join(t));
            total_size = total_size.saturating_add(size);
            newest = newest.max(time);
        }

        ProjectTargetAnalysis {
            project_path: path.to_path_buf(),
            kinds,
            targets,
            size: total_size,
            last_modified: newest,
        }
    }

    /// Recursively measure size and newest mtime of a directory, treating
    /// errors as empty subtrees. Does not follow symlinks.
    fn recursive_scan_target(path: &Path) -> (u64, SystemTime) {
        let default = (0, SystemTime::UNIX_EPOCH);

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
}

/// Walk one directory, schedule jobs for its subdirectories, and emit a
/// `ProjectTargetAnalysis` if this directory looks like a project root for
/// any registered detector (or, with `--include-git`, if it contains `.git`).
fn find_projects_in_path(
    path: &Path,
    include_git: bool,
    job_sender: Sender<Job>,
    results: Sender<ProjectTargetAnalysis>,
) {
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

    let file_names: Vec<String> = files
        .iter()
        .map(|f| f.file_name().unwrap_or_default().to_string_lossy().into_owned())
        .collect();
    let dir_names: Vec<String> = dirs
        .iter()
        .map(|d| d.file_name().unwrap_or_default().to_string_lossy().into_owned())
        .collect();

    // Apply every detector. A dir can match multiple kinds.
    let mut kinds: Vec<&'static str> = Vec::new();
    let mut candidate_targets: Vec<&'static str> = Vec::new();
    for kind in PROJECT_KINDS {
        if file_names.iter().any(|n| n == kind.identifier) {
            kinds.push(kind.name);
            for t in (kind.targets)(path) {
                if !candidate_targets.contains(&t) {
                    candidate_targets.push(t);
                }
            }
        }
    }

    // .git is a target on its own when --include-git is set. Any directory
    // containing .git becomes a project even without another identifier.
    let has_git = dir_names.iter().any(|n| n == ".git");
    if include_git && has_git {
        kinds.push("git");
        if !candidate_targets.contains(&".git") {
            candidate_targets.push(".git");
        }
    }

    let found_targets: Vec<&'static str> = candidate_targets
        .iter()
        .copied()
        .filter(|t| dir_names.iter().any(|n| n == *t))
        .collect();

    for (dir, filename) in dirs.iter().zip(dir_names.iter()) {
        if EXCLUDE_DIRS.contains(&filename.as_str()) {
            continue;
        }
        // Don't recurse into dirs we've already accounted for as targets of
        // this project (framework caches, target/, node_modules, etc.).
        if candidate_targets.iter().any(|t| *t == filename) {
            continue;
        }

        job_sender
            .send(Job {
                path: dir.to_path_buf(),
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
            .send(ProjectTargetAnalysis::analyze(path, kinds, found_targets))
            .unwrap();
        sp.stop_with_symbol("✓");
        println!("\r");
    }
}

/// Scan `path` recursively, detecting every supported project kind in one
/// pass. With `include_git`, also surfaces any directory containing `.git`.
pub fn analyze_all_projects(
    path: &Path,
    mut num_threads: usize,
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
                include_git,
                job_sender,
            })
            .unwrap();

        result_receiver
    }
    .into_iter()
    .collect()
}
