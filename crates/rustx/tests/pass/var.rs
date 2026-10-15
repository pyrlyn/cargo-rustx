rustx::sugar! {
    pub fn summarize(samples: &[f64]) -> (usize, f64, f64, f64) {
        var count = 0;
        var sum = 0.0;
        var min = f64::MAX;
        var max = f64::MIN;
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
        (count, sum, min, max)
    }

    pub fn histogram(samples: &[f64], buckets: usize) -> Vec<usize> {
        var out = vec![0; buckets];
        var sorted = samples.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let lo = sorted[0];
        let hi = sorted[sorted.len() - 1];
        let width = (hi - lo) / buckets as f64;
        for s in sorted {
            var idx: usize = ((s - lo) / width) as usize;
            if idx >= buckets {
                idx = buckets - 1;
            }
            out[idx] += 1;
        }
        out
    }

    // `var` stays usable as an ordinary identifier.
    fn var(var: i32) -> i32 {
        var + 1
    }

    fn first_digit(s: &str) -> u32 {
        var d @ 0..=9 = s.chars().next().and_then(|c| c.to_digit(10)).unwrap_or(0) else {
            return 0;
        };
        d += 1;
        d
    }
}

fn main() {
    assert_eq!(summarize(&[1.0, 4.0, 2.0]), (3, 7.0, 1.0, 4.0));
    assert_eq!(histogram(&[0.0, 1.0, 2.0, 3.0], 2), vec![2, 2]);
    assert_eq!(var(1), 2);
    assert_eq!(first_digit("7x"), 8);
}
