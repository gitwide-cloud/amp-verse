//! Per-user credentials. Windows uses DPAPI; Unix storage is restricted to 0700/0600.
#[cfg(windows)]
mod platform {
    #[repr(C)]
    struct Blob {
        size: u32,
        data: *mut u8,
    }
    #[link(name = "crypt32")]
    unsafe extern "system" {
        fn CryptProtectData(
            input: *const Blob,
            description: *const u16,
            entropy: *const Blob,
            reserved: *mut core::ffi::c_void,
            prompt: *mut core::ffi::c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
        fn CryptUnprotectData(
            input: *const Blob,
            description: *mut *mut u16,
            entropy: *const Blob,
            reserved: *mut core::ffi::c_void,
            prompt: *mut core::ffi::c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(memory: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
    }
    pub fn convert(bytes: &[u8], protect: bool) -> Result<Vec<u8>, String> {
        if bytes.len() > 65536 {
            return Err("Credential exceeds bound".into());
        }
        let input = Blob {
            size: bytes.len() as u32,
            data: bytes.as_ptr().cast_mut(),
        };
        let mut output = Blob {
            size: 0,
            data: core::ptr::null_mut(),
        };
        // DPAPI binds to the current Windows user; UI_FORBIDDEN prevents prompts.
        let ok = unsafe {
            if protect {
                CryptProtectData(
                    &input,
                    core::ptr::null(),
                    core::ptr::null(),
                    core::ptr::null_mut(),
                    core::ptr::null_mut(),
                    1,
                    &mut output,
                )
            } else {
                CryptUnprotectData(
                    &input,
                    core::ptr::null_mut(),
                    core::ptr::null(),
                    core::ptr::null_mut(),
                    core::ptr::null_mut(),
                    1,
                    &mut output,
                )
            }
        };
        if ok == 0 {
            return Err(
                "Windows could not access this account's protected station credential".into(),
            );
        }
        let result = if output.size == 0 {
            Vec::new()
        } else if output.data.is_null() {
            return Err("Invalid protected credential output".into());
        } else {
            unsafe { core::slice::from_raw_parts(output.data, output.size as usize).to_vec() }
        };
        unsafe {
            LocalFree(output.data.cast());
        }
        Ok(result)
    }
}
pub fn seal(bytes: &[u8]) -> Result<Vec<u8>, String> {
    #[cfg(windows)]
    {
        platform::convert(bytes, true)
    }
    #[cfg(not(windows))]
    {
        Ok(bytes.to_vec())
    }
}
pub fn open(bytes: &[u8]) -> Result<Vec<u8>, String> {
    #[cfg(windows)]
    {
        platform::convert(bytes, false)
    }
    #[cfg(not(windows))]
    {
        Ok(bytes.to_vec())
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn credential_roundtrip() {
        let secret = b"fixture-only-credential";
        let protected = super::seal(secret).unwrap();
        assert_eq!(super::open(&protected).unwrap(), secret);
        #[cfg(windows)]
        assert_ne!(protected, secret);
    }
}
