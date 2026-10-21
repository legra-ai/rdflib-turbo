//! Audited CPython scalar-table boundary, for CPython-specific wheels (not abi3).
//!
//! CPython 3.11 through 3.15 declares these Py_UCS4 -> int exports in
//! Include/cpython/unicodeobject.h. Objects/unicodectype.c and the generated
//! unicodetype_db.h implement them as immutable table lookups / switches.
//! They access no PyObject, refcount, interpreter state, error indicator or GIL.
//! Source: https://github.com/python/cpython/blob/3.14/Objects/unicodectype.c

use std::ffi::c_int;

unsafe extern "C" {
    fn _PyUnicode_IsAlpha(ch: u32) -> c_int;
    fn _PyUnicode_IsNumeric(ch: u32) -> c_int;
    fn _PyUnicode_IsWhitespace(ch: u32) -> c_int;
}

pub(super) fn decimal(ch: char) -> Option<u8> {
    // SAFETY: char is a valid Unicode scalar; this pure const-table lookup
    // accepts Py_UCS4 by value and borrows no Python object or thread state.
    let digit = unsafe { pyo3::ffi::Py_UNICODE_TODECIMAL(ch as u32) };
    u8::try_from(digit).ok()
}

pub(super) fn word(ch: char) -> bool {
    // SAFETY: same scalar-only, immutable-table contract as decimal().
    ch == '_'
        || unsafe { _PyUnicode_IsAlpha(ch as u32) != 0 || _PyUnicode_IsNumeric(ch as u32) != 0 }
}

pub(super) fn whitespace(ch: char) -> bool {
    // SAFETY: same scalar-only contract; this export is a pure generated switch.
    unsafe { _PyUnicode_IsWhitespace(ch as u32) != 0 }
}
