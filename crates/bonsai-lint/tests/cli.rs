//! The contract a CI gate and an editor rely on: exit codes, output shapes and the flags that
//! change what a scan means. Each test drives the real binary against a throwaway project.

use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
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
    assert_eq!(thresholds, vec!["php", "typescript", "vue"]);
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

/// Nested ifs score 1+2+..+depth, which is how these fixtures land either side of a threshold.
fn nested_php(name: &str, depth: usize) -> String {
    nest(&format!("<?php\nfunction {name}($a) {{\n"), "$a", depth)
}

fn nested_ts(name: &str, depth: usize) -> String {
    nest(&format!("function {name}(a) {{\n"), "a", depth)
}

fn nest(header: &str, condition: &str, depth: usize) -> String {
    let mut source = header.to_string();
    for level in 0..depth {
        writeln!(source, "{}if ({condition}) {{", "    ".repeat(level + 1)).expect("string write");
    }
    source.push_str("    return 1;\n");
    for level in (0..depth).rev() {
        writeln!(source, "{}}}", "    ".repeat(level + 1)).expect("string write");
    }
    source.push_str("}\n");
    source
}

fn over_threshold(name: &str) -> String {
    nested_php(name, 6)
}

/// Baits completion-order bugs: a slow first file so the worker holding index 0 finishes last,
/// ties that `rank` cannot break, and a warning with a position to be wrong about.
fn racy_project() -> Project {
    let project = Project::new();
    project.file("bonsai-lint.toml", "exclude = [\"skipped/**\"]\n");

    let mut heavy = String::from("<?php\n");
    for index in 0..4000 {
        writeln!(
            heavy,
            "function h{index}($a) {{ if ($a) {{ return 1; }} return 0; }}"
        )
        .expect("string write");
    }
    project.file("aaa_heavy.php", &heavy);

    let mut deep = String::from("<?php\n$x = 'a'");
    for _ in 0..30_000 {
        deep.push_str(" . 'a'");
    }
    deep.push_str(";\n");
    project.file("deep.php", &deep);

    for index in 0..40 {
        project.file(&format!("src/tie{index}.php"), &over_threshold("tie"));
        project.file(&format!("src/calm{index}.ts"), CALM_TS);
    }

    let unit = "function ($a) { if ($a) { if ($a) { if ($a) { if ($a) { if ($a) { if ($a) { return 1; } } } } } } }";
    project.file(
        "src/same_line.php",
        &format!("<?php\n$a = {unit}; $b = {unit};\n"),
    );
    project.file("src/types.d.ts", "export declare function f(): void;\n");
    project.file("skipped/ignored.php", BUSY_PHP);

    let mut legacy = over_threshold("legacy").into_bytes();
    legacy.extend_from_slice(b"// caf\xE9\n");
    fs::write(project.root.join("src/legacy.php"), legacy).expect("write");

    project
}

/// Repeated because a scheduling bug that reorders output does not do so on every run.
fn assert_matches_serial(project: &Project, format: &str, jobs: &str, serial: &Output) {
    for _ in 0..3 {
        let parallel = project.run(&["--all", "--format", format, "--jobs", jobs, "."]);
        assert_eq!(stdout(&parallel), stdout(serial), "{format} jobs={jobs}");
        assert_eq!(stderr(&parallel), stderr(serial), "{format} jobs={jobs}");
        assert_eq!(code(&parallel), code(serial), "{format} jobs={jobs}");
    }
}

#[test]
fn a_parallel_scan_prints_exactly_what_a_serial_one_prints() {
    let project = racy_project();

    for format in ["text", "json"] {
        let serial = project.run(&["--all", "--format", format, "--jobs", "1", "."]);
        for jobs in ["2", "3", "8", "16", "0", "4096"] {
            assert_matches_serial(&project, format, jobs, &serial);
        }
    }
}

#[test]
fn a_parallel_scan_finds_the_same_breaches_a_serial_one_finds() {
    let project = racy_project();

    let serial = project.run(&["--format", "json", "--jobs", "1", "."]);
    assert_eq!(code(&serial), 1, "{}", stderr(&serial));
    assert!(
        report(&serial)["breaches"].as_u64().expect("breaches") > 40,
        "the fixture must actually breach the threshold"
    );
    assert!(
        stderr(&serial).contains("not valid UTF-8"),
        "the fixture must exercise a warning: {}",
        stderr(&serial)
    );

    let parallel = project.run(&["--format", "json", "--jobs", "8", "."]);
    assert_eq!(stdout(&parallel), stdout(&serial));
    assert_eq!(stderr(&parallel), stderr(&serial));
}

#[test]
fn a_baseline_is_the_same_file_however_many_workers_wrote_it() {
    let project = racy_project();
    project.file("src/twice.php", &over_threshold("same"));

    let serial = project.run(&["--write-baseline", "--jobs", "1", "."]);
    assert_eq!(code(&serial), 0, "{}", stderr(&serial));
    let expected = project.read(".bonsai-lint-baseline.json");

    for jobs in ["2", "8", "16"] {
        let parallel = project.run(&["--write-baseline", "--jobs", jobs, "."]);
        assert_eq!(code(&parallel), 0, "{}", stderr(&parallel));
        assert_eq!(
            project.read(".bonsai-lint-baseline.json"),
            expected,
            "jobs={jobs}"
        );
    }
}

#[test]
fn jobs_is_accepted_alongside_stdin() {
    let project = Project::new();
    project.file("src/a.php", CALM_PHP);

    let output = project.run_with_stdin(
        &["--stdin", "--stdin-path", "src/a.php", "--jobs", "4"],
        BUSY_PHP,
    );

    assert_eq!(code(&output), 0, "{}", stderr(&output));
}

/// A monorepo's shape: several domains at once, each with its own config, threshold and
/// baseline. The engine tests cover two domains; the combination below is what a real repository
/// runs and what only ever got checked by hand.
const DOMAINS: &[&str] = &[
    "apps/lenient",
    "apps/strict",
    "apps/flat",
    "apps/plain",
    "packages/scoped",
    "packages/plain",
];

/// `heavy` scores 21 and `mid` scores 6, so they straddle the per-domain thresholds below.
fn seed_domain(project: &Project, domain: &str) {
    for index in 0..3 {
        project.file(
            &format!("{domain}/heavy{index}.php"),
            &nested_php(&format!("heavy{index}"), 6),
        );
        project.file(
            &format!("{domain}/mid{index}.ts"),
            &nested_ts(&format!("mid{index}"), 3),
        );
        project.file(&format!("{domain}/calm{index}.php"), CALM_PHP);
    }
}

fn multi_domain_project() -> Project {
    let project = Project::new();
    project.file(
        "bonsai-lint.toml",
        "domains = [\"apps/*\", \"packages/*\"]\nthreshold = 12\nexclude = [\"**/vendor/**\"]\n\n[php]\nthreshold = 20\n\n[typescript]\nthreshold = 8\n",
    );
    project.file("apps/lenient/bonsai-lint.toml", "threshold = 30\n");
    project.file(
        "apps/strict/bonsai-lint.toml",
        "threshold = 5\n\n[typescript]\nthreshold = 4\n",
    );
    project.file("apps/flat/bonsai-lint.toml", "toplevel = false\n");
    project.file(
        "packages/scoped/bonsai-lint.toml",
        "exclude = [\"generated/**\"]\n",
    );

    for domain in DOMAINS {
        seed_domain(&project, domain);
    }

    // Several warnings, spread across the walk: one alone cannot be emitted out of order.
    for (index, domain) in DOMAINS.iter().enumerate().take(4) {
        let mut legacy = nested_php(&format!("legacy{index}"), 6).into_bytes();
        legacy.extend_from_slice(b"// caf\xE9\n");
        fs::write(project.root.join(format!("{domain}/legacy.php")), legacy).expect("write");
    }

    project.file("apps/plain/vendor/bundled.php", &nested_php("bundled", 6));
    project.file(
        "packages/scoped/generated/out.php",
        &nested_php("generated", 6),
    );
    project.file(
        "apps/flat/script.php",
        "<?php\nif ($a) { if ($a) { echo 1; } }\n",
    );
    project.file(
        "apps/plain/script.php",
        "<?php\nif ($a) { if ($a) { echo 1; } }\n",
    );
    project
}

fn collect_baselines(
    root: &Path,
    dir: &Path,
    stack: &mut Vec<PathBuf>,
    found: &mut Vec<(String, String)>,
) {
    for entry in fs::read_dir(dir).expect("read dir").flatten() {
        let path = entry.path();
        if path.is_dir() {
            stack.push(path);
        } else if path
            .file_name()
            .is_some_and(|name| name == ".bonsai-lint-baseline.json")
        {
            let key = path
                .strip_prefix(root)
                .expect("under root")
                .to_string_lossy()
                .replace('\\', "/");
            found.push((key, fs::read_to_string(&path).expect("read baseline")));
        }
    }
}

fn baselines(project: &Project) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut stack = vec![project.root.clone()];
    while let Some(dir) = stack.pop() {
        collect_baselines(&project.root, &dir, &mut stack, &mut found);
    }
    found.sort();
    found
}

fn remove_baselines(project: &Project) {
    for (relative, _) in baselines(project) {
        fs::remove_file(project.root.join(relative)).expect("remove baseline");
    }
}

#[test]
fn many_domains_scan_identically_however_many_workers_run() {
    let project = multi_domain_project();

    let reference = project.run(&["--all", "--format", "json", "--jobs", "1", "."]);
    assert!(
        paths(&reference).len() > 50,
        "the comparison is only meaningful if the scan reports plenty"
    );

    for format in ["text", "json"] {
        let serial = project.run(&["--all", "--format", format, "--jobs", "1", "."]);
        for jobs in ["2", "4", "8", "0"] {
            assert_matches_serial(&project, format, jobs, &serial);
        }
    }
}

#[test]
fn each_domain_applies_its_own_threshold() {
    let project = multi_domain_project();

    let breached = paths(&project.run(&["--format", "json", "."]));

    assert!(
        !breached
            .iter()
            .any(|path| path.starts_with("apps/lenient/")),
        "a threshold of 30 accepts a score of 21: {breached:?}"
    );
    assert!(breached.contains(&"apps/strict/heavy0.php".to_string()));
    assert!(breached.contains(&"apps/plain/heavy0.php".to_string()));

    assert!(
        breached.contains(&"apps/strict/mid0.ts".to_string()),
        "6 is over the domain's typescript threshold of 4"
    );
    assert!(
        !breached.contains(&"apps/plain/mid0.ts".to_string()),
        "6 is under the root's typescript threshold of 8"
    );

    assert!(!breached.iter().any(|path| path.contains("/vendor/")));
    assert!(!breached.iter().any(|path| path.contains("/generated/")));
}

#[test]
fn a_domain_can_turn_off_toplevel_scoring_without_affecting_its_neighbours() {
    let project = multi_domain_project();

    let reported = paths(&project.run(&["--all", "--format", "json", "."]));

    assert!(!reported.contains(&"apps/flat/script.php".to_string()));
    assert!(reported.contains(&"apps/plain/script.php".to_string()));
}

#[test]
fn a_baseline_per_domain_is_written_identically_however_many_workers_run() {
    let project = multi_domain_project();

    let written = project.run(&["--write-baseline", "--jobs", "1", "."]);
    assert_eq!(code(&written), 0, "{}", stderr(&written));

    let expected = baselines(&project);
    let names: Vec<&String> = expected.iter().map(|(path, _)| path).collect();
    assert!(
        expected.len() >= 4,
        "several domains must record their own baseline: {names:?}"
    );
    assert!(names.iter().any(|path| path.starts_with("apps/strict/")));
    assert!(names.iter().any(|path| path.starts_with("packages/plain/")));

    for jobs in ["2", "8", "16"] {
        remove_baselines(&project);
        let parallel = project.run(&["--write-baseline", "--jobs", jobs, "."]);
        assert_eq!(code(&parallel), 0, "{}", stderr(&parallel));
        assert_eq!(baselines(&project), expected, "jobs={jobs}");
    }
}

#[test]
fn every_domain_baseline_is_honoured_at_once() {
    let project = multi_domain_project();
    assert_eq!(code(&project.run(&["--write-baseline", "."])), 0);

    let accepted = project.run(&["--jobs", "8", "."]);

    assert_eq!(code(&accepted), 0, "{}", stderr(&accepted));
    assert_eq!(stdout(&accepted), "");
}

fn inject_ghost(project: &Project, domain: &str) {
    let relative = format!("{domain}/.bonsai-lint-baseline.json");
    let mut baseline: serde_json::Value =
        serde_json::from_str(&project.read(&relative)).expect("baseline is json");
    baseline["entries"]["ghost.php"] = serde_json::json!({ "ghost": 99 });
    fs::write(
        project.root.join(&relative),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&baseline).expect("serialize")
        ),
    )
    .expect("write baseline");
}

#[test]
fn stale_entries_are_reported_for_each_domain_that_has_them() {
    let project = multi_domain_project();
    assert_eq!(code(&project.run(&["--write-baseline", "."])), 0);
    inject_ghost(&project, "apps/strict");
    inject_ghost(&project, "packages/plain");

    let output = project.run(&["--jobs", "8", "."]);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(
        stderr(&output).contains("apps/strict: 1 baseline entr(ies) matched nothing"),
        "{}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("packages/plain: 1 baseline entr(ies) matched nothing"),
        "{}",
        stderr(&output)
    );
}

const BUSY_VUE: &str = "<template>\n  <p v-if=\"a && b\">x</p>\n</template>\n\n<script setup lang=\"ts\">\nfunction busy(a: number, b: number) {\n    if (a) { if (b) { return 1 } }\n    return 0\n}\n</script>\n";

#[test]
fn a_vue_component_is_scored_and_reported_at_its_line_in_the_file() {
    let project = Project::new();
    project.file("src/Panel.vue", BUSY_VUE);

    let output = project.run(&["--over", "0", "--format", "json", "."]);

    let finding = &report(&output)["findings"][0];
    assert_eq!(finding["name"], "busy");
    assert_eq!(finding["language"], "vue");
    assert_eq!(finding["line"], 6);
    assert_eq!(code(&output), 1);
}

#[test]
fn a_vue_threshold_section_applies_only_to_vue() {
    let project = Project::new();
    project.file(
        "bonsai-lint.toml",
        "threshold = 0\n\n[vue]\nthreshold = 99\n",
    );
    project.file("src/Panel.vue", BUSY_VUE);
    project.file("src/plain.ts", BUSY_TS);

    let output = project.run(&["--format", "json", "."]);

    let names: Vec<String> = report(&output)["findings"]
        .as_array()
        .expect("findings is an array")
        .iter()
        .map(|finding| finding["name"].to_string())
        .collect();
    assert_eq!(report(&output)["breaches"], 1);
    assert!(names.iter().any(|name| name.contains("busy")));
}

#[test]
fn an_unsaved_vue_buffer_is_scored_through_stdin() {
    let project = Project::new();

    let output = project.run_with_stdin(
        &[
            "--over",
            "0",
            "--format",
            "json",
            "--stdin",
            "--stdin-path",
            "src/Panel.vue",
        ],
        BUSY_VUE,
    );

    let finding = &report(&output)["findings"][0];
    assert_eq!(finding["language"], "vue");
    assert_eq!(finding["line"], 6);
}
