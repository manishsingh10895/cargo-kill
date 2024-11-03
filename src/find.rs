use std::{
    fmt::Display,
    path::{Path, PathBuf},
    time::SystemTime,
};

use std::cmp::max;

use crossbeam_channel::Sender;

use crate::{utils, ProjectType};

const EXCLUDE_DIRS: [&str; 4] = [".git", "node_modules", ".vscode", "src"];

/// Folder Details
#[derive(Debug)]
pub struct ProjectTargetAnalysis {
    /// Path of the project
    pub project_path: PathBuf,
    /// Size in bytes of the target directoryA
    pub size: u64,
    /// Last Modified of the folder
    #[allow(dead_code)]
    last_modified: SystemTime,
}

impl Display for ProjectTargetAnalysis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let size = bytefmt::format(self.size);
        write!(f, "{0} \t| {1}", self.project_path.to_str().unwrap(), size,)
    }
}

struct Job(PathBuf, ProjectType, Sender<Job>);

impl ProjectTargetAnalysis {
    fn analyze(path: &Path) -> Self {
        let (size, time) = Self::recursive_scan_target(path);

        ProjectTargetAnalysis {
            project_path: path.to_path_buf(),
            size,
            last_modified: time,
        }
    }

    /// Recursive scan `target` folder and
    /// Scan for folder `size` and `last_modified`
    fn recursive_scan_target(path: &Path) -> (u64, SystemTime) {
        let default = (0, SystemTime::UNIX_EPOCH);

        match (path.is_file(), path.metadata()) {
            // If path is file return true, and file's last modified time
            (true, Ok(md)) => (md.len(), md.modified().unwrap_or(default.1)),
            _ => {
                // Else path is a directory
                // recursive scan path and accumulate
                // len and max

                let x = path
                    .read_dir()
                    .unwrap()
                    .filter_map(|it| it.ok().map(|it| it.path())) // Filter out any error `path`s
                    .map(|path| Self::recursive_scan_target(&path))
                    .fold(default, |a, b| (a.0 + b.0, a.1.max(b.1)));

                return x;
            }
        }
    }

    // Analyze size and last_modified of the folders
}

/// Find projects in the given path
/// that match the given `ProjectType`
/// and send the results to the `results` channel
fn find_projects_in_path(
    path: &Path,
    project_type: ProjectType,
    job_sender: Sender<Job>,
    results: Sender<ProjectTargetAnalysis>,
) {
    let (project_identifier, target_directory_name) =
        utils::get_project_indentifiers(project_type.clone());

    let mut has_target = false; // Checks if the current directory has `relevant_target` folder

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

    // List all directories and files in the path
    let (dirs, files): (Vec<_>, Vec<_>) = read_dir
        .filter_map(|it| it.ok().map(|it| it.path()))
        .partition(|it| it.is_dir()); // partition create two vector from above, if is_dir adds to
                                      // first vector and if not second vector
                                      // Consumes the iterator

    // Check if this path has `relevant project_identifier (pakage.json, Cargo.toml)`
    let has_project_identifier = files
        .iter()
        .filter(|file| file.file_name().unwrap_or_default().to_string_lossy() == project_identifier)
        .count()
        > 0;

    // Iterate over subdirectories, check if "target" exits,
    // Send remaning subdirectories to process further
    // Exlude directories like `node_modules`, `.git`, `.vscode`
    for dir in dirs {
        let filename = dir.file_name().unwrap_or_default().to_string_lossy();

        if filename.as_ref() == target_directory_name && has_project_identifier {
            has_target = true;
        } else {
            if EXCLUDE_DIRS.contains(&filename.as_ref()) {
                continue;
            }

            // send a new job for this directory
            job_sender
                .send(Job(
                    dir.to_path_buf(),
                    project_type.clone(),
                    job_sender.clone(),
                ))
                .unwrap();
        }
    }

    if has_target {
        let mut sp = spinners::Spinner::new(
            spinners::Spinners::Dots,
            format!("Analyzing {}", &path.to_string_lossy()).into(),
        );
        results.send(ProjectTargetAnalysis::analyze(&path)).unwrap();
        sp.stop_with_symbol("✓");
        println!("\r");
    }
}

/// Traverse and look for `ProjectType` projects
pub fn analyze_all_projects(
    path: &Path,
    mut num_threads: usize,
    project_type: ProjectType,
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
                    jr.into_iter()
                        .for_each(|job| find_projects_in_path(&job.0, job.1, job.2, rs.clone()))
                });
            });

        job_sender
            .clone()
            .send(Job(path.to_path_buf(), project_type, job_sender))
            .unwrap();

        result_receiver
    }
    .into_iter()
    .collect()
}
