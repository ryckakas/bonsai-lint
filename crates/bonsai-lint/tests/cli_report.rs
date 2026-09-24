//! Exit codes, output shapes and the flags that change what a scan means.

use std::fmt::Write as _;
use std::process::Stdio;

mod common;

use common::{code, paths, report, scores, stderr, stdout, Project, BUSY_PHP, CALM_PHP, CALM_TS};
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
    assert_eq!(thresholds, vec!["go", "php", "typescript", "vue"]);
}

#[test]
fn help_offers_every_language_built_in() {
    let output = Project::new().run(&["--help"]);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let help = stdout(&output);
    for id in ["go", "php", "typescript", "vue"] {
        assert!(help.contains(&format!("`{id}`")), "{help}");
    }
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

/// Go has no file-scope calls to hang a closure on, so the same claim is made with declared
/// functions.
#[test]
fn identical_logic_scores_the_same_in_php_javascript_and_go() {
    let project = Project::new();
    project
        .file(
            "php/logic.php",
            "<?php\nfunction a($a) { if ($a) { return 1; } return 0; }\nfunction b($xs) { foreach ($xs as $x) { if ($x) { echo 1; } } }\n",
        )
        .file(
            "js/logic.js",
            "function a(a) { if (a) { return 1; } return 0; }\nfunction b(xs) { for (const x of xs) { if (x) { f(); } } }\n",
        )
        .file(
            "go/logic.go",
            "package logic\n\nfunc a(a bool) int {\n\tif a {\n\t\treturn 1\n\t}\n\treturn 0\n}\n\nfunc b(xs []bool) {\n\tfor _, x := range xs {\n\t\tif x {\n\t\t\tf()\n\t\t}\n\t}\n}\n",
        );

    let php = project.run(&["--all", "--format", "json", "php"]);
    let js = project.run(&["--all", "--format", "json", "js"]);
    let go = project.run(&["--all", "--format", "json", "go"]);

    assert_eq!(scores(&php), vec![1, 3]);
    assert_eq!(scores(&php), scores(&js));
    assert_eq!(scores(&php), scores(&go));
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
