use overseer::review::map::{build, parse_tree, Changed, Region};
use std::sync::Arc;

fn regions(scale: usize) -> Vec<Region> {
    (0..3)
        .map(|w| {
            let tree: Vec<(String, u64)> = (0..400 * scale)
                .map(|i| (format!("src/d{}/f{i}.rs", i % 40), 100 + i as u64))
                .collect();
            let changed: Vec<Changed> = (0..20 * scale)
                .map(|i| Changed {
                    path: format!("src/d{}/f{i}.rs", i % 40),
                    add: 5,
                    logic: 4,
                    ..Default::default()
                })
                .collect();
            let edges = (0..20 * scale)
                .map(|i| {
                    (
                        format!("src/d{}/f{i}.rs", i % 40),
                        format!("src/d{}/f{}.rs", (i + 1) % 40, i + 1),
                        "uses",
                    )
                })
                .collect();
            let symbols = (0..20 * scale)
                .map(|i| {
                    (
                        format!("src/d{}/f{i}.rs", i % 40),
                        format!("symbol_{w}_{i}"),
                    )
                })
                .collect();
            Region {
                worktree: format!("/w/{w}"),
                name: format!("w{w}"),
                tree: Arc::new((tree, false)),
                changed,
                edges,
                symbols,
                ..Default::default()
            }
        })
        .collect()
}

#[test]
fn map_assembly_work_grows_linearly_with_the_repository() {
    let (_, small) = build(&regions(1));
    let (_, big) = build(&regions(10));
    assert!(small > 0);
    let ratio = big as f64 / small as f64;
    assert!(ratio <= 11.0, "10x input did {ratio:.1}x the work");
}

#[test]
fn shared_names_across_worktrees_stay_capped_per_name() {
    let mut rs = regions(1);
    for r in &mut rs {
        r.symbols = (0..2000)
            .map(|i| (format!("f{i}.rs"), "push_manifest".to_string()))
            .collect();
    }
    let (v, work) = build(&rs);
    let links = v["links"].as_array().unwrap();
    assert!(links.iter().filter(|l| l["probable"] == true).count() <= 12);
    assert!(work < 50_000, "{work}");
}

#[test]
fn tree_parsing_is_capped() {
    let out: String = (0..50_000)
        .map(|i| format!("100644 blob x 1\tf{i}\0"))
        .collect();
    let (files, cut) = parse_tree(&out);
    assert!(cut);
    assert!(files.len() <= 40_000);
}
