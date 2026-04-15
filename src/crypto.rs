use aes_gcm::{
    aead::Aead, Aes256Gcm, Key, Nonce, KeyInit
};
use sha2::{Sha256, Digest};

pub fn hash256(text: &String) -> [u8; 32] {
    let res = Sha256::digest(text.as_bytes());
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&res);
    bytes
}

pub fn encrypt(key: &[u8], plaintext: &[u8], nonce: &[u8]) -> Result<Vec<u8>, String> {
    let key = Key::<Aes256Gcm>::from_slice(key);
    let nonce = Nonce::from_slice(nonce); // nonce must be a 12 byte shit
    let cipher = Aes256Gcm::new(key);

    let ciphertext = match cipher.encrypt(nonce, plaintext) {
        Ok(x) => x,
        Err(x) => {
            return Err(format!("[!] Error encrypting message: {}", x));
        },
    };

    return Ok(ciphertext);
}

pub fn decrypt(key: &[u8], ciphertext: &[u8], nonce: &[u8]) -> Result<Vec<u8>, String> {
    let key = Key::<Aes256Gcm>::from_slice(key);
    let nonce = Nonce::from_slice(nonce); // nonce must be a 12 byte shit
    let cipher = Aes256Gcm::new(key);

    let plaintext = match cipher.decrypt(nonce, ciphertext.as_ref()) {
        Ok(x) => x,
        Err(x) => {
            return Err(format!("[!] Error decrypting message: {}", x));
        },
    };

    return Ok(plaintext);
}
