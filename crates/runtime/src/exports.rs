//! The WebAssembly face of the runtime. Text goes in through one buffer
//! and comes out through another; the frame block is read in place.

use crate::Session;
use std::sync::Mutex;

struct State {
    session: Option<Session>,
    input: Vec<u8>,
    output: Vec<u8>,
    block_at: usize,
}

static STATE: Mutex<State> = Mutex::new(State {
    session: None,
    input: Vec::new(),
    output: Vec::new(),
    block_at: 0,
});

fn with<T>(run: impl FnOnce(&mut State) -> T) -> T {
    let mut state = STATE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    run(&mut state)
}

fn answer(state: &mut State, run: impl FnOnce(&mut Session, &str) -> String) -> usize {
    let text = String::from_utf8_lossy(&state.input).into_owned();
    let session = state.session.get_or_insert_with(Session::new);
    state.output = run(session, &text).into_bytes();
    state.output.len()
}

/// Makes room for `len` bytes of input and returns where to write them.
#[no_mangle]
pub extern "C" fn fs_input(len: usize) -> *mut u8 {
    with(|state| {
        state.input.clear();
        state.input.resize(len, 0);
        state.input.as_mut_ptr()
    })
}

/// Where the last answer is.
#[no_mangle]
pub extern "C" fn fs_output() -> *const u8 {
    with(|state| state.output.as_ptr())
}

/// Reads the snapshot in the input. Returns the length of the answer.
#[no_mangle]
pub extern "C" fn fs_read() -> usize {
    with(|state| answer(state, |session, text| session.read(text)))
}

/// Resolves with the matches in the input. Returns the length of the answer.
#[no_mangle]
pub extern "C" fn fs_resolve() -> usize {
    with(|state| answer(state, |session, text| session.resolve(text)))
}

/// Fixes the frame rate. Returns the length of the answer.
#[no_mangle]
pub extern "C" fn fs_begin(fps: u32) -> usize {
    with(|state| answer(state, |session, _| session.begin(fps)))
}

/// Computes a frame. Returns how many numbers the block holds.
#[no_mangle]
pub extern "C" fn fs_frame(frame: u32) -> usize {
    with(|state| {
        let session = state.session.get_or_insert_with(Session::new);
        let block = session.frame(frame);
        state.block_at = block.as_ptr() as usize;
        block.len()
    })
}

/// Where the block of the last frame is.
#[no_mangle]
pub extern "C" fn fs_block() -> usize {
    with(|state| state.block_at)
}
