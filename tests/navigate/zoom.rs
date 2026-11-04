use ripwire_rs::navigate::zoom::{build_zoom_tree, resolve_levels_shown, serialize_zoom_xml};
use ripwire_rs::serialize::{GraphGauges, PageRequest};

fn three_level_fixture() -> (Vec<Vec<u32>>, Vec<&'static str>) {
    let l0 = vec![0, 0, 1, 1, 2, 2, 3, 3, 4];
    let l1 = vec![0, 0, 0, 0, 1, 1, 1, 1, 2];
    let l2 = vec![0, 0, 0, 0, 0, 0, 0, 0, 1];
    let dirs = vec![
        "src/a", "src/a", "src/a", "src/b", "src/c", "src/c", "src/c", "src/d", "x",
    ];
    (vec![l0, l1, l2], dirs)
}

#[test]
fn test_levels_shown_defaults_overrides_and_all() {
    assert_eq!(resolve_levels_shown(4, None), 2);
    assert_eq!(resolve_levels_shown(1, None), 1);
    assert_eq!(resolve_levels_shown(4, Some(0)), 4);
    assert_eq!(resolve_levels_shown(4, Some(3)), 3);
    assert_eq!(resolve_levels_shown(4, Some(9)), 4);
}

#[test]
fn test_zoom_tree_nests_to_the_cut_and_counts_unprinted_children() {
    let (hier, dirs) = three_level_fixture();
    let tree = build_zoom_tree(&hier, &dirs, None);
    assert_eq!((tree.levels, tree.levels_shown, tree.symbols), (3, 2, 9));
    assert_eq!(tree.top.len(), 1);
    assert_eq!(tree.isolated, 1);
    let top = &tree.top[0];
    assert_eq!((top.level, top.id, top.size), (2, 0, 8));
    assert_eq!(top.children.len(), 2);
    let first = &top.children[0];
    assert_eq!(
        (first.level, first.size, first.dir.as_str()),
        (1, 4, "src/a")
    );
    assert!(first.children.is_empty());
    assert_eq!(first.unprinted_children, 2);
    assert_eq!(top.children[1].dir, "src/c");

    let all = build_zoom_tree(&hier, &dirs, Some(0));
    assert_eq!(all.levels_shown, 3);
    assert_eq!(all.top[0].children[0].children.len(), 2);
    assert_eq!(all.top[0].children[0].children[0].unprinted_children, 0);
}

#[test]
fn test_zoom_xml_reports_computed_levels_and_window() {
    let (hier, dirs) = three_level_fixture();
    let tree = build_zoom_tree(&hier, &dirs, None);
    let window = PageRequest::default().window_fit(tree.top.len(), |_| 1);
    let mut out = Vec::new();
    serialize_zoom_xml(&tree, &window, 7, GraphGauges::default(), &mut out).unwrap();
    let xml = String::from_utf8(out).unwrap();
    assert!(xml.contains("levels=\"3\" levels_shown=\"2\" top_modules=\"1\" symbols=\"9\" isolated=\"1\" shown=\"1\" capped=\"0\" pr_iters=\"7\""));
    assert!(xml.contains("<module level=\"2\" id=\"0\" size=\"8\" dir=\"src/a\"><module level=\"1\" id=\"0\" size=\"4\" dir=\"src/a\" children=\"2\">"));
    assert_eq!(xml.matches("<module level=\"").count(), 3);
}
