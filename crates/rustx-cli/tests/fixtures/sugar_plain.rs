// Nothing here is a shape that sugar writes, so the file comes back as it is.
pub fn sum(xs: &[i32]) -> i32 {
    let total: i32 = xs.iter().sum();
    let (mut lo, hi) = (0, total);
    lo  +=  hi; /* odd spacing stays */
    lo
}
