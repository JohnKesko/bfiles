//! Directory scanning abstraction.
//!
//! On macOS we use `getattrlistbulk(2)`, which returns each entry's name,
//! object type, and data-fork length in bulk — typically a handful of syscalls
//! per directory instead of `readdir` plus one `stat` per file. On every other
//! platform (and as a fallback if the bulk call is unsupported) we use the
//! portable `std::fs` path.

use std::ffi::OsString;
use std::io;
use std::path::Path;

/// A single directory entry with just the fields the traversal needs.
pub struct Entry {
    pub name: OsString,
    pub is_dir: bool,
    pub is_symlink: bool,
    /// Logical size of the file's data (matches `Metadata::len`). 0 for non-files.
    pub size: u64,
}

/// Escape hatch: set `BFILES_NO_BULK=1` to force the portable `std::fs` path
/// (for A/B comparison, or if the macOS bulk path ever misbehaves on a volume).
#[cfg(target_os = "macos")]
fn bulk_disabled() -> bool {
    use std::sync::OnceLock;
    static DISABLED: OnceLock<bool> = OnceLock::new();
    *DISABLED.get_or_init(|| std::env::var_os("BFILES_NO_BULK").is_some())
}

#[cfg(target_os = "macos")]
pub fn read_dir_entries(path: &Path) -> io::Result<Vec<Entry>> {
    if bulk_disabled() {
        return portable::read_dir_entries(path);
    }
    // Fall back to the portable scan if the bulk call isn't supported by the
    // underlying filesystem (e.g. some network mounts) or hits a parse anomaly.
    match macos::read_dir_bulk(path) {
        Ok(entries) => Ok(entries),
        Err(_) => portable::read_dir_entries(path),
    }
}

#[cfg(not(target_os = "macos"))]
pub fn read_dir_entries(path: &Path) -> io::Result<Vec<Entry>> {
    portable::read_dir_entries(path)
}

mod portable {
    use super::Entry;
    use std::fs;
    use std::io;
    use std::path::Path;

    pub fn read_dir_entries(path: &Path) -> io::Result<Vec<Entry>> {
        let rd = fs::read_dir(path)?;
        let mut out = Vec::new();

        for entry in rd.filter_map(Result::ok) {
            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };

            let size = if file_type.is_file() {
                entry.metadata().map(|m| m.len()).unwrap_or(0)
            } else {
                0
            };

            out.push(Entry {
                name: entry.file_name(),
                is_dir: file_type.is_dir(),
                is_symlink: file_type.is_symlink(),
                size,
            });
        }

        Ok(out)
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::Entry;
    use std::cell::RefCell;
    use std::ffi::{CString, OsString};
    use std::io;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::path::Path;

    use libc::{c_int, c_void, size_t};

    // attrlist bitmaps (sys/attr.h)
    const ATTR_BIT_MAP_COUNT: u16 = 5;
    const ATTR_CMN_RETURNED_ATTRS: u32 = 0x8000_0000;
    const ATTR_CMN_NAME: u32 = 0x0000_0001;
    const ATTR_CMN_OBJTYPE: u32 = 0x0000_0008;
    const ATTR_FILE_DATALENGTH: u32 = 0x0000_0200;

    // Pack a zeroed slot for attributes that can't be returned, so every entry
    // has a fixed field layout regardless of object type.
    const FSOPT_PACK_INVAL_ATTRS: u64 = 0x0000_0008;

    // fsobj_type_t values (sys/vnode.h)
    const VREG: u32 = 1;
    const VDIR: u32 = 2;
    const VLNK: u32 = 5;

    // 256 KiB lets us pull many entries per syscall, it is reused per worker thread.
    const BUF_SIZE: usize = 256 * 1024;

    thread_local! {
        static BUF: RefCell<Vec<u8>> = RefCell::new(vec![0u8; BUF_SIZE]);
    }

    #[repr(C)]
    struct Attrlist {
        bitmapcount: u16,
        reserved: u16,
        commonattr: u32,
        volattr: u32,
        dirattr: u32,
        fileattr: u32,
        forkattr: u32,
    }

    unsafe extern "C" {
        fn getattrlistbulk(
            dirfd: c_int,
            attr_list: *mut Attrlist,
            attr_buf: *mut c_void,
            attr_buf_size: size_t,
            options: u64,
        ) -> c_int;
    }

    struct FdGuard(c_int);

    impl Drop for FdGuard {
        fn drop(&mut self) {
            unsafe { libc::close(self.0) };
        }
    }

    pub fn read_dir_bulk(path: &Path) -> io::Result<Vec<Entry>> {
        let cpath = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;

        // Match std's opendir() flags: O_DIRECTORY fails fast on non-directories.
        let fd = unsafe { libc::open(cpath.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let _guard = FdGuard(fd);

        let mut attrlist = Attrlist {
            bitmapcount: ATTR_BIT_MAP_COUNT,
            reserved: 0,
            commonattr: ATTR_CMN_RETURNED_ATTRS | ATTR_CMN_NAME | ATTR_CMN_OBJTYPE,
            volattr: 0,
            dirattr: 0,
            fileattr: ATTR_FILE_DATALENGTH,
            forkattr: 0,
        };

        BUF.with(|buf| {
            let buf = &mut *buf.borrow_mut();
            let mut entries = Vec::new();

            // Backstop against a filesystem whose fd offset never advances:
            // bail to Err (→ portable fallback) instead of looping forever.
            let mut calls = 0u32;

            loop {
                calls += 1;
                if calls > 1_000_000 {
                    return Err(io::Error::from(io::ErrorKind::Other));
                }

                let retcount = unsafe {
                    getattrlistbulk(
                        fd,
                        &mut attrlist,
                        buf.as_mut_ptr() as *mut c_void,
                        buf.len() as size_t,
                        FSOPT_PACK_INVAL_ATTRS,
                    )
                };

                if retcount < 0 {
                    return Err(io::Error::last_os_error());
                }
                if retcount == 0 {
                    break;
                }

                let mut offset = 0usize;
                for _ in 0..retcount {
                    offset = parse_entry(buf, offset, &mut entries)?;
                }
            }

            Ok(entries)
        })
    }

    /// Parse one entry starting at `start`, push it and return the offset of
    /// the next entry. Returns `Err` (never panics) on any malformed layout so
    /// the caller can fall back to the portable scan.
    fn parse_entry(buf: &[u8], start: usize, out: &mut Vec<Entry>) -> io::Result<usize> {
        // Layout (canonical order, fixed thanks to FSOPT_PACK_INVAL_ATTRS):
        //   u32              length (whole entry, including this field)
        //   attribute_set_t  returned attrs (5 x u32 = 20 bytes)
        //   attrreference_t  name (i32 dataoffset, u32 length = 8 bytes)
        //   u32              objtype
        //   u64 (off_t)      data length
        let length = read_u32(buf, start)? as usize;
        if length == 0 {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        let next = start.checked_add(length).ok_or(io::ErrorKind::InvalidData)?;

        let name_field = start + 4 + 20; // skip length + returned attribute_set_t
        let name_dataoffset = read_i32(buf, name_field)? as isize;
        let name_len = read_u32(buf, name_field + 4)? as usize;

        let objtype = read_u32(buf, name_field + 8)?;
        let datalength = read_u64(buf, name_field + 12)?;

        // Name data is located relative to the start of its attrreference field.
        let name_start = (name_field as isize)
            .checked_add(name_dataoffset)
            .and_then(|v| usize::try_from(v).ok())
            .ok_or(io::ErrorKind::InvalidData)?;
        let raw = buf
            .get(name_start..name_start.checked_add(name_len).ok_or(io::ErrorKind::InvalidData)?)
            .ok_or(io::ErrorKind::InvalidData)?;
        // attr_length includes the trailing NUL (and possibly padding); cut at it.
        let name_bytes = match raw.iter().position(|&b| b == 0) {
            Some(p) => &raw[..p],
            None => raw,
        };

        out.push(Entry {
            name: OsString::from_vec(name_bytes.to_vec()),
            is_dir: objtype == VDIR,
            is_symlink: objtype == VLNK,
            size: if objtype == VREG { datalength } else { 0 },
        });

        Ok(next)
    }

    fn read_u32(buf: &[u8], off: usize) -> io::Result<u32> {
        let s = buf.get(off..off + 4).ok_or(io::ErrorKind::InvalidData)?;
        Ok(u32::from_ne_bytes(s.try_into().unwrap()))
    }

    fn read_i32(buf: &[u8], off: usize) -> io::Result<i32> {
        let s = buf.get(off..off + 4).ok_or(io::ErrorKind::InvalidData)?;
        Ok(i32::from_ne_bytes(s.try_into().unwrap()))
    }

    fn read_u64(buf: &[u8], off: usize) -> io::Result<u64> {
        let s = buf.get(off..off + 8).ok_or(io::ErrorKind::InvalidData)?;
        Ok(u64::from_ne_bytes(s.try_into().unwrap()))
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::Path;

    /// (name -> (is_dir, is_symlink, size)) for easy comparison.
    fn index(entries: Vec<super::Entry>) -> BTreeMap<String, (bool, bool, u64)> {
        entries
            .into_iter()
            .map(|e| (e.name.to_string_lossy().into_owned(), (e.is_dir, e.is_symlink, e.size)))
            .collect()
    }

    #[test]
    fn bulk_matches_portable() {
        // Build a directory with a file, a subdir, and a symlink.
        let dir = std::env::temp_dir().join(format!("bfiles_scan_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("hello.txt"), b"hello world").unwrap(); // 11 bytes
        fs::create_dir(dir.join("child")).unwrap();
        std::os::unix::fs::symlink(dir.join("hello.txt"), dir.join("link")).unwrap();

        let bulk = index(super::macos::read_dir_bulk(&dir).unwrap());
        let portable = index(super::portable::read_dir_entries(&dir).unwrap());

        assert_eq!(bulk, portable, "getattrlistbulk path must match std::fs");
        assert_eq!(bulk["hello.txt"], (false, false, 11));
        assert_eq!(bulk["child"].0, true);
        assert_eq!(bulk["link"].1, true);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn bulk_matches_portable_on_src_tree() {
        // A real, non-trivial directory exercises many entries per syscall.
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let bulk = index(super::macos::read_dir_bulk(&src).unwrap());
        let portable = index(super::portable::read_dir_entries(&src).unwrap());
        assert_eq!(bulk, portable);
    }
}
