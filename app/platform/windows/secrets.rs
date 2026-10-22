//! User-scoped DPAPI. Failure never falls back to plaintext storage.

use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretError {
    InvalidInput,
    ProtectionUnavailable,
}

pub struct SecretBytes(Vec<u8>);

impl SecretBytes {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl Drop for SecretBytes {
    fn drop(&mut self) {
        for byte in &mut self.0 {
            // Keep clearing visible to the compiler before the allocation is freed.
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
    }
}

pub fn protect(plaintext: &[u8]) -> Result<Vec<u8>, SecretError> {
    crypt(plaintext, true)
}

pub fn unprotect(ciphertext: &[u8]) -> Result<SecretBytes, SecretError> {
    crypt(ciphertext, false).map(SecretBytes)
}

fn crypt(input: &[u8], encrypt: bool) -> Result<Vec<u8>, SecretError> {
    let length = u32::try_from(input.len()).map_err(|_| SecretError::InvalidInput)?;
    if length == 0 {
        return Err(SecretError::InvalidInput);
    }
    let source = CRYPT_INTEGER_BLOB {
        cbData: length,
        pbData: input.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let success = if encrypt {
        unsafe {
            CryptProtectData(
                &source,
                null(),
                null(),
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    } else {
        unsafe {
            CryptUnprotectData(
                &source,
                null_mut(),
                null(),
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if success == 0 || output.pbData.is_null() {
        return Err(SecretError::ProtectionUnavailable);
    }
    let bytes = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) };
    let copied = bytes.to_vec();
    if !encrypt {
        for index in 0..output.cbData as usize {
            unsafe { std::ptr::write_volatile(output.pbData.add(index), 0) };
        }
    }
    unsafe { LocalFree(output.pbData.cast()) };
    Ok(copied)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_scoped_dpapi_round_trip_and_invalid_blob() {
        let plaintext = b"island-test-secret";
        let encrypted = protect(plaintext).expect("current Windows user can protect");
        assert_ne!(encrypted, plaintext);
        let recovered = unprotect(&encrypted).expect("current Windows user can unprotect");
        assert_eq!(recovered.as_bytes(), plaintext);
        assert!(matches!(
            unprotect(b"not-dpapi"),
            Err(SecretError::ProtectionUnavailable)
        ));
    }
}
