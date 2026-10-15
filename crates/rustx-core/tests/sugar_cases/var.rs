// Problem 02's "As is" example, with comments that must survive the conversion.
pub struct Stats {
    pub count: usize,
    pub sum: f64,
    pub min: f64,
    pub max: f64,
}

/// Summary of the samples, or `None` when there are none.
pub fn summarize(samples: &[f64]) -> Option<Stats> {
    if samples.is_empty() {
        return None;
    }
    let mut count = 0; // samples seen
    let mut sum = 0.0;
    let mut min = f64::MAX;
    let mut max: f64 = f64::MIN;
    for &s in samples {
        count += 1;
        sum += s;
        if s < min {
            min = s;
        }
        if s > max {
            max = s;
        }
    }
    Some(Stats { count, sum, min, max })
}

pub fn histogram(samples: &[f64], buckets: usize) -> Vec<usize> {
    let mut out = vec![0; buckets];
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let lo = sorted[0];
    let hi = sorted[sorted.len() - 1];
    let width = (hi - lo) / buckets as f64;
    for s in sorted {
        let mut idx = ((s - lo) / width) as usize;
        if idx >= buckets {
            idx = buckets - 1;
        }
        out[idx] += 1;
    }
    out
}

// Destructuring, `let ref mut` and `if let` stay; only the plain `let mut late` changes.
pub fn untouched(pair: (u8, u8), o: Option<u8>) -> u8 {
    let (mut a, b) = pair;
    let ref mut r = 0u8;
    if let Some(mut v) = o {
        v += 1;
        a += v;
    }
    let mut late;
    late = a + b + *r;
    late
}
