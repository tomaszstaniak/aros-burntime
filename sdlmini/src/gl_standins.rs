//! Stand-ins for the six gl.library functions the ABIv11 SDK's static SDL2
//! names (`SDL_arosopengl.o`). Linking libGL for them would bring its
//! auto-open, and gl.library would be opened when the program starts
//! (`build.rs`). This program asks SDL for the software renderer, so SDL
//! never reaches these; if it did, OpenGL would fail cleanly: no context, no
//! functions.

use core::ffi::c_void;

#[no_mangle]
pub extern "C" fn glACreateContext(_tags: *mut c_void) -> *mut c_void {
    core::ptr::null_mut()
}

#[no_mangle]
pub extern "C" fn glADestroyContext(_ctx: *mut c_void) {}

#[no_mangle]
pub extern "C" fn glAMakeCurrent(_ctx: *mut c_void) {}

#[no_mangle]
pub extern "C" fn glASwapBuffers(_ctx: *mut c_void) {}

#[no_mangle]
pub extern "C" fn glASetRast(_ctx: *mut c_void, _tags: *mut c_void) {}

#[no_mangle]
pub extern "C" fn glAGetProcAddress(_name: *const u8) -> *mut c_void {
    core::ptr::null_mut()
}
