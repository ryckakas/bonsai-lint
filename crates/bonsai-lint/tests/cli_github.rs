//! `--format github` turns the report into workflow commands, which GitHub Actions shows as
//! annotations on the pull request. It must select what `text` selects and exit as `text` exits,
//! so switching format in CI never changes what passes.

use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::process::{Output, Stdio};

mod common;

use common::{CALM_PHP, Project, code, nested_php, stderr, stdout};

/// This repository's own CI defines `GITHUB_WORKSPACE`, so every run pins it one way or the other.
fn run_in_actions(project: &Project, args: &[&str], workspace: Option<&Path>) -> Output {
    run_in_actions_from(project, &project.root, args, workspace)
}

fn run_in_actions_from(
    project: &Project,
    directory: &Path,
    args: &[&str],
    workspace: Option<&Path>,
) -> Output {
    let mut command = project.command(args);
    command.current_dir(directory);
    match workspace {
        Some(workspace) => command.env("GITHUB_WORKSPACE", workspace),
        None => command.env_remove("GITHUB_WORKSPACE"),
    };
    command.output().expect("binary runs")
}

fn with_marker(source: &str, marker: &str) -> String {
    source.replacen("<?php\n", &format!("<?php\n// {marker}\n"), 1)
}

#[test]
fn a_breach_is_an_error_on_its_line() {
    let project = Project::new();
    project.file("src/a.php", &nested_php("busy", 6));

    let output = run_in_actions(&project, &["--format", "github", "."], None);

    assert_eq!(
        stdout(&output),
        "::error file=src/a.php,line=2,title=bonsai-lint::src/a.php:2: busy scores 21, over the \
         threshold of 15\n"
    );
    assert!(stderr(&output).ends_with("\n1 unit(s) over the threshold\n"));
    assert_eq!(code(&output), 1);
    assert_eq!(code(&project.run(&["."])), 1);
}

#[test]
fn a_baselined_unit_that_got_worse_names_its_recorded_score() {
    let project = Project::new();
    project.file("src/a.php", &nested_php("busy", 6));
    assert_eq!(code(&project.run(&["--write-baseline", "."])), 0);
    project.file("src/a.php", &nested_php("busy", 7));

    let output = run_in_actions(&project, &["--format", "github", "."], None);

    assert_eq!(
        stdout(&output),
        "::error file=src/a.php,line=2,title=bonsai-lint::src/a.php:2: busy scores 28, above its \
         baselined 21\n"
    );
    assert_eq!(code(&output), 1);
}

#[test]
fn a_clean_or_accepted_run_prints_nothing_and_exits_as_text_does() {
    let project = Project::new();
    project.file("src/a.php", CALM_PHP);
    project.file("src/b.php", &nested_php("accepted", 6));
    assert_eq!(code(&project.run(&["--write-baseline", "."])), 0);

    let output = run_in_actions(&project, &["--format", "github", "."], None);

    assert_eq!(stdout(&output), "");
    assert_eq!(stderr(&output), "");
    assert_eq!(code(&output), 0);
    assert_eq!(code(&project.run(&["."])), 0);
}

#[test]
fn paths_are_rebased_on_the_github_workspace() {
    let project = Project::new();
    project.file("src/a.php", &nested_php("busy", 6));
    let parent = project.root.parent().expect("temp dir has a parent");
    let name = project
        .root
        .file_name()
        .expect("temp dir has a name")
        .to_string_lossy();

    let output = run_in_actions(&project, &["--format", "github", "."], Some(parent));

    assert!(
        stdout(&output).starts_with(&format!(
            "::error file={name}/src/a.php,line=2,title=bonsai-lint::{name}/src/a.php:2: busy"
        )),
        "{}",
        stdout(&output)
    );
}

/// A step with `working-directory` runs below the checkout, but GitHub still reads `file` from
/// the repository root.
#[test]
fn a_run_from_a_subdirectory_files_under_the_repository_root() {
    let project = Project::new();
    project.file("src/a.php", &nested_php("busy", 6));
    let src = project.root.join("src");

    let plain = run_in_actions_from(&project, &src, &["--format", "github", "."], None);
    assert!(stdout(&plain).starts_with("::error file=a.php,line=2,"));

    let rebased = run_in_actions_from(
        &project,
        &src,
        &["--format", "github", "."],
        Some(&project.root),
    );
    assert!(
        stdout(&rebased).starts_with("::error file=src/a.php,line=2,"),
        "{}",
        stdout(&rebased)
    );
}

#[test]
fn an_absolute_argument_files_under_the_repository_root() {
    let project = Project::new();
    project.file("src/a.php", &nested_php("busy", 6));
    // A canonical Windows path is verbatim, which no one types and which prints as `//?/C:/…`.
    let root = project.root.to_string_lossy();
    let root = root.trim_start_matches(r"\\?\");
    let absolute = Path::new(root).join("src");

    let output = run_in_actions(
        &project,
        &["--format", "github", &absolute.to_string_lossy()],
        Some(&project.root),
    );

    assert!(
        stdout(&output).starts_with("::error file=src/a.php,line=2,"),
        "{}",
        stdout(&output)
    );
}

#[cfg(unix)]
#[test]
fn a_symlinked_workspace_is_resolved_like_the_files() {
    let project = Project::new();
    project.file("src/a.php", &nested_php("busy", 6));
    let links = tempfile::tempdir().expect("temp dir");
    let link = links.path().join("checkout");
    std::os::unix::fs::symlink(&project.root, &link).expect("symlink");

    let output = run_in_actions(&project, &["--format", "github", "."], Some(&link));

    assert!(
        stdout(&output).starts_with("::error file=src/a.php,line=2,"),
        "{}",
        stdout(&output)
    );
}

/// A path GitHub cannot place would be filed under a wrong or missing file, so the annotation goes
/// to the run's summary and the message still names the file.
#[test]
fn a_file_outside_the_workspace_is_annotated_on_the_run() {
    let project = Project::new();
    project.file("src/a.php", &nested_php("busy", 6));
    project.file("elsewhere/README", "");

    let output = run_in_actions(
        &project,
        &["--format", "github", "."],
        Some(&project.root.join("elsewhere")),
    );

    assert_eq!(
        stdout(&output),
        "::error title=bonsai-lint::src/a.php:2: busy scores 21, over the threshold of 15\n"
    );
}

#[test]
fn paths_are_escaped_as_properties_and_as_messages() {
    let project = Project::new();
    project.file("src/100%,a.php", &nested_php("busy", 6));

    let output = run_in_actions(&project, &["--format", "github", "."], None);

    assert!(
        stdout(&output).starts_with(
            "::error file=src/100%25%2Ca.php,line=2,title=bonsai-lint::src/100%25,a.php:2: busy"
        ),
        "{}",
        stdout(&output)
    );
}

/// Windows refuses `:` in a file name, and a property must escape it or the command ends there.
#[cfg(unix)]
#[test]
fn a_colon_in_a_path_is_escaped_in_the_file_property() {
    let project = Project::new();
    project.file("src/a:b.php", &nested_php("busy", 6));

    let output = run_in_actions(&project, &["--format", "github", "."], None);

    assert!(
        stdout(&output).starts_with(
            "::error file=src/a%3Ab.php,line=2,title=bonsai-lint::src/a:b.php:2: busy"
        ),
        "{}",
        stdout(&output)
    );
}

#[test]
fn a_loose_entry_warns_on_the_baseline_file_and_errs_only_when_strict() {
    let project = Project::new();
    project.file("src/a.php", &nested_php("busy", 7));
    assert_eq!(code(&project.run(&["--write-baseline", "."])), 0);
    project.file("src/a.php", &nested_php("busy", 6));
    let expected = "file=.bonsai-lint-baseline.json,line=1,title=bonsai-lint::root: src/a.php: busy \
                    is baselined at 28 but scores 21\n";

    let lenient = run_in_actions(&project, &["--format", "github", "."], Some(&project.root));
    assert_eq!(stdout(&lenient), format!("::warning {expected}"));
    assert_eq!(stderr(&lenient), "");
    assert_eq!(code(&lenient), 0);

    let strict = run_in_actions(
        &project,
        &["--format", "github", "--strict-baseline", "."],
        Some(&project.root),
    );
    assert_eq!(stdout(&strict), format!("::error {expected}"));
    assert!(stderr(&strict).contains("tighten them with --write-baseline"));
    assert_eq!(code(&strict), 1);
}

#[test]
fn a_marker_without_a_reason_warns_on_its_unit() {
    let project = Project::new();
    project.file(
        "src/a.php",
        &with_marker(&nested_php("busy", 6), "bonsai-lint-ignore"),
    );

    let output = run_in_actions(&project, &["--format", "github", "."], None);

    assert!(
        stderr(&output).starts_with(
            "::warning file=src/a.php,line=3,title=bonsai-lint::src/a.php:3: `bonsai-lint-ignore` \
             needs a reason"
        ),
        "{}",
        stderr(&output)
    );
    assert!(stdout(&output).starts_with("::error file=src/a.php,line=3,"));
}

#[test]
fn run_level_warnings_and_errors_are_annotated_on_the_run() {
    let project = Project::new();
    project.file("bonsai-lint.toml", "[ruby]\nthreshold = 3\n");
    project.file("src/a.php", CALM_PHP);

    let warned = run_in_actions(&project, &["--format", "github", "."], None);
    assert!(
        stderr(&warned).starts_with("::warning title=bonsai-lint::")
            && stderr(&warned).contains("unknown language section `[ruby]`"),
        "{}",
        stderr(&warned)
    );
    assert_eq!(code(&warned), 0);

    let failed = run_in_actions(&project, &["--format", "github", "missing"], None);
    assert!(
        stderr(&failed).ends_with(
            "::error title=bonsai-lint::1 path(s) could not be read; refusing to report a clean \
             run\n"
        ),
        "{}",
        stderr(&failed)
    );
    assert_eq!(code(&failed), 1);
}

/// A partial scan cannot judge a baseline, and saying so must not wait for someone to open the
/// log.
#[test]
fn the_unjudged_strict_note_is_a_warning() {
    let project = Project::new();
    project.file("src/a.php", &nested_php("busy", 6));
    assert_eq!(code(&project.run(&["--write-baseline", "."])), 0);

    let output = run_in_actions(
        &project,
        &[
            "--format",
            "github",
            "--strict-baseline",
            "--lang",
            "php",
            ".",
        ],
        None,
    );

    assert_eq!(
        stderr(&output),
        "::warning title=bonsai-lint::--strict-baseline: 1 baseline(s) not judged; only a scan of \
         a whole domain, without --stdin, --domain or --lang, judges its baseline\n"
    );
    assert_eq!(code(&output), 0);
}

#[test]
fn all_is_refused_with_the_github_format() {
    let project = Project::new();
    project.file("src/a.php", CALM_PHP);

    let output = run_in_actions(&project, &["--all", "--format", "github", "."], None);

    assert!(stderr(&output).contains("--all cannot be combined with --format github"));
    assert_eq!(code(&output), 2);
}

#[test]
fn past_ten_errors_the_log_says_the_annotations_stop() {
    let project = Project::new();
    for index in 0..10 {
        project.file(
            &format!("src/u{index}.php"),
            &nested_php(&format!("u{index}"), 6),
        );
    }
    let at_the_cap = run_in_actions(&project, &["--format", "github", "."], None);
    assert!(!stderr(&at_the_cap).contains("GitHub annotates"));

    project.file("src/u10.php", &nested_php("u10", 6));
    let past_it = run_in_actions(&project, &["--format", "github", "."], None);
    assert!(
        stderr(&past_it).ends_with(
            "::notice title=bonsai-lint::GitHub annotates at most 10 errors per step; all 11 are \
             listed in the log\n"
        ),
        "{}",
        stderr(&past_it)
    );
    assert_eq!(stdout(&past_it).lines().count(), 11);
}

#[test]
fn a_shared_baselines_loose_entry_is_filed_on_it() {
    let project = Project::new();
    project.file("src/a.php", &nested_php("busy", 6));
    let shared = project.root.join("ledger.json");
    let shared = shared.to_string_lossy();
    assert_eq!(
        code(&project.run(&["--write-baseline", "--baseline", &shared, "."])),
        0
    );
    fs::remove_file(project.root.join("src/a.php")).expect("remove");
    project.file("src/b.php", CALM_PHP);

    let output = run_in_actions(
        &project,
        &["--format", "github", "--baseline", &shared, "."],
        Some(&project.root),
    );

    assert_eq!(
        stdout(&output),
        "::warning file=ledger.json,line=1,title=bonsai-lint::baseline: src/a.php: busy is baselined \
         at 21 but matched nothing in this scan\n"
    );
}

/// The runner annotates only the first ten errors, so deleting baselined code under a strict
/// baseline must not push a breach on the pull request's own lines out of them.
#[test]
fn a_breach_comes_before_the_loose_entries_that_could_crowd_it_out() {
    let project = Project::new();
    for index in 0..11 {
        project.file(
            &format!("src/old{index}.php"),
            &nested_php(&format!("old{index}"), 6),
        );
    }
    assert_eq!(code(&project.run(&["--write-baseline", "."])), 0);
    for index in 0..11 {
        fs::remove_file(project.root.join(format!("src/old{index}.php"))).expect("remove");
    }
    project.file("src/fresh.php", &nested_php("fresh", 6));

    let output = run_in_actions(
        &project,
        &["--format", "github", "--strict-baseline", "."],
        Some(&project.root),
    );

    let printed = stdout(&output);
    let lines: Vec<&str> = printed.lines().collect();
    assert_eq!(lines.len(), 12);
    assert!(
        lines[0].starts_with("::error file=src/fresh.php,line=2,"),
        "{lines:?}"
    );
    assert!(
        lines[1..]
            .iter()
            .all(|line| line.starts_with("::error file=.bonsai-lint-baseline.json,line=1,")),
        "{lines:?}"
    );
}

#[test]
fn an_editor_buffer_is_annotated_under_its_stdin_path() {
    let project = Project::new();
    project.file("src/a.php", CALM_PHP);

    let args = [
        "--format",
        "github",
        "--stdin",
        "--stdin-path",
        "src/new/b.php",
    ];
    let mut child = project
        .command(&args)
        .env("GITHUB_WORKSPACE", &project.root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("binary starts");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(nested_php("buffer", 6).as_bytes())
        .expect("stdin accepts input");
    let output = child.wait_with_output().expect("binary exits");

    assert!(
        stdout(&output).starts_with("::error file=src/new/b.php,line=2,"),
        "{}",
        stdout(&output)
    );
}

/// An empty variable names no checkout, so the paths print as they would without it.
#[test]
fn an_empty_github_workspace_is_no_workspace() {
    let project = Project::new();
    project.file("src/a.php", &nested_php("busy", 6));

    let output = run_in_actions(&project, &["--format", "github", "."], Some(Path::new("")));

    assert!(
        stdout(&output).starts_with("::error file=src/a.php,line=2,"),
        "{}",
        stdout(&output)
    );
}
