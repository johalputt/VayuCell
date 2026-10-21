// SPDX-License-Identifier: Apache-2.0

//! The free space on a filesystem, from the platform's own libc.
//!
//! # Why this file exists, and why it is the only one like it
//!
//! The catalogue wants to know how much room the device has. The kernel does
//! not write that number down anywhere the project can read it: `/proc` has
//! the memory, `/sys` has the charge nodes, and the free space on a
//! filesystem is only available through `statvfs(2)`.
//!
//! Calling into libc is `unsafe` in Rust, and this repository forbids unsafe
//! code twice over in the core crate. So this file is the **one** place in
//! the project where an unsafe block is allowed, and the gate in
//! `.github/workflows/ci.yml` fails the build if a second one appears
//! anywhere, or if this one moves.
//!
//! The alternative was rejected: the kernel does publish the free space, in
//! `/proc/self/mountstats` — and only for filesystems that implement
//! `show_stats`. `f2fs`, which is what a great many Android phones put their
//! data partition on, does not, so the measurement would have read *cannot be
//! told* on the very devices this project is for.
//!
//! # What the numbers mean
//!
//! `f_bavail` is the blocks available to an unprivileged user, and `f_frsize`
//! is the unit they are counted in. The product is the free space the process
//! may actually use, which is smaller than the space the filesystem has left
//! and is the number a vault asking for room should be told.

use std::path::Path;

/// The free space on the filesystem holding `path`, in megabytes.
///
/// `None` when the platform will not say: the path does not exist, the
/// filesystem refused, or this is not a platform that has the call. An
/// unanswered question is never reported as an answer — in particular, never
/// as zero, which is a measurement of a full disk and not of an unreadable
/// one.
///
/// # Safety
///
/// None of this is unsafe to *call*: the only unsafe thing in the project is
/// inside this function, and it is the call into libc.
#[cfg(all(unix, not(miri)))]
#[allow(unsafe_code)]
pub fn free_space_mb(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::mem::MaybeUninit;

    /// `unsigned long` in the C library this binary was linked against.
    #[cfg(target_pointer_width = "64")]
    type Word = u64;
    #[cfg(target_pointer_width = "32")]
    type Word = u32;

    /// `fsid_t`: two `int`s on glibc, and `unsigned long` on musl and
    /// bionic.
    ///
    /// The two are the same size on a 64-bit target and differ on a 32-bit
    /// one. Every field this module reads sits *before* the id, so the
    /// disagreement never reaches them; the struct is declared with glibc's
    /// layout because it is the largest of the three, and the C library must
    /// never be asked to write past the end of the buffer it was handed.
    #[repr(C)]
    struct Fsid {
        val: [i32; 2],
    }

    /// The C library's `struct statvfs`.
    ///
    /// The first five fields are the ones that matter and they sit at the
    /// same offsets in glibc, musl and bionic, because all three declare
    /// them first and in this order.
    #[repr(C)]
    struct StatVfs {
        f_bsize: Word,
        f_frsize: Word,
        f_blocks: Word,
        f_bfree: Word,
        f_bavail: Word,
        f_files: Word,
        f_ffree: Word,
        f_favail: Word,
        f_fsid: Fsid,
        f_flag: Word,
        f_namemax: Word,
        f_spare: [i32; 6],
    }

    extern "C" {
        fn statvfs(path: *const std::ffi::c_char, buf: *mut StatVfs) -> std::ffi::c_int;
    }

    // A path with a NUL in it cannot be passed to C at all, and the failure
    // is a path the kernel would not have accepted either.
    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;

    // The one unsafe block in the project. It is a call into the C library,
    // with a path the C library is given a pointer to and a buffer it is
    // given the address of; the buffer is only read after the call says it
    // was filled, and the path outlives the call.
    let (code, buffer) = unsafe {
        let mut buffer = MaybeUninit::<StatVfs>::uninit();
        let code = statvfs(c_path.as_ptr(), buffer.as_mut_ptr());
        // A failed call leaves the buffer as it was — uninitialised — so it
        // is not read at all in that case.
        if code == 0 {
            (code, Some(buffer.assume_init()))
        } else {
            (code, None)
        }
    };
    if code != 0 {
        return None;
    }
    let buffer = buffer?;

    // A fragment size of zero would make the multiplication below an answer
    // of zero, and zero free space is a claim about a full disk.
    let unit = u64::from(buffer.f_frsize);
    if unit == 0 {
        return None;
    }
    let bytes = u64::from(buffer.f_bavail).checked_mul(unit)?;
    Some(bytes / (1024 * 1024))
}

/// The free space on the filesystem holding `path`, in megabytes.
///
/// `None` on the platforms that cannot answer, and `None` under Miri, which
/// cannot call into libc at all. The honest answer on both is the one this
/// project uses for a question nobody answered: nobody has measured it.
#[cfg(not(all(unix, not(miri))))]
pub fn free_space_mb(_path: &Path) -> Option<u64> {
    // The product is a Unix product: the installer refuses to run anywhere
    // else. A development host that is not Unix is not a device this can
    // measure, and it says so rather than guessing.
    None
}

#[cfg(all(test, unix, not(miri)))]
mod tests {
    use super::free_space_mb;
    use std::path::Path;

    #[test]
    fn a_filesystem_that_is_there_answers_with_its_free_space() {
        // The measurement itself, against the real kernel: a root filesystem
        // exists, and it has room or it does not, but it always has an
        // answer. A None here would mean the call is wrong on this platform,
        // which is exactly what a test on a real filesystem is for.
        let free = free_space_mb(Path::new("/"));
        assert!(
            free.is_some(),
            "the root filesystem exists and the kernel will say how much of it is free"
        );
    }

    #[test]
    fn a_path_that_is_not_there_has_no_free_space_to_report() {
        // The defect this forecloses: a missing path reported as zero free
        // bytes, which a catalogue would read as a full disk and a service
        // would be refused for want of room that nobody ever measured.
        assert_eq!(
            free_space_mb(Path::new("/definitely/not/a/real/path")),
            None
        );
    }
}
