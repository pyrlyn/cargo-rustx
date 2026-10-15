rustx::sugar! {
    mod rows {
        use std::collections::{BTreeMap, HashSet};

        pub struct Row {
            pub id: u32,
            pub tags: Vec<String>,
            pub score: f32,
        }

        pub fn parse_row(line: &str) -> Option<Row> {
            let parts = line.split(';').map(str::trim).collect<Vec>();
            if parts.len() != 3 {
                return None;
            }
            let id = parts[0].parse<u32>().ok()?;
            let tags = parts[1].split(',').map(|t| t.to_lowercase()).collect<Vec<String>>();
            let score = parts[2].parse<f32>().ok()?;
            Some(Row { id, tags, score })
        }

        pub fn all_tags(rows: &[Row]) -> Vec<String> {
            let set = rows.iter().flat_map(|r| r.tags.iter().cloned()).collect<HashSet>();
            let mut v = set.into_iter().collect<Vec>();
            v.sort();
            v
        }

        pub fn by_id(rows: Vec<Row>) -> BTreeMap<u32, Row> {
            rows.into_iter().map(|r| (r.id, r)).collect<BTreeMap>()
        }

        pub fn total(rows: &[Row]) -> f32 {
            rows.iter().map(|r| r.score).sum<f32>()
        }

        pub fn shorter(a: &[u8], b: &[u8]) -> bool {
            a.len() < b.len()
        }
    }
}

fn main() {
    let rows: Vec<rows::Row> = ["1; Rust,Macro; 1.5", "2; rust; 2.0"]
        .iter()
        .filter_map(|line| rows::parse_row(line))
        .collect();
    assert!(rows::parse_row("bad").is_none());
    assert_eq!(rows::all_tags(&rows), ["macro", "rust"]);
    assert!((rows::total(&rows) - 3.5).abs() < f32::EPSILON);
    let by_id = rows::by_id(rows);
    assert_eq!(by_id.keys().copied().collect::<Vec<_>>(), [1, 2]);
    assert_eq!(by_id[&2].tags, ["rust"]);
    assert!(rows::shorter(b"a", b"ab"));
}
