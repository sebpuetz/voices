use xsalsa20poly1305::{AeadInPlace, Nonce, XSalsa20Poly1305};

pub use xsalsa20poly1305;

pub trait VoiceCrypto {
    fn decrypt(
        &self,
        voice: &mut Vec<u8>,
        sequence: u64,
        stream_time: u64,
    ) -> Result<(), xsalsa20poly1305::Error>;
    fn encrypt(
        &self,
        voice: &mut Vec<u8>,
        sequence: u64,
        stream_time: u64,
    ) -> Result<(), xsalsa20poly1305::Error>;
}

impl VoiceCrypto for XSalsa20Poly1305 {
    fn decrypt(
        &self,
        voice: &mut Vec<u8>,
        sequence: u64,
        stream_time: u64,
    ) -> Result<(), xsalsa20poly1305::Error> {
        let mut nonce = Nonce::default();
        nonce[..8].copy_from_slice(&sequence.to_be_bytes());
        nonce[8..16].copy_from_slice(&stream_time.to_be_bytes());

        self.decrypt_in_place(&nonce, &[], voice)
    }

    fn encrypt(
        &self,
        voice: &mut Vec<u8>,
        sequence: u64,
        stream_time: u64,
    ) -> Result<(), xsalsa20poly1305::Error> {
        let mut nonce = Nonce::default();
        nonce[..8].copy_from_slice(&sequence.to_be_bytes());
        nonce[8..16].copy_from_slice(&stream_time.to_be_bytes());

        self.encrypt_in_place(&nonce, &[], voice)
    }
}
