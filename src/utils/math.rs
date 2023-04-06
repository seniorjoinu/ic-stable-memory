/// Efficient ceiling division of [u64]
///
/// # Important
/// Can overflow if a + b > [u64::MAX]
#[inline]
pub fn ceil_div(a: u64, b: u64) -> u64 {
    (a + b - 1) / b
}

/// Pseudo-randomly shuffles bits of [u32] number.
/// Function from the "Xorshift RNGs" paper by George Marsaglia.
#[inline]
pub fn shuffle_bits(mut seed: u32) -> u32 {
    seed ^= seed << 13;
    seed ^= seed >> 17;
    seed ^= seed << 5;
    seed
}

/// Constant analog to [usize::max] function
#[inline]
pub const fn max_usize(a: usize, b: usize) -> usize {
    if a > b {
        a
    } else {
        b
    }
}
