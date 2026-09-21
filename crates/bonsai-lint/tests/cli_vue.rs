//! Vue components end to end: scoring, lines, configuration and baselines.

mod common;

use common::{
    baselines, code, paths, report, stderr, stdout, vue_component, Project, BUSY_TS, BUSY_VUE,
};
fn vue_domains() -> Project {
    let project = Project::new();
    project.file(
        "bonsai-lint.toml",
        "domains = [\"apps/*\"]\nthreshold = 0\n",
    );
    project.file("apps/lenient/bonsai-lint.toml", "[vue]\nthreshold = 99\n");
    project.file("apps/strict/bonsai-lint.toml", "threshold = 0\n");
    for domain in ["apps/lenient", "apps/strict"] {
        project.file(&format!("{domain}/Panel.vue"), &vue_component("busy", 3, 4));
    }
    project
}

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

    // Both fixtures declare `busy`, so only the path proves which one was reported.
    assert_eq!(paths(&output), vec!["src/plain.ts"]);
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

#[test]
fn a_vue_baseline_round_trips_and_is_then_quiet() {
    let project = Project::new();
    project.file("bonsai-lint.toml", "threshold = 0\n");
    project.file("src/Panel.vue", &vue_component("busy", 3, 5));

    let written = project.run(&["--write-baseline", "."]);
    assert_eq!(code(&written), 0, "{}", stderr(&written));
    assert!(project.read(".bonsai-lint-baseline.json").contains("busy"));

    let rerun = project.run(&["."]);
    assert_eq!(code(&rerun), 0, "{}", stdout(&rerun));
}

/// The property the whole design turns on: a `.vue` baseline key must not drift by the length of
/// a template.
#[test]
fn growing_a_vue_template_does_not_invalidate_its_baseline() {
    let project = Project::new();
    project.file("bonsai-lint.toml", "threshold = 0\n");
    project.file("src/Panel.vue", &vue_component("busy", 3, 5));
    project.run(&["--write-baseline", "."]);
    let recorded = project.read(".bonsai-lint-baseline.json");

    project.file("src/Panel.vue", &vue_component("busy", 3, 40));
    let rerun = project.run(&["."]);

    assert_eq!(code(&rerun), 0, "{}", stdout(&rerun));
    project.run(&["--write-baseline", "."]);
    assert_eq!(recorded, project.read(".bonsai-lint-baseline.json"));
}

#[test]
fn a_worsened_vue_unit_breaks_its_baseline() {
    let project = Project::new();
    project.file("bonsai-lint.toml", "threshold = 0\n");
    project.file("src/Panel.vue", &vue_component("busy", 3, 5));
    project.run(&["--write-baseline", "."]);

    project.file("src/Panel.vue", &vue_component("busy", 6, 5));
    let rerun = project.run(&["."]);

    assert_eq!(code(&rerun), 1);
    assert!(stdout(&rerun).contains("busy"), "{}", stdout(&rerun));
}

#[test]
fn a_stale_vue_baseline_entry_is_reported() {
    let project = Project::new();
    project.file("bonsai-lint.toml", "threshold = 0\n");
    project.file("src/Panel.vue", &vue_component("busy", 3, 5));
    project.run(&["--write-baseline", "."]);

    project.file("src/Panel.vue", "<template><p/></template>\n");
    let rerun = project.run(&["."]);

    assert!(
        stderr(&rerun).contains("matched nothing"),
        "{}",
        stderr(&rerun)
    );
}

#[test]
fn a_domain_scoped_vue_threshold_applies_only_inside_that_domain() {
    let project = vue_domains();

    let output = project.run(&["--format", "json", "."]);

    let paths: Vec<String> = report(&output)["findings"]
        .as_array()
        .expect("findings is an array")
        .iter()
        .map(|finding| finding["path"].to_string())
        .collect();
    assert!(
        paths.iter().all(|path| path.contains("strict")),
        "{paths:?}"
    );
}

#[test]
fn each_domain_writes_its_own_vue_baseline_keyed_relative_to_itself() {
    let project = Project::new();
    project.file(
        "bonsai-lint.toml",
        "domains = [\"apps/*\"]\nthreshold = 0\n",
    );
    for domain in ["apps/lenient", "apps/strict"] {
        project.file(&format!("{domain}/Panel.vue"), &vue_component("busy", 3, 4));
    }

    assert_eq!(code(&project.run(&["--write-baseline", "."])), 0);

    let written = baselines(&project);
    let names: Vec<&str> = written.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "apps/lenient/.bonsai-lint-baseline.json",
            "apps/strict/.bonsai-lint-baseline.json"
        ]
    );

    // A domain baseline is keyed relative to its own root, so the domain name is not in the key.
    for (path, body) in &written {
        assert!(body.contains("\"Panel.vue\""), "{path}: {body}");
        assert!(!body.contains("apps/"), "{path}: {body}");
    }

    assert_eq!(code(&project.run(&["."])), 0);
}

/// `<script setup>` is all top-level code, so a domain turning top-level scoring off changes a
/// Vue component far more than it changes a PHP file.
#[test]
fn a_domain_can_turn_off_toplevel_scoring_for_vue_without_affecting_its_neighbours() {
    let project = Project::new();
    project.file(
        "bonsai-lint.toml",
        "domains = [\"apps/*\"]\nthreshold = 0\n",
    );
    project.file("apps/flat/bonsai-lint.toml", "toplevel = false\n");
    let component =
        "<template><p/></template>\n<script setup lang=\"ts\">\nif (a) { if (b) { c() } }\n</script>\n";
    project.file("apps/flat/Panel.vue", component);
    project.file("apps/plain/Panel.vue", component);

    let output = project.run(&["--format", "json", "."]);

    let toplevel: Vec<String> = report(&output)["findings"]
        .as_array()
        .expect("findings is an array")
        .iter()
        .filter(|finding| finding["name"] == "<toplevel>")
        .map(|finding| finding["path"].to_string())
        .collect();
    assert_eq!(toplevel.len(), 1, "{toplevel:?}");
    assert!(toplevel[0].contains("plain"), "{toplevel:?}");
}

/// End to end: the driver parses each script block on its own, so a comment closing one cannot
/// silently blank the next.
#[test]
fn a_comment_at_the_end_of_a_block_does_not_blank_the_component() {
    let project = Project::new();
    project.file(
        "src/Panel.vue",
        "<script>const x = 1 // note</script>\n\
         <script setup>function busy(n) { if (n) { if (n) { return 1 } } return 0 }</script>\n",
    );

    let output = project.run(&["--over", "0", "--format", "json", "."]);

    assert_eq!(report(&output)["findings"][0]["name"], "busy");
}

/// Each region is parsed on its own, so two regions disambiguate only against themselves unless
/// the driver reconciles them afterwards. Colliding names cannot both be baselined.
#[test]
fn units_colliding_across_script_blocks_are_disambiguated() {
    let project = Project::new();
    project.file("bonsai-lint.toml", "threshold = 0\n");
    project.file(
        "src/Panel.vue",
        "<script lang=\"ts\">\nwatch(a, () => { if (x) { if (y) { return 1 } } return 0 })\n</script>\n\
         <script setup lang=\"ts\">\nwatch(b, () => { if (z) { return 1 } return 0 })\n</script>\n",
    );

    let names: Vec<String> = report(&project.run(&["--format", "json", "."]))["findings"]
        .as_array()
        .expect("findings is an array")
        .iter()
        .map(|finding| finding["name"].to_string())
        .collect();
    assert_eq!(names.len(), 2);
    assert_ne!(names[0], names[1]);

    assert_eq!(code(&project.run(&["--write-baseline", "."])), 0);
    assert_eq!(
        code(&project.run(&["."])),
        0,
        "the file must be baselineable"
    );
}

/// The blocks' file-level code adds up, through the driver's own merge rather than a test copy.
#[test]
fn top_level_code_from_both_blocks_is_summed_by_the_driver() {
    let project = Project::new();
    project.file("bonsai-lint.toml", "threshold = 0\n");
    project.file(
        "src/Panel.vue",
        "<script lang=\"ts\">\nif (a) { b() }\n</script>\n\
         <script setup lang=\"ts\">\nif (c) { if (d) { e() } }\n</script>\n",
    );

    let findings = report(&project.run(&["--format", "json", "."]));
    let findings = findings["findings"]
        .as_array()
        .expect("findings is an array");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["name"], "<toplevel>");
    assert_eq!(findings[0]["score"], 4);
}

/// A marker in any script block applies to the one `<toplevel>` the component reports.
#[test]
fn a_marker_in_the_second_block_suppresses_the_merged_toplevel() {
    let project = Project::new();
    project.file("bonsai-lint.toml", "threshold = 0\n");
    project.file(
        "src/Panel.vue",
        "<script lang=\"ts\">\nif (a) { b() }\n</script>\n\
         <script setup lang=\"ts\">\n// bonsai-lint-ignore: accepted bootstrap\n\
         if (c) { if (d) { e() } }\n</script>\n",
    );

    assert_eq!(
        report(&project.run(&["--format", "json", "."]))["breaches"],
        0
    );
}

/// The host grammar cannot fail spec compilation, so this warning is what stands in for it.
/// `sfc.rs` proves the extractor sets it; this proves it reaches the user.
#[test]
fn a_component_yielding_no_script_block_warns_on_stderr() {
    let project = Project::new();
    project.file(
        "src/Panel.vue",
        "<template><p/></template>\n<!-- <script setup> was here -->\n",
    );

    let output = project.run(&["--over", "0", "."]);

    assert!(
        stderr(&output).contains("no script block could be read"),
        "{}",
        stderr(&output)
    );
}
