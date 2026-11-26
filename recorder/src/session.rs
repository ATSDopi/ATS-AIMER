//! Session d'enregistrement : dossier horodaté, input.atsi, video.mp4, meta.json.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Child;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use serde::Serialize;
use time::OffsetDateTime;
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

use crate::event::{Ev, LogWriter, Msg, EV_BUTTON, EV_KEY, EV_MOUSE_MOVE, EV_WHEEL};
use crate::input;
use crate::video::{self, Caps};

#[derive(Clone)]
pub struct RecordArgs {
    pub game: String,
    pub sens: Option<f64>,
    pub dpi: Option<u32>,
    pub out: PathBuf,
    pub ffmpeg: PathBuf,
    pub fps: u32,
    pub input_only: bool,
    pub monitor: u32,
}

#[derive(Serialize, Clone)]
struct FocusEvt {
    t_ms: u64,
    title: String,
}

#[derive(Serialize)]
struct Meta<'a> {
    app: &'a str,
    version: &'a str,
    game: &'a str,
    sens_declared: Option<f64>,
    dpi_declared: Option<u32>,
    fps_capture: u32,
    video_status: &'a str,
    video_file: Option<String>,
    input_file: &'a str,
    started_unix_ms: u64,
    stopped_unix_ms: u64,
    duration_ms: u64,
    qpc_freq: u64,
    qpc_start: u64,
    events_total: u64,
    events_by_kind: [u64; 4],
    monitor: u32,
    screen_w: i32,
    screen_h: i32,
    focus_timeline: Vec<FocusEvt>,
}

pub struct Session {
    dir: PathBuf,
    log: LogWriter,
    ffmpeg: Option<Child>,
    video_failed: bool,
    args: RecordArgs,
    started: Instant,
    started_unix_ms: u64,
    qpc_start: u64,
    kinds: [u64; 4],
    timeline: Vec<FocusEvt>,
}

fn timestamp_name() -> String {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let (h, m, s) = now.time().as_hms();
    format!(
        "{:04}{:02}{:02}_{:02}{:02}{:02}",
        now.year(),
        now.month() as u8,
        now.day(),
        h,
        m,
        s
    )
}

impl Session {
    fn start(args: &RecordArgs, caps: Option<Caps>) -> io::Result<Self> {
        let dir = args.out.join(timestamp_name());
        fs::create_dir_all(&dir)?;
        let qpc_start = input::qpc_now();
        let started_unix_ms = input::unix_ms();
        let log = LogWriter::create(
            &dir.join("input.atsi"),
            input::qpc_freq(),
            qpc_start,
            started_unix_ms,
        )?;
        let mut video_failed = false;
        let mut ffmpeg = None;
        if !args.input_only {
            match caps {
                Some(c) => match video::spawn(&args.ffmpeg, &video::video_path(&dir), args.fps, &c) {
                    Ok(mut child) => {
                        // Échec d'init (encodeur/grab) se voit dans la 1re seconde.
                        std::thread::sleep(Duration::from_millis(1200));
                        if video::alive(&mut child) {
                            ffmpeg = Some(child);
                        } else {
                            video_failed = true;
                            let _ = child.wait();
                        }
                    }
                    Err(e) => {
                        video_failed = true;
                        eprintln!("[!] ffmpeg spawn: {e}");
                    }
                },
                None => video_failed = true,
            }
        }
        Ok(Self {
            dir,
            log,
            ffmpeg,
            video_failed,
            args: args.clone(),
            started: Instant::now(),
            started_unix_ms,
            qpc_start,
            kinds: [0; 4],
            timeline: Vec::new(),
        })
    }

    fn on_ev(&mut self, e: &Ev) {
        match e.kind {
            EV_MOUSE_MOVE => self.kinds[0] += 1,
            EV_BUTTON => self.kinds[1] += 1,
            EV_KEY => self.kinds[2] += 1,
            EV_WHEEL => self.kinds[3] += 1,
            _ => {}
        }
        let _ = self.log.write(e);
    }

    /// Écrit meta.json immédiatement (sécurise la session même si le process
    /// est tué avant le finish propre), puis le réécrit à la fin avec les
    /// compteurs finaux.
    fn write_meta(&self, final_write: bool) -> io::Result<()> {
        let video_ok = self
            .ffmpeg
            .is_none()
            .then_some(())
            .map(|_| video::video_path(&self.dir))
            .filter(|p| p.metadata().map(|m| m.len() > 0).unwrap_or(false))
            .is_some();
        let (video_status, video_file) = if self.args.input_only {
            ("disabled", None)
        } else if self.video_failed {
            ("failed", None)
        } else if video_ok || final_write {
            ("ok", Some("video.mp4".to_string()))
        } else {
            ("recording", None)
        };
        let (w, h) = screen_size();
        let meta = Meta {
            app: "ats-aimer-recorder",
            version: env!("CARGO_PKG_VERSION"),
            game: &self.args.game,
            sens_declared: self.args.sens,
            dpi_declared: self.args.dpi,
            fps_capture: self.args.fps,
            video_status,
            video_file,
            input_file: "input.atsi",
            started_unix_ms: self.started_unix_ms,
            stopped_unix_ms: input::unix_ms(),
            duration_ms: self.started.elapsed().as_millis() as u64,
            qpc_freq: input::qpc_freq(),
            qpc_start: self.qpc_start,
            events_total: self.log.events,
            events_by_kind: self.kinds,
            monitor: self.args.monitor,
            screen_w: w,
            screen_h: h,
            focus_timeline: self.timeline.clone(),
        };
        fs::write(self.dir.join("meta.json"), serde_json::to_string_pretty(&meta)?)
    }

    fn finish(mut self) -> io::Result<()> {
        let _ = self.log.flush();
        if let Some(mut c) = self.ffmpeg.take() {
            video::stop(&mut c);
        }
        self.write_meta(true)?;
        println!(
            "■ session saved → {} ({:.0}s, {} events{})",
            self.dir.display(),
            self.started.elapsed().as_secs_f32(),
            self.log.events,
            if self.video_failed { ", vidéo KO" } else { "" }
        );
        Ok(())
    }
}

fn screen_size() -> (i32, i32) {
    unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) }
}

pub struct Controller {
    args: RecordArgs,
    caps: Option<Caps>,
    probed: bool,
    session: Option<Session>,
}

impl Controller {
    pub fn new(args: RecordArgs) -> Self {
        Self { args, caps: None, probed: false, session: None }
    }

    fn toggle(&mut self) {
        if let Some(s) = self.session.take() {
            if let Err(e) = s.finish() {
                eprintln!("[!] finish failed: {e}");
            }
            println!("○ standby — F8 pour relancer, Ctrl+C pour quitter");
            return;
        }
        if !self.probed && !self.args.input_only {
            println!("… probe ffmpeg (encodeur/capture réels)");
            self.caps = video::probe(&self.args.ffmpeg);
            match &self.caps {
                Some(c) => println!(
                    "   grab={:?} enc={:?}",
                    c.grab, c.encoder
                ),
                None => eprintln!("[!] aucun encodeur dispo — vidéo désactivée"),
            }
            self.probed = true;
        }
        match Session::start(&self.args, self.caps.clone()) {
            Ok(s) => {
                let _ = s.write_meta(false); // meta dès le départ (survit à un kill)
                let failed = s.video_failed;
                self.session = Some(s);
                println!(
                    "● REC{} — F8 pour stopper",
                    if self.args.input_only { " (input only)" }
                    else if failed { " (inputs seuls — vidéo KO)" }
                    else { "" }
                );
            }
            Err(e) => eprintln!("[!] start failed: {e}"),
        }
    }

    pub fn run(&mut self, rx: Receiver<Msg>) {
        loop {
            match rx.recv_timeout(Duration::from_secs(2)) {
                Ok(Msg::Ev(e)) => {
                    if let Some(s) = self.session.as_mut() {
                        s.on_ev(&e);
                    }
                }
                Ok(Msg::Toggle) => self.toggle(),
                Ok(Msg::Focus(t)) => {
                    if let Some(s) = self.session.as_mut() {
                        let t_ms = (input::qpc_now().saturating_sub(s.qpc_start) * 1000)
                            / input::qpc_freq();
                        s.timeline.push(FocusEvt { t_ms, title: t });
                    }
                }
                Ok(Msg::Quit) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if let Some(s) = self.session.as_mut() {
                        let _ = s.log.flush();
                        println!(
                            "● REC {:>7.1}s — {} events",
                            s.started.elapsed().as_secs_f32(),
                            s.log.events
                        );
                    }
                }
            }
        }
        if let Some(s) = self.session.take() {
            let _ = s.finish();
        }
    }
}
