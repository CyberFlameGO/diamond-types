//! Ordinarily I'd just make direct calls to the std::File API, but I'm wrapping the file API here
//! so I can swap out the implementation with something we can test.

use std::fs::File;
use std::path::Path;
use std::io;
#[cfg(not(unix))]
use std::io::{Read, Write};
use std::io::{ErrorKind, Seek, SeekFrom};
use std::os::fd::{AsFd, AsRawFd};
#[cfg(unix)]
use std::os::unix::fs::FileExt;
#[cfg(unix)]
use std::ffi::c_int;

pub trait DTFilesystem {
    type File: DTFile;

    fn open<P: AsRef<Path>>(&mut self, path: P) -> io::Result<Self::File>;
}

pub trait DTFile {
    fn stream_len(&mut self) -> io::Result<u64>;

    fn write_all_at(&mut self, data: &[u8], offset: u64) -> io::Result<()>;
    fn read_all_at(&mut self, buffer: &mut [u8], offset: u64) -> io::Result<()>;

    // fn sync_all(&self) -> io::Result<()>;
    fn write_barrier(&self) -> io::Result<()>;
    fn sync_data(&self) -> io::Result<()>;
}

// *** Os filesystem. This is the real filesystem - and the actual filesystem used will monomorphize to this.

pub struct OsFilesystem;

impl DTFilesystem for OsFilesystem {
    type File = File;

    fn open<P: AsRef<Path>>(&mut self, path: P) -> io::Result<Self::File> {
        File::options()
            .read(true)
            .create(true)
            .write(true)
            .append(false)
            .open(path.as_ref())
    }
}

impl DTFile for File {
    fn stream_len(&mut self) -> io::Result<u64> {
        self.seek(SeekFrom::End(0))
    }

    fn write_all_at(&mut self, data: &[u8], offset: u64) -> io::Result<()> {
        #[cfg(unix)]
        <Self as FileExt>::write_all_at(self, data, offset)?;
        #[cfg(not(unix))] {
            self.seek(std::io::SeekFrom::Start(offset))?;
            self.write_all(data)?;
        }

        Ok(())
    }

    fn read_all_at(&mut self, buffer: &mut [u8], offset: u64) -> io::Result<()> {
        #[cfg(unix)]
        <Self as FileExt>::read_exact_at(self, buffer, offset)?;
        #[cfg(not(unix))] {
            self.seek(SeekFrom::Start(offset))?;
            self.read_exact(buffer)?;
        }

        Ok(())
    }

    fn write_barrier(&self) -> io::Result<()> {
        // I have this as a separate function because fsync is very slow on apple hardware (probably
        // because its not cheating). When we finalize a block with blitted data or write a new
        // file header, we need to enforce specific write ordering to make sure the block is written
        // correctly. But thankfully, apple platforms expose F_BARRIERFSYNC which enforces write
        // ordering without needing to incur the cost of a full fsync.
        //
        // Unfortunately, std doesn't expose a wrapper around F_BARRIERFSYNC. So we need to access
        // it directly through libc.
        #[cfg(any(target_os = "macos", target_os = "ios", target_os = "tvos", target_os = "watchos"))]
        {
            let ret = unsafe {
                libc::fcntl(self.as_raw_fd(), libc::F_BARRIERFSYNC)
            };

            if ret == -1 {
                Err(io::Error::last_os_error())
            } else { Ok(()) }
        }

        // Everywhere else can just do a normal fsync.
        #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "tvos", target_os = "watchos")))]
        File::sync_data(self)
    }

    fn sync_data(&self) -> io::Result<()> {
        File::sync_data(self)
    }
}

// *** Testing filesystem. This is used to make writing tests easier, and enable filesystem error
// injection.

pub struct TestFilesystem;

impl DTFilesystem for TestFilesystem {
    type File = TestFile;

    fn open<P: AsRef<Path>>(&mut self, _path: P) -> io::Result<Self::File> {
        Ok(TestFile::default())
    }
}

#[derive(Debug, Default, Clone)]
pub struct TestFile {
    data: Vec<u8>,
}

impl DTFile for TestFile {
    fn stream_len(&mut self) -> io::Result<u64> {
        Ok(self.data.len() as u64)
    }

    fn write_all_at(&mut self, data: &[u8], offset: u64) -> io::Result<()> {
        let offset = offset as usize;
        let end = offset + data.len();
        if self.data.len() < end {
            self.data.resize(end, 0);
        }
        self.data[offset .. end].copy_from_slice(data);
        Ok(())
    }

    fn read_all_at(&mut self, buffer: &mut [u8], offset: u64) -> io::Result<()> {
        let start = offset as usize;
        let end = start + buffer.len();

        if end > self.data.len() {
            Err(io::Error::from(ErrorKind::UnexpectedEof))
        } else {
            buffer.copy_from_slice(&self.data[start..end]);
            Ok(())
        }
    }

    fn write_barrier(&self) -> io::Result<()> {
        Ok(())
    }

    fn sync_data(&self) -> io::Result<()> {
        Ok(())
    }
}