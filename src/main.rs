use crate::find::{analyze_all_projects, ProjectTargetAnalysis};
use bytefmt;
use clap::Parser;
use inquire::{
    list_option::ListOption,
    ui::{IndexPrefix, RenderConfig},
    MultiSelect,
};
use std::cmp::Reverse;
use std::path::Path;

mod find;
mod utils;

#[derive(Debug, Parser)]
#[clap(
    author,
    version,
    about,
    bin_name = "cargo kill-all",
    long_about = "Scan a directory tree for cargo and npm projects (and optionally .git directories) and reclaim their build/cache space."
)]
struct KillArgs {
    /// Starting directory to clean
    #[clap(default_value_t = String::from("."), value_name = "DIR")]
    root_dir: String,

    /// Don't ask to confirmation
    #[clap(short = 'y', long = "yes")]
    yes: bool,

    #[clap(short = 'd', long = "dry-run")]
    dry_run: bool,

    #[clap(
        short = 't',
        long = "threads",
        value_name = "THREADS",
        default_value_t = 4
    )]
    num_threads: usize,

    /// Also list each detected project's `.git` directory as a deletion
    /// candidate, and surface bare `.git` directories that aren't otherwise
    /// recognized as a project. Destructive: wipes local repo history.
    #[clap(long = "include-git")]
    include_git: bool,
}

fn main() {
    let mut args = std::env::args();

    // When called using `cargo kill-all` the argument `kill-all` is inserted.
    // It is not required, so remove  it
    if let Some("kill-all") = std::env::args().nth(1).as_deref() {
        args.next();
    }
    let args = KillArgs::parse_from(args);

    let mut projects = analyze_all_projects(
        &Path::new(&args.root_dir),
        args.num_threads,
        args.include_git,
    );

    projects.sort_by_key(|p| Reverse(p.size));

    // Transform vector to string to better display directry data
    let options = projects
        .iter()
        .map(|project| project.to_string().to_owned())
        .collect::<Vec<String>>();

    // Transform generated options to `ListOption` for `MultiSelect`
    let options = options
        .iter()
        .enumerate()
        .map(|(i, s)| ListOption::new(i, s as &str))
        .collect();

    // Show number in front of every row
    let mut render_config = RenderConfig::default_colored();
    render_config.option_index_prefix = IndexPrefix::Simple;

    let ans = MultiSelect::new("Select the folders to delete", options)
        .with_render_config(render_config)
        .prompt();

    // Extract only selected projects from the full vector
    let selected_projects: Vec<&ProjectTargetAnalysis> = ans
        .unwrap_or(vec![])
        .iter()
        .map(|l| l.index)
        .map(|i| &projects[i])
        .collect();

    // Total size of the selected folders
    let selected_total_size: u64 = selected_projects.iter().map(|x| x.size).sum();

    println!(
        "\n{} Folders to be removed\nTotal space to be freed {}",
        selected_projects.len(),
        bytefmt::format(selected_total_size)
    );

    if args.dry_run {
        println!("Dry run");
        return;
    }

    // Exit if no projects are selected
    if selected_projects.is_empty() {
        println!("No projects selected, exiting ...");
        return;
    }

    // Ask user for confirmation if `yes` argument not provided
    if !args.yes {
        let ans = inquire::Confirm::new("Do you want to delete the selected folders?")
            .with_default(false)
            .with_help_message("This action is permanent")
            .prompt();

        match ans {
            // User selected `No` exit
            Ok(false) => {
                println!("No cleanup ahead");
                return;
            }
            Ok(true) => {}
            Err(_) => {
                println!("No cleanup ahead");
                return;
            }
        }
    }

    let mut sp = spinners::Spinner::new(
        spinners::Spinners::Dots,
        String::from("Deleting dependency folders"),
    );

    selected_projects.iter().for_each(|p| {
        for target in &p.targets {
            let dir = p.project_path.join(target);
            if let Err(e) = remove_dir_all::remove_dir_all(&dir) {
                eprintln!(
                    "Directory Deletion failed for {} \n {}",
                    dir.to_string_lossy(),
                    e
                );
            }
        }
    });

    sp.stop_with_message(String::from("DONE"));
}
