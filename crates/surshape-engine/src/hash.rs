//! Hash estable para la cache de renders: FNV-1a de 128 bit sobre una
//! codificación explícita (no depende de la versión de Rust ni de la
//! plataforma, a diferencia de `DefaultHasher`). No es criptográfico: alcanza
//! para identificar renders, no para resistir colisiones provocadas.

const OFFSET: u128 = 0x6c62272e07bb014262b821756295c58d;
const PRIME: u128 = 0x0000000001000000000000000000013B;

#[derive(Clone, Debug)]
pub struct StableHasher(u128);

impl Default for StableHasher {
    fn default() -> Self {
        Self(OFFSET)
    }
}

impl StableHasher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bytes(&mut self, b: &[u8]) {
        for &x in b {
            self.0 ^= x as u128;
            self.0 = self.0.wrapping_mul(PRIME);
        }
    }

    pub fn u8(&mut self, v: u8) {
        self.bytes(&[v]);
    }

    pub fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes());
    }

    /// -0 y 0 se igualan; todos los NaN también.
    pub fn f64(&mut self, v: f64) {
        let v = if v == 0.0 { 0.0 } else if v.is_nan() { f64::NAN } else { v };
        self.u64(v.to_bits());
    }

    /// Cadena con prefijo de largo (así "ab"+"c" != "a"+"bc").
    pub fn str(&mut self, s: &str) {
        self.u64(s.len() as u64);
        self.bytes(s.as_bytes());
    }

    /// 32 dígitos hexadecimales.
    pub fn finish_hex(&self) -> String {
        format!("{:032x}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vector_and_separation() {
        // FNV-1a 128 de "" es el offset; de "a" es el valor publicado.
        assert_eq!(StableHasher::new().finish_hex(), "6c62272e07bb014262b821756295c58d");
        let mut h = StableHasher::new();
        h.bytes(b"a");
        assert_eq!(h.finish_hex(), "d228cb696f1a8caf78912b704e4a8964");
        let (mut a, mut b) = (StableHasher::new(), StableHasher::new());
        a.str("ab");
        a.str("c");
        b.str("a");
        b.str("bc");
        assert_ne!(a.finish_hex(), b.finish_hex());
        let (mut z, mut nz) = (StableHasher::new(), StableHasher::new());
        z.f64(0.0);
        nz.f64(-0.0);
        assert_eq!(z.finish_hex(), nz.finish_hex());
    }
}
