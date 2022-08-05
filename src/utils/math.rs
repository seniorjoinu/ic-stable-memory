use std::num::Wrapping;

const TAB64: [u64; 64] = [
    63, 0, 58, 1, 59, 47, 53, 2, 60, 39, 48, 27, 54, 33, 42, 3, 61, 51, 37, 40, 49, 18, 28, 20, 55,
    30, 34, 11, 43, 14, 22, 4, 62, 57, 46, 52, 38, 26, 32, 41, 50, 36, 17, 19, 29, 10, 13, 21, 56,
    45, 25, 31, 35, 16, 9, 12, 44, 24, 15, 8, 23, 7, 6, 5,
];

pub fn fast_log2_64(mut value: u64) -> u64 {
    value |= value >> 1;
    value |= value >> 2;
    value |= value >> 4;
    value |= value >> 8;
    value |= value >> 16;
    value |= value >> 32;

    TAB64[(((value - (value >> 1)) * 0x07EDD5E59A4E28C2) >> 58) as usize]
}

const TAB32: [u32; 32] = [
    0, 9, 1, 10, 13, 21, 2, 29, 11, 14, 16, 18, 22, 25, 3, 30, 8, 12, 20, 28, 15, 17, 24, 7, 19,
    27, 23, 6, 26, 5, 4, 31,
];

pub fn fast_log2_32(mut value: u32) -> u32 {
    value |= value >> 1;
    value |= value >> 2;
    value |= value >> 4;
    value |= value >> 8;
    value |= value >> 16;

    TAB32[((value * 0x07C4ACDD) >> 27) as usize]
}

pub fn fast_log2(value: usize) -> u32 {
    if usize::MAX == u32::MAX as usize {
        fast_log2_32(value as u32) as u32
    } else {
        fast_log2_64(value as u64) as u32
    }
}
