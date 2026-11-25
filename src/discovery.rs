use std::ffi::{CStr, c_char, c_void};

unsafe extern "C" {
    fn portal_discover(
        callback: extern "C" fn(*mut c_void, *const c_char, bool),
        context: *mut c_void,
    ) -> i32;
}

pub struct Console {
    pub address: String,
    pub standby: bool,
}

pub enum State {
    Searching,
    Found(Vec<Console>),
    Empty,
    Failed(i32),
}

extern "C" fn receive(context: *mut c_void, address: *const c_char, standby: bool) {
    // The native search invokes this on the caller thread before returning.
    let consoles = unsafe { &mut *context.cast::<Vec<Console>>() };
    let Ok(address) = unsafe { CStr::from_ptr(address) }.to_str() else {
        return;
    };
    if !consoles.iter().any(|console| console.address == address) {
        consoles.push(Console {
            address: address.to_owned(),
            standby,
        });
    }
}

pub fn scan() -> State {
    let mut consoles: Vec<Console> = Vec::new();
    let code = unsafe { portal_discover(receive, (&mut consoles as *mut Vec<Console>).cast()) };
    if !consoles.is_empty() {
        consoles.sort_by(|a, b| (a.standby, &a.address).cmp(&(b.standby, &b.address)));
        State::Found(consoles)
    } else if code == 0 {
        State::Empty
    } else {
        State::Failed(code)
    }
}
