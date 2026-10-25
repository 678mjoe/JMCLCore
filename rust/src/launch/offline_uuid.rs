//! Minecraft's legacy UUID v3: MD5 of UTF-8 `OfflinePlayer:` + name.
//! Identity compatibility only; MD5 is not used for security or artifact integrity.
pub fn uuid(name: &str) -> String {
    let mut bytes = b"OfflinePlayer:".to_vec();
    bytes.extend_from_slice(name.as_bytes());
    let bits = (bytes.len() as u64).wrapping_mul(8);
    bytes.push(0x80);
    while bytes.len() % 64 != 56 {
        bytes.push(0)
    }
    bytes.extend_from_slice(&bits.to_le_bytes());
    let mut h = [0x67452301u32, 0xefcdab89, 0x98badcfe, 0x10325476];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
        0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
        0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
        0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
        0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
        0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
        0xeb86d391,
    ];
    for block in bytes.as_chunks::<64>().0 {
        let mut words = [0u32; 16];
        for (i, w) in words.iter_mut().enumerate() {
            *w = u32::from_le_bytes(block[i * 4..i * 4 + 4].try_into().unwrap())
        }
        let [a0, b0, c0, d0] = h;
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g, s) = match i {
                0..=15 => ((b & c) | (!b & d), i, [7, 12, 17, 22][i % 4]),
                16..=31 => ((d & b) | (!d & c), (5 * i + 1) % 16, [5, 9, 14, 20][i % 4]),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16, [4, 11, 16, 23][i % 4]),
                _ => (c ^ (b | !d), (7 * i) % 16, [6, 10, 15, 21][i % 4]),
            };
            let next = b.wrapping_add(
                a.wrapping_add(f)
                    .wrapping_add(K[i])
                    .wrapping_add(words[g])
                    .rotate_left(s),
            );
            a = d;
            d = c;
            c = b;
            b = next;
        }
        h = [
            a0.wrapping_add(a),
            b0.wrapping_add(b),
            c0.wrapping_add(c),
            d0.wrapping_add(d),
        ];
    }
    let mut digest = [0u8; 16];
    for (i, w) in h.iter().enumerate() {
        digest[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes())
    }
    digest[6] = (digest[6] & 0x0f) | 0x30;
    digest[8] = (digest[8] & 0x3f) | 0x80;
    digest.iter().map(|b| format!("{b:02x}")).collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn java_name_uuid() {
        assert_eq!(uuid("OfflinePlayer"), "cb7ecbf7173e39428f9fb92995eab3d0");
        assert_eq!(uuid("Notch"), "b50ad385829d3141a2167e7d7539ba7f");
    }
}
