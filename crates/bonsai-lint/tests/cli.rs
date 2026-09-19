//! The contract a CI gate and an editor rely on: exit codes, output shapes and the flags that
//! change what a scan means. Each test drives the real binary against a throwaway project.

use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const BUSY_PHP: &str =
    "<?php\nfunction busy($a, $b) {\n    if ($a) { if ($b) { return 1; } }\n    return 0;\n}\n";
const BUSY_PHP_TOO: &str =
    "<?php\nfunction busier($a, $b) {\n    if ($a) { if ($b) { return 1; } }\n    return 0;\n}\n";
const CALM_PHP: &str = "<?php\nfunction calm() { return 1; }\n";
const CALM_TS: &str = "function calm() { return 1; }\n";
const BUSY_TS: &str =
    "function busy(a, b) {\n    if (a) { if (b) { return 1; } }\n    return 0;\n}\n";

struct Project {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Project {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path().canonicalize().expect("canonical temp dir");
        Self { _dir: dir, root }
    }

    fn file(&self, relative: &str, content: &str) -> &Self {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().expect("has parent")).expect("mkdir");
        fs::write(path, content).expect("write");
        self
    }

    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.root.join(relative)).expect("file exists")
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_bonsai-lint"));
        command.args(args).current_dir(&self.root);
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().expect("binary runs")
    }

    fn run_with_stdin(&self, args: &[&str], input: &str) -> Output {
        let mut child = self
            .command(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("binary starts");
        child
            .stdin
            .take()
            .expect("stdin is piped")
            .write_all(input.as_bytes())
            .expect("stdin accepts input");
        child.wait_with_output().expect("binary exits")
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("exited normally")
}

fn report(output: &Output) -> serde_json::Value {
    serde_json::from_str(&stdout(output))
        .unwrap_or_else(|error| panic!("{error}\nstdout:\n{}", stdout(output)))
}

fn ranked_scores(output: &Output) -> Vec<u64> {
    report(output)["findings"]
        .as_array()
        .expect("findings is an array")
        .iter()
        .map(|finding| finding["score"].as_u64().expect("score is a number"))
        .collect()
}

fn scores(output: &Output) -> Vec<u64> {
    let mut scores = ranked_scores(output);
    scores.sort_unstable();
    scores
}

fn paths(output: &Output) -> Vec<String> {
    report(output)["findings"]
        .as_array()
        .expect("findings is an array")
        .iter()
        .map(|finding| {
            finding["path"]
                .as_str()
                .expect("path is a string")
                .to_string()
        })
        .collect()
}

#[test]
fn a_clean_run_prints_nothing_and_exits_zero() {
    let project = Project::new();
    project.file("src/a.php", CALM_PHP);

    let output = project.run(&["src"]);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(stdout(&output), "");
}

#[test]
fn a_breach_exits_one_and_names_the_unit() {
    let project = Project::new();
    project.file("src/a.php", BUSY_PHP);

    let output = project.run(&["--over", "1", "src"]);

    assert_eq!(code(&output), 1);
    assert!(stdout(&output).contains("busy"), "{}", stdout(&output));
    assert!(
        stderr(&output).contains("1 unit(s) over the threshold"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_path_without_supported_files_is_a_failure() {
    let project = Project::new();
    project.file("src/notes.txt", "nothing to see");

    let output = project.run(&["src"]);

    assert_eq!(code(&output), 1);
    assert!(
        stderr(&output).contains("no supported files found"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn an_unreadable_path_is_a_failure() {
    let project = Project::new();
    project.file("src/a.php", CALM_PHP);

    let output = project.run(&["src", "missing"]);

    assert_eq!(code(&output), 1);
    assert!(
        stderr(&output).contains("could not be read"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_broken_config_is_a_failure_that_names_the_file() {
    let project = Project::new();
    project
        .file("bonsai-lint.toml", "thresold = 20\n")
        .file("src/a.php", CALM_PHP);

    let output = project.run(&["src"]);

    assert_eq!(code(&output), 1);
    assert!(
        stderr(&output).contains("bonsai-lint.toml"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn json_thresholds_are_keyed_by_language_id() {
    let project = Project::new();
    project.file("src/a.php", CALM_PHP);

    let output = project.run(&["--format", "json", "src"]);

    let thresholds = report(&output)["thresholds"]
        .as_object()
        .expect("thresholds is an object")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(thresholds, vec!["php", "typescript"]);
}

#[test]
fn the_language_filter_covers_every_grammar_of_a_language() {
    let project = Project::new();
    project
        .file("a.ts", CALM_TS)
        .file("b.js", CALM_TS)
        .file("c.php", CALM_PHP);

    let output = project.run(&["--all", "--lang", "typescript", "."]);

    let listing = stdout(&output);
    assert!(listing.contains("a.ts"), "{listing}");
    assert!(listing.contains("b.js"), "{listing}");
    assert!(!listing.contains("c.php"), "{listing}");
}

#[test]
fn an_unknown_language_id_is_rejected_up_front() {
    let project = Project::new();
    project.file("a.ts", CALM_TS);

    for args in [&["--lang", "tsx", "."][..], &["--over", "tsx=1", "."][..]] {
        let output = project.run(args);
        assert_eq!(code(&output), 1, "{args:?}");
        assert!(
            stderr(&output).contains("unknown language `tsx`"),
            "{}",
            stderr(&output)
        );
        assert!(
            stderr(&output).contains("typescript"),
            "{}",
            stderr(&output)
        );
    }
}

#[test]
fn a_per_language_override_applies_to_every_grammar_of_that_language() {
    let project = Project::new();
    project.file("routes.js", "app.get('/a', () => { if (a) { f(); } });\n");

    let output = project.run(&["--over", "typescript=0", "."]);

    assert_eq!(code(&output), 1, "{}", stderr(&output));
}

#[test]
fn stdin_respects_the_workspace_excludes() {
    let project = Project::new();
    project
        .file("bonsai-lint.toml", "exclude = [\"vendor/**\"]\n")
        .file("vendor/x.php", BUSY_PHP);

    let output = project.run_with_stdin(
        &[
            "--format",
            "json",
            "--over",
            "1",
            "--stdin",
            "--stdin-path",
            "vendor/x.php",
        ],
        BUSY_PHP,
    );

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(scores(&output).is_empty());
}

#[test]
fn stdin_scores_the_buffer_not_the_file_on_disk() {
    let project = Project::new();
    project.file("src/a.php", CALM_PHP);

    let output = project.run_with_stdin(
        &[
            "--format",
            "json",
            "--over",
            "1",
            "--stdin",
            "--stdin-path",
            "src/a.php",
        ],
        BUSY_PHP,
    );

    assert_eq!(code(&output), 1);
    assert_eq!(scores(&output), vec![3]);
}

#[test]
fn stale_baseline_entries_are_reported_only_by_a_scan_that_covers_the_domain() {
    let project = Project::new();
    project
        .file("src/a.php", BUSY_PHP)
        .file("lib/b.php", BUSY_PHP_TOO);
    assert_eq!(
        code(&project.run(&["--over", "1", "--write-baseline", "."])),
        0
    );

    let stale = "matched nothing";
    let buffer = project.run_with_stdin(
        &["--over", "1", "--stdin", "--stdin-path", "src/a.php"],
        BUSY_PHP,
    );
    assert!(!stderr(&buffer).contains(stale), "{}", stderr(&buffer));

    let whole = project.run(&["--over", "1", "."]);
    assert_eq!(code(&whole), 0, "{}", stderr(&whole));
    assert!(!stderr(&whole).contains(stale), "{}", stderr(&whole));

    fs::remove_file(project.root.join("lib/b.php")).expect("remove");

    let partial = project.run(&["--over", "1", "src"]);
    assert!(!stderr(&partial).contains(stale), "{}", stderr(&partial));

    let whole = project.run(&["--over", "1", "."]);
    assert!(stderr(&whole).contains(stale), "{}", stderr(&whole));
}

/// Pre-commit hooks pass changed files one by one, and each of them must see the baseline the
/// whole-project run wrote.
#[test]
fn a_single_file_scan_shares_the_project_baseline_without_a_config() {
    let project = Project::new();
    project.file("src/a.php", BUSY_PHP);
    assert_eq!(
        code(&project.run(&["--over", "1", "--write-baseline", "."])),
        0
    );
    assert!(project.root.join(".bonsai-lint-baseline.json").is_file());

    let output = project.run(&["--over", "1", "src/a.php"]);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(!project.root.join("src/.bonsai-lint-baseline.json").exists());
}

#[test]
fn a_shared_baseline_keys_by_workspace_relative_path() {
    let project = Project::new();
    project
        .file("bonsai-lint.toml", "domains = [\"packages/*\"]\n")
        .file("packages/web/src/index.php", BUSY_PHP)
        .file("packages/api/src/index.php", BUSY_PHP_TOO);

    let written = project.run(&[
        "--over",
        "1",
        "--write-baseline",
        "--baseline",
        "shared.json",
        ".",
    ]);
    assert_eq!(code(&written), 0, "{}", stderr(&written));

    let shared = project.read("shared.json");
    assert!(shared.contains("packages/web/src/index.php"), "{shared}");
    assert!(shared.contains("packages/api/src/index.php"), "{shared}");

    let checked = project.run(&["--over", "1", "--baseline", "shared.json", "."]);
    assert_eq!(code(&checked), 0, "{}", stderr(&checked));
}

/// The headline claim: the same logic in either language gets the same number.
#[test]
fn identical_logic_scores_the_same_in_php_and_javascript() {
    let project = Project::new();
    project
        .file(
            "php/routes.php",
            "<?php\nRoute::get('/a', function () { if ($a) { return 1; } return 0; });\nRoute::get('/b', function () { foreach ($xs as $x) { if ($x) { echo 1; } } });\n",
        )
        .file(
            "js/routes.js",
            "app.get('/a', () => { if (a) { return 1; } return 0; });\napp.get('/b', () => { for (const x of xs) { if (x) { f(); } } });\n",
        );

    let php = project.run(&["--all", "--format", "json", "php"]);
    let js = project.run(&["--all", "--format", "json", "js"]);

    assert_eq!(scores(&php), vec![1, 3]);
    assert_eq!(scores(&php), scores(&js));
}

#[test]
fn deeply_nested_expressions_do_not_overflow_the_stack() {
    let project = Project::new();
    let mut source = String::from("<?php\n$x = 'a'");
    for _ in 0..30_000 {
        source.push_str(" . 'a'");
    }
    source.push_str(";\n");
    project.file("deep.php", &source);

    let output = project.run(&["--all", "deep.php"]);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
}

#[cfg(unix)]
#[test]
fn a_reader_closing_the_pipe_early_is_not_a_failure() {
    let project = Project::new();
    let mut source = String::from("<?php\n");
    for index in 0..3000 {
        writeln!(source, "function f{index}() {{ return 1; }}").expect("string write");
    }
    project.file("many.php", &source);

    let mut child = project
        .command(&["--all", "many.php"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("binary starts");
    drop(child.stdout.take());
    let output = child.wait_with_output().expect("binary exits");

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(!stderr(&output).contains("panicked"), "{}", stderr(&output));
}

/// A per-app CI job, a pre-commit hook and a developer inside one package all point the CLI at
/// a domain directory. The answer must be the one the root scan gives.
#[test]
fn a_scan_started_inside_a_domain_agrees_with_one_from_the_root() {
    let project = Project::new();
    project
        .file(
            "bonsai-lint.toml",
            "domains = [\"apps/*\"]\nthreshold = 15\nexclude = [\"**/*.spec.ts\"]\n",
        )
        .file(
            "apps/wallet/bonsai-lint.toml",
            "name = \"wallet\"\nthreshold = 2\n",
        )
        .file("apps/wallet/src/a.ts", BUSY_TS)
        .file("apps/wallet/src/a.spec.ts", BUSY_TS)
        .file("src/calm.ts", CALM_TS);

    let from_root = project.run(&["--format", "json", "."]);
    let from_domain = project.run(&["--format", "json", "apps/wallet"]);
    let from_file = project.run(&["--format", "json", "apps/wallet/src/a.ts"]);

    for output in [&from_root, &from_domain, &from_file] {
        assert_eq!(code(output), 1, "{}", stderr(output));
        assert_eq!(report(output)["breaches"], 1);
        assert_eq!(
            paths(output),
            vec!["apps/wallet/src/a.ts"],
            "spec files stay excluded"
        );
    }

    // The thresholds a report claims are the ones that gated it: the root's across domains,
    // the domain's own when the scan stayed inside one.
    assert_eq!(report(&from_root)["thresholds"]["typescript"], 15);
    assert_eq!(report(&from_domain)["thresholds"]["typescript"], 2);
    assert_eq!(report(&from_file)["thresholds"]["typescript"], 2);
}

#[test]
fn stdin_resolves_the_same_workspace_as_a_file_scan() {
    let project = Project::new();
    project
        .file(
            "bonsai-lint.toml",
            "domains = [\"apps/*\"]\nthreshold = 15\n",
        )
        .file("apps/wallet/bonsai-lint.toml", "threshold = 2\n")
        .file("apps/wallet/src/a.ts", BUSY_TS);

    let from_disk = project.run(&["--format", "json", "apps/wallet/src/a.ts"]);
    let from_stdin = project.run_with_stdin(
        &[
            "--format",
            "json",
            "--stdin",
            "--stdin-path",
            "apps/wallet/src/a.ts",
        ],
        BUSY_TS,
    );

    assert_eq!(code(&from_disk), 1);
    assert_eq!(code(&from_stdin), 1, "{}", stderr(&from_stdin));
    assert_eq!(
        report(&from_disk)["thresholds"],
        report(&from_stdin)["thresholds"]
    );
    assert_eq!(paths(&from_disk), paths(&from_stdin));
}

#[test]
fn a_stray_config_is_reported_only_when_the_scan_passed_through_it() {
    let project = Project::new();
    project
        .file(
            "bonsai-lint.toml",
            "domains = [\"apps/*\"]\nthreshold = 15\n",
        )
        .file("packages/stray/bonsai-lint.toml", "threshold = 1\n")
        .file("packages/stray/src/a.php", CALM_PHP)
        .file("packages/clean/src/a.php", CALM_PHP);

    let through = project.run(&["packages/stray/src"]);
    let elsewhere = project.run(&["packages/clean"]);

    assert!(
        stderr(&through).contains("not a declared domain"),
        "{}",
        stderr(&through)
    );
    assert!(stderr(&elsewhere).is_empty(), "{}", stderr(&elsewhere));
}

#[test]
fn write_baseline_says_so_when_there_is_nothing_to_record() {
    let project = Project::new();
    project.file("src/calm.php", CALM_PHP);

    let output = project.run(&["--write-baseline", "."]);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(
        stdout(&output).contains("no baseline written"),
        "{}",
        stdout(&output)
    );
    assert!(!project.root.join(".bonsai-lint-baseline.json").exists());
}

#[test]
fn stdin_ranks_units_like_a_scan_from_disk() {
    let source = "<?php\nfunction calm() { return 1; }\nfunction busy($a, $b) {\n    if ($a) { if ($b) { return 1; } }\n    return 0;\n}\n";
    let project = Project::new();
    project.file("src/a.php", source);

    let from_disk = project.run(&["--all", "--format", "json", "src/a.php"]);
    let from_stdin = project.run_with_stdin(
        &[
            "--all",
            "--format",
            "json",
            "--stdin",
            "--stdin-path",
            "src/a.php",
        ],
        source,
    );

    assert_eq!(ranked_scores(&from_disk), vec![3, 0]);
    assert_eq!(ranked_scores(&from_stdin), ranked_scores(&from_disk));
}

#[test]
fn paths_print_the_same_however_the_scan_root_is_spelled() {
    let project = Project::new();
    project.file("src/a.php", BUSY_PHP);

    let spellings = [".", "./src", "src", "src/", "./src/a.php"];
    for spelling in spellings {
        let output = project.run(&["--format", "json", "--over", "1", spelling]);
        assert_eq!(paths(&output), vec!["src/a.php"], "spelled as {spelling}");
    }

    let text = project.run(&["--over", "1", "."]);
    assert!(
        stdout(&text).contains("  src/a.php:2  "),
        "{}",
        stdout(&text)
    );
}

#[test]
fn a_file_level_marker_is_honoured_and_a_bare_one_refused_out_loud() {
    let project = Project::new();
    project
        .file(
            "src/bootstrap.php",
            "<?php // bonsai-lint-ignore: legacy bootstrap\nif ($a) { if ($b) { echo 1; } }\n",
        )
        .file(
            "src/bare.php",
            "<?php // bonsai-lint-ignore\nif ($a) { if ($b) { echo 1; } }\n",
        );

    let honoured = project.run(&["--over", "1", "src/bootstrap.php"]);
    assert_eq!(code(&honoured), 0, "{}", stderr(&honoured));
    assert!(stdout(&honoured).is_empty(), "{}", stdout(&honoured));

    let refused = project.run(&["--over", "1", "src/bare.php"]);
    assert_eq!(code(&refused), 1);
    assert!(
        stderr(&refused).contains("needs a reason"),
        "{}",
        stderr(&refused)
    );
    assert!(
        stderr(&refused).contains("<toplevel>"),
        "{}",
        stderr(&refused)
    );
    assert!(
        stdout(&refused).contains("<toplevel>"),
        "{}",
        stdout(&refused)
    );
}
