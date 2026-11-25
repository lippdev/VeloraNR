use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::{
    ffi::{CString, c_char, c_void},
    time::{Duration, Instant},
};
unsafe extern "C" {
    fn portal_pair_start(host: *const c_char, account: *const u8, pin: u32) -> *mut c_void;
    fn portal_pair_status(registration: *mut c_void) -> i32;
    fn portal_pair_finish(registration: *mut c_void, profile: *const u16) -> bool;
}
fn account_id(value: &str) -> Option<[u8; 8]> {
    let value = value.trim();
    if value.bytes().all(|b| b.is_ascii_digit()) {
        return value.parse::<u64>().ok().map(u64::to_le_bytes);
    }
    STANDARD.decode(value).ok()?.try_into().ok()
}
pub fn register(file: &[u16], host: &str, account: &str, pin: &str) -> i32 {
    let (Some(account), Ok(host)) = (account_id(account), CString::new(host.trim())) else {
        return -1;
    };
    if pin.len() != 8 || !pin.bytes().all(|b| b.is_ascii_digit()) {
        return -1;
    }
    let registration =
        unsafe { portal_pair_start(host.as_ptr(), account.as_ptr(), pin.parse().unwrap()) };
    if registration.is_null() {
        return -2;
    }
    let started = Instant::now();
    let status = loop {
        let state = unsafe { portal_pair_status(registration) };
        if state != 0 {
            break state;
        }
        if started.elapsed() >= Duration::from_secs(30) {
            break -3;
        }
        std::thread::sleep(Duration::from_millis(40));
    };
    let saved = unsafe {
        portal_pair_finish(
            registration,
            if status == 1 {
                file.as_ptr()
            } else {
                std::ptr::null()
            },
        )
    };
    if saved {
        0
    } else if status == 1 {
        -4
    } else {
        status
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn account_formats_match() {
        assert_eq!(account_id("1"), account_id("AQAAAAAAAAA="));
        assert_eq!(account_id("x"), None);
    }
}
