//! What a program of the phone's built against a newer Android asks its
//! libraries for, and the recovery's own copies of those libraries do not
//! carry: loaded before them, so that the program links and starts.
//!
//! The phone's hardware programs are built against the system it ships
//! with. A recovery carries libraries of its own build of Android, which
//! is older, and a program that asks for an entry point added since stops
//! before its first line: the dynamic linker finds nothing under the name
//! and ends it. What is here is the smallest that closes that gap — an
//! entry point that only ever names transaction codes for tracing, whose
//! absence is worth nothing at all, and whose presence is the difference
//! between a service that registers and one that dies a second at a time.
//!
//! Nothing here is particular to a phone: a program that does not ask for
//! what is here is not changed by it, and one that does is started where it
//! would otherwise not be.

#![no_std]
#![allow(non_snake_case)]

use core::ffi::c_void;

/// Nothing here allocates, formats or unwinds: the entry points below hand
/// what they are given back, and a panic in them cannot happen.
#[panic_handler]
fn panicked(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

/// Give the transaction codes of a binder class the names they are logged
/// under. The recovery has no use for the names, and the codes work the
/// same without them: this says it is done and keeps nothing.
///
/// The signature is the one `libbinder_ndk.so` has from Android 14 on:
///
/// ```c
/// bool AIBinder_Class_setTransactionCodeToFunctionNameMap(
///     AIBinder_Class* clazz, const AIBinder_TransactionCode* codes, size_t count);
/// ```
///
/// # Safety
///
/// Called by the program that loads this, with the arguments its own build
/// of the library gave it. Nothing here reads any of them.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn AIBinder_Class_setTransactionCodeToFunctionNameMap(
    _class: *mut c_void,
    _codes: *const c_void,
    _count: usize,
) -> bool {
    true
}
