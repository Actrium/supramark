//! Adler-32 checksum (RFC 1950 §8). 7-line implementation, no SIMD tricks —
//! we don't need throughput, we need exact-match output.

const MOD: u32 = 65521;

pub(crate) struct Adler32 {
    a: u32,
    b: u32,
}

impl Adler32 {
    pub(crate) fn new() -> Self {
        Self { a: 1, b: 0 }
    }

    pub(crate) fn update(mut self, bytes: &[u8]) -> Self {
        for &x in bytes {
            self.a = (self.a + x as u32) % MOD;
            self.b = (self.b + self.a) % MOD;
        }
        self
    }

    pub(crate) fn finalize(self) -> u32 {
        (self.b << 16) | self.a
    }
}
