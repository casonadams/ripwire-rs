use ripwire_rs::gate::merge_scout::{
    ArmChanges, ChangedFiles, evaluate_merge_scout, serialize_merge_scout_xml, touched_symbol_names,
};
use ripwire_rs::ingest::pipeline::file_definitions;
use std::collections::BTreeSet;

const LIB: &str = "src/lib.rs";
const SRC: &[u8] = b"fn alpha() {\n    1\n}\n\nfn beta() {\n    2\n}\n";

fn files(entries: &[(&str, &[&str])]) -> ChangedFiles {
    entries
        .iter()
        .map(|(path, names)| {
            (
                (*path).to_string(),
                names
                    .iter()
                    .map(|n| (*n).to_string())
                    .collect::<BTreeSet<_>>(),
            )
        })
        .collect()
}

fn arm(label: &str, base: &str, changed: ChangedFiles, head: ChangedFiles) -> ArmChanges {
    ArmChanges {
        label: label.to_string(),
        base: Some(base.to_string()),
        files: changed,
        head_files: head,
    }
}

fn xml(arms: &[ArmChanges]) -> String {
    let rep = evaluate_merge_scout("abc123def", "abc123def", arms);
    let mut out = Vec::new();
    serialize_merge_scout_xml(&rep, &mut out).expect("serialize");
    String::from_utf8(out).expect("utf-8")
}

/// The two arms of one merge pair on the shared base, touching the given symbols of `LIB`.
fn two_arms(first: &[&str], second: &[&str]) -> [ArmChanges; 2] {
    [
        arm(
            "feat-a",
            "b1b1b1",
            files(&[(LIB, first)]),
            ChangedFiles::new(),
        ),
        arm(
            "feat-b",
            "b1b1b1",
            files(&[(LIB, second)]),
            ChangedFiles::new(),
        ),
    ]
}

#[test]
fn test_arms_touching_same_symbol_conflict() {
    let arms = two_arms(&["alpha"], &["alpha"]);
    let rep = evaluate_merge_scout("abc123def", "abc123def", &arms);
    assert_eq!(rep.pairs.len(), 1);
    assert_eq!(rep.pairs[0].conflicts.len(), 1);
    assert_eq!(rep.pairs[0].conflicts[0].name, "alpha");
    assert_eq!(rep.pairs[0].risks, 0);
    let text = xml(&arms);
    assert!(text.contains("<pair a=\"feat-a\" b=\"feat-b\" conflicts=\"1\" risks=\"0\">"));
    assert!(text.contains("<sym p=\"src/lib.rs\" n=\"alpha\"/>"));
}

#[test]
fn test_disjoint_symbols_in_same_file_are_risk_not_conflict() {
    let arms = two_arms(&["alpha"], &["beta"]);
    let rep = evaluate_merge_scout("abc123def", "abc123def", &arms);
    assert!(rep.pairs[0].conflicts.is_empty());
    assert_eq!(rep.pairs[0].risks, 1, "same file, different symbols");
    let text = xml(&arms);
    assert!(text.contains("<pair a=\"feat-a\" b=\"feat-b\" conflicts=\"0\" risks=\"1\"/>"));
}

#[test]
fn test_arm_without_divergent_work_is_no_work_and_has_no_landing_slot() {
    let idle = arm(
        "feat-idle",
        "b1b1b1",
        ChangedFiles::new(),
        ChangedFiles::new(),
    );
    let busy = arm(
        "feat-busy",
        "b1b1b1",
        files(&[(LIB, &["alpha"])]),
        ChangedFiles::new(),
    );
    let rep = evaluate_merge_scout("abc123def", "abc123def", &[idle.clone(), busy]);
    assert_eq!(rep.landing_order, vec!["feat-busy".to_string()]);
    assert_eq!(
        rep.pairs.len(),
        1,
        "every arm that ran is paired, work or not"
    );
    assert!(rep.pairs[0].conflicts.is_empty());
    assert_eq!(rep.pairs[0].risks, 0, "nothing shared with the idle arm");
    let text = xml(&[idle]);
    assert!(text.contains("<arm ref=\"feat-idle\" base=\"b1b1b1\" ok=\"1\" changed=\"0\" head_conflicts=\"0\"><no-work note="));
    assert!(text.contains("<landing order=\"\"/>"));
}

#[test]
fn test_head_versus_head_is_no_work() {
    let head_self = arm(
        "HEAD",
        "abc123def",
        ChangedFiles::new(),
        ChangedFiles::new(),
    );
    let rep = evaluate_merge_scout("abc123def", "abc123def", std::slice::from_ref(&head_self));
    assert_eq!(rep.arms.len(), 1);
    assert_eq!(rep.arms[0].files, 0);
    assert!(rep.landing_order.is_empty());
    assert!(xml(&[head_self]).contains("<no-work"));
}

#[test]
fn test_arm_with_no_shared_history_is_reported_not_compared() {
    let orphan = ArmChanges {
        label: "orphan".to_string(),
        base: None,
        files: ChangedFiles::new(),
        head_files: ChangedFiles::new(),
    };
    let text = xml(&[orphan]);
    assert!(text.contains("<arm ref=\"orphan\" ok=\"0\""));
    let arm_line = text
        .lines()
        .find(|l| l.contains("ref=\"orphan\""))
        .expect("orphan arm row");
    assert!(
        !arm_line.contains("changed="),
        "no compared counts for an arm that did not run: {arm_line}"
    );
}

#[test]
fn test_head_changed_same_symbol_counts_as_head_conflict() {
    let a = arm(
        "feat-a",
        "b1b1b1",
        files(&[(LIB, &["alpha"])]),
        files(&[(LIB, &["alpha"])]),
    );
    let rep = evaluate_merge_scout("abc123def", "abc123def", &[a]);
    assert_eq!(rep.arms[0].head_conflicts, 1);
    assert!(
        xml(&[arm(
            "feat-a",
            "b1b1b1",
            files(&[(LIB, &["alpha"])]),
            files(&[(LIB, &["alpha"])]),
        )])
        .contains("head_conflicts=\"1\"")
    );
}

#[test]
fn test_landing_puts_fewest_conflicts_first() {
    let clash = arm(
        "clash",
        "b1b1b1",
        files(&[(LIB, &["alpha"])]),
        ChangedFiles::new(),
    );
    let clash2 = arm(
        "clash2",
        "b1b1b1",
        files(&[(LIB, &["alpha"])]),
        ChangedFiles::new(),
    );
    let clean = arm(
        "clean",
        "b1b1b1",
        files(&[(LIB, &["beta"])]),
        ChangedFiles::new(),
    );
    let rep = evaluate_merge_scout("abc123def", "abc123def", &[clash, clash2, clean]);
    assert_eq!(rep.landing_order, vec!["clean", "clash", "clash2"]);
}

#[test]
fn test_touched_symbols_attribute_changed_line_to_innermost_definition() {
    let defs = file_definitions(
        "src/thing.rs",
        b"struct Thing;\nimpl Thing {\n    fn first(&self) {}\n    fn second(&self) {}\n}\n",
    );
    let names = touched_symbol_names(
        b"struct Thing;\nimpl Thing {\n    fn first(&self) {}\n    fn second(&self) {}\n}\n",
        &defs,
        std::slice::from_ref(&(2..3)),
    );
    assert!(names.iter().any(|n| n.ends_with("first")), "got {names:?}");
    assert!(
        !names.iter().any(|n| n.ends_with("second")),
        "got {names:?}"
    );
}

#[test]
fn test_touched_symbols_name_nothing_outside_any_definition() {
    let defs = file_definitions(LIB, SRC);
    let names = touched_symbol_names(SRC, &defs, std::slice::from_ref(&(3..4)));
    assert!(names.is_empty(), "blank line between fns: {names:?}");
}
