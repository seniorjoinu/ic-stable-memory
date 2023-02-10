use rand::rngs::ThreadRng;
use rand::Rng;

const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                            abcdefghijklmnopqrstuvwxyz\
                            0123456789)(*&^%$#@!~";

pub fn generate_random_string(rng: &mut ThreadRng) -> String {
    let len = rng.gen_range(10..1000usize);

    (0..len)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}
