// Problem 13 through `sugar!`: inside `#[widen]`, lossless conversions between written number
// types are implicit; lossy ones keep their `as`.
rustx::sugar! {
    #![widen]
    mod summary {
        #[data]
        pub struct Sample { timestamp_ms: u64, bytes: u32, latency_us: u16 }

        #[data]
        pub struct Summary { total_bytes: u64, avg_latency_us: f64, throughput_bps: f64, window_s: f32 }

        pub fn summarize(samples: &[Sample]) -> Option<Summary> {
            let first = samples.first()?;
            let last = samples.last()?;
            var total_bytes: u64 = 0;
            var latency_sum: u64 = 0;
            for s in samples {
                total_bytes += s.bytes;          // u32 -> u64
                latency_sum += s.latency_us;     // u16 -> u64
            }
            let n = samples.len() as f64;        // lossy: stays explicit
            let window_ms = (last.timestamp_ms - first.timestamp_ms).max(1);
            let window_s = window_ms as f64 / 1000.0;
            Some(Summary::new(
                total_bytes,
                latency_sum as f64 / n,
                total_bytes as f64 * 8.0 / window_s,
                window_s as f32,                 // narrowing: stays explicit
            ))
        }

        pub fn bucket(latency_us: u16) -> usize {
            let v: u32 = latency_us;             // u16 -> u32
            (v / 100) as usize
        }
    }
}

rustx::sugar! {
    mod positions {
        pub struct Counter { pub total: u64, pub peak: i64, pub ratio: f64 }

        #[widen]
        impl Counter {
            pub fn new(total: u64, peak: i64, ratio: f64) -> Self {
                Counter { total, peak, ratio }
            }

            pub fn add(&mut self, n: u32, level: i16) {
                self.total += n;
                self.peak = level;
            }
        }

        pub fn scale(x: u64, by: f64) -> f64 { x as f64 * by }

        #[widen]
        pub fn demo(small: u8, half: f32, c: &mut Counter) -> f64 {
            let wide: u64 = small;
            let mut sum: i32 = 0;
            sum += small;
            c.total += small;
            let fresh = Counter::new(small, -1, half);
            c.ratio = half;
            // Shadowed without a written type: `small` is a plain integer literal from here on.
            let small = 3;
            let still: u64 = small;
            let lossy: f32 = wide as f32;
            sum <<= small;
            scale(wide, half) + fresh.ratio + f64::from(lossy) + still as f64 + f64::from(sum)
        }
    }
}

// Every pair of the widening table, so a pair std does not convert fails to build here.
rustx::sugar! {
    #[widen]
    #[allow(clippy::too_many_arguments)]
    fn table(a: u8, b: u16, c: u32, d: u64, e: i8, f: i16, g: i32, h: i64, x: f32) -> f64 {
        let _: u16 = a; let _: u32 = a; let _: u64 = a; let _: u128 = a; let _: usize = a;
        let _: i16 = a; let _: i32 = a; let _: i64 = a; let _: i128 = a; let _: isize = a;
        let _: f32 = a; let _: f64 = a;
        let _: u32 = b; let _: u64 = b; let _: u128 = b; let _: usize = b;
        let _: i32 = b; let _: i64 = b; let _: i128 = b; let _: f32 = b; let _: f64 = b;
        let _: u64 = c; let _: u128 = c; let _: i64 = c; let _: i128 = c; let _: f64 = c;
        let _: u128 = d; let _: i128 = d;
        let _: i16 = e; let _: i32 = e; let _: i64 = e; let _: i128 = e; let _: isize = e;
        let _: f32 = e; let _: f64 = e;
        let _: i32 = f; let _: i64 = f; let _: i128 = f; let _: isize = f; let _: f32 = f;
        let _: f64 = f;
        let _: i64 = g; let _: i128 = g; let _: f64 = g;
        let _: i128 = h;
        let wide: f64 = x;
        wide
    }
}

// `#[sugared]` runs every AST pass, `widen` among them, so `#[widen]` works under it too.
#[rustx::sugared]
#[widen]
fn through_sugared(n: u16) -> u64 {
    let wide: u64 = n;
    wide
}

fn main() {
    use summary::{Sample, bucket, summarize};

    let samples = [Sample::new(1_000, 1_500, 120), Sample::new(3_000, 500, 80)];
    let summary = summarize(&samples).unwrap();
    assert_eq!(summary.total_bytes(), 2_000);
    assert_eq!(summary.avg_latency_us(), 100.0);
    assert_eq!(summary.throughput_bps(), 8_000.0);
    assert_eq!(summary.window_s(), 2.0);
    assert_eq!(bucket(250), 2);

    let mut counter = positions::Counter::new(1, -2, 0.5);
    counter.add(2, -3);
    assert_eq!((counter.total, counter.peak, counter.ratio), (3, -3, 0.5));
    let result = positions::demo(4, 0.25, &mut counter);
    assert_eq!((counter.total, counter.peak, counter.ratio), (7, -3, 0.25));
    // 4 * 0.25 + 0.25 + 4 + 3 + (4 << 3)
    assert_eq!(result, 40.25);

    assert_eq!(table(1, 2, 3, 4, -5, -6, -7, -8, 1.5), 1.5);
    assert_eq!(through_sugared(7), 7);
}
