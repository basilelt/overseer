#[cfg(unix)]
pub use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
#[cfg(unix)]
pub use std::os::unix::process::CommandExt;

#[cfg(not(unix))]
pub use shim::*;

#[cfg(not(unix))]
mod shim {
    pub trait OpenOptionsExt {
        fn mode(&mut self, _: u32) -> &mut Self;
    }

    impl OpenOptionsExt for std::fs::OpenOptions {
        fn mode(&mut self, _: u32) -> &mut Self {
            self
        }
    }

    pub trait DirBuilderExt {
        fn mode(&mut self, _: u32) -> &mut Self;
    }

    impl DirBuilderExt for std::fs::DirBuilder {
        fn mode(&mut self, _: u32) -> &mut Self {
            self
        }
    }

    pub trait PermissionsExt {
        fn mode(&self) -> u32;
    }

    impl PermissionsExt for std::fs::Permissions {
        fn mode(&self) -> u32 {
            if self.readonly() {
                0o500
            } else {
                0o700
            }
        }
    }

    pub trait CommandExt {
        fn exec(&mut self) -> std::io::Error;
    }

    impl CommandExt for std::process::Command {
        fn exec(&mut self) -> std::io::Error {
            match self.status() {
                Ok(s) => std::process::exit(s.code().unwrap_or(1)),
                Err(e) => e,
            }
        }
    }
}

pub fn random(buf: &mut [u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Read;
        std::fs::File::open("/dev/urandom")?.read_exact(buf)
    }
    #[cfg(not(unix))]
    {
        use std::hash::{BuildHasher, Hasher};
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        for c in buf.chunks_mut(8) {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u128(nanos);
            c.copy_from_slice(&h.finish().to_le_bytes()[..c.len()]);
        }
        Ok(())
    }
}
