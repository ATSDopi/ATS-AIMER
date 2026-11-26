//! Événements d'entrée + journal binaire `.atsi`.
//!
//! Format `input.atsi` :
//! ```text
//! header 32B : "ATSI" | u16 version=1 | u16 hdr_size=32 | u64 qpc_freq
//!              | u64 qpc_start | u64 unix_start_ms
//! record 20B : u64 qpc | u16 kind | u16 flags | i32 a | i32 b | i32 c
//! ```

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

pub const EV_MOUSE_MOVE: u16 = 1;
pub const EV_BUTTON: u16 = 2;
pub const EV_KEY: u16 = 3;
pub const EV_WHEEL: u16 = 4;

/// Un événement d'entrée horodaté (QueryPerformanceCounter).
#[derive(Clone, Copy)]
pub struct Ev {
    pub qpc: u64,
    pub kind: u16,
    /// MouseMove: RAWMOUSE.usFlags. Button/Key: 1=down, 0=up.
    pub flags: u16,
    pub a: i32,
    pub b: i32,
    pub c: i32,
}

/// Canal pompe Win32 → contrôleur.
pub enum Msg {
    Ev(Ev),
    /// F8 / demande de bascule enregistrement.
    Toggle,
    /// Changement de fenêtre au premier plan (titre).
    Focus(String),
    Quit,
}

/// Écrivain binaire `.atsi`.
pub struct LogWriter {
    w: BufWriter<File>,
    pub events: u64,
}

impl LogWriter {
    pub fn create(path: &Path, qpc_freq: u64, qpc_start: u64, unix_start_ms: u64) -> io::Result<Self> {
        let mut w = BufWriter::new(File::create(path)?);
        w.write_all(b"ATSI")?;
        w.write_all(&1u16.to_le_bytes())?; // version
        w.write_all(&32u16.to_le_bytes())?; // header size
        w.write_all(&qpc_freq.to_le_bytes())?;
        w.write_all(&qpc_start.to_le_bytes())?;
        w.write_all(&unix_start_ms.to_le_bytes())?;
        Ok(Self { w, events: 0 })
    }

    #[inline]
    pub fn write(&mut self, e: &Ev) -> io::Result<()> {
        self.w.write_all(&e.qpc.to_le_bytes())?;
        self.w.write_all(&e.kind.to_le_bytes())?;
        self.w.write_all(&e.flags.to_le_bytes())?;
        self.w.write_all(&e.a.to_le_bytes())?;
        self.w.write_all(&e.b.to_le_bytes())?;
        self.w.write_all(&e.c.to_le_bytes())?;
        self.events += 1;
        Ok(())
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.w.flush()
    }
}
