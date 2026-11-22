//! Secrets at rest: the current Windows user's DPAPI key, bound to this
//! application by a fixed entropy string. A blob only opens for the same user on
//! the same machine, so a copied settings file carries no usable secret.
//! Stored as lowercase hex; nothing here logs or returns a plaintext value.

const ENTROPY: &[u8] = b"shinatama.launch-settings.v1";

pub fn protect(plain: &str) -> Result<String, &'static str> {
    let blob = imp::protect(plain.as_bytes())?;
    Ok(hex(&blob))
}
pub fn unprotect(stored: &str) -> Result<String, &'static str> {
    let blob = unhex(stored).ok_or("The saved secret is damaged.")?;
    let plain = imp::unprotect(&blob)?;
    String::from_utf8(plain).map_err(|_| "The saved secret is damaged.")
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 15) as usize] as char);
    }
    out
}
fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || text.len() > 64 * 1024 {
        return None;
    }
    let nibble = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    };
    text.as_bytes()
        .chunks(2)
        .map(|pair| Some(nibble(pair[0])? << 4 | nibble(pair[1])?))
        .collect()
}

#[cfg(windows)]
mod imp {
    use super::ENTROPY;
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
        },
    };

    fn blob(bytes: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB {
            cbData: bytes.len() as u32,
            pbData: bytes.as_ptr() as *mut u8,
        }
    }
    /// Copies the system-allocated output and frees it.
    unsafe fn take(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        let copy = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
        unsafe { LocalFree(out.pbData.cast()) };
        copy
    }
    pub fn protect(plain: &[u8]) -> Result<Vec<u8>, &'static str> {
        let input = blob(plain);
        let entropy = blob(ENTROPY);
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        // SAFETY: both input blobs outlive the call; `out` is filled by the system.
        let ok = unsafe {
            CryptProtectData(
                &input,
                std::ptr::null(),
                &entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok == 0 || out.pbData.is_null() {
            return Err("Windows could not protect the secret.");
        }
        Ok(unsafe { take(out) })
    }
    pub fn unprotect(stored: &[u8]) -> Result<Vec<u8>, &'static str> {
        let input = blob(stored);
        let entropy = blob(ENTROPY);
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        // SAFETY: as in `protect`; the description output is not requested.
        let ok = unsafe {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                &entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok == 0 || out.pbData.is_null() {
            return Err("The saved secret cannot be opened by this Windows user.");
        }
        Ok(unsafe { take(out) })
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn protect(_: &[u8]) -> Result<Vec<u8>, &'static str> {
        Err("Secrets are only supported on Windows.")
    }
    pub fn unprotect(_: &[u8]) -> Result<Vec<u8>, &'static str> {
        Err("Secrets are only supported on Windows.")
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn a_secret_round_trips_and_the_stored_form_does_not_contain_it() {
        let secret = "ghp_example-token-0123456789";
        let stored = protect(secret).unwrap();
        assert!(!stored.contains(secret));
        assert!(stored.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(unprotect(&stored).unwrap(), secret);
        // Same input, fresh blob each time; both still open.
        assert_eq!(unprotect(&protect(secret).unwrap()).unwrap(), secret);
    }

    #[test]
    fn damaged_or_foreign_input_is_refused_without_a_value() {
        assert!(unprotect("zz").is_err());
        assert!(unprotect("abc").is_err());
        assert!(unprotect("00112233").is_err());
        assert_eq!(unprotect(&protect("").unwrap()).unwrap(), "");
    }
}
