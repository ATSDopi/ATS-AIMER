//! Capture vidéo via FFmpeg : probe RÉEL des capacités (un encodeur listé par
//! `-encoders` peut échouer à l'init — ex. nvenc sans nvcuda.dll), puis
//! spawn/stop propre (`q` sur stdin).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Encoder {
    Nvenc,
    X264,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Grab {
    Ddagrab,
    Gdigrab,
}

#[derive(Clone)]
pub struct Caps {
    pub grab: Grab,
    pub encoder: Encoder,
}

/// Test réel d'un encodeur : encode 3 frames synthétiques.
fn encoder_works(ffmpeg: &Path, enc: &str) -> bool {
    Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=0.3:size=64x64:rate=10",
            "-c:v",
            enc,
            "-f",
            "null",
            "-",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Test réel du demuxer ddagrab : capture 1 frame du desktop.
fn ddagrab_works(ffmpeg: &Path) -> bool {
    Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "ddagrab",
            "-i",
            "desktop",
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Probe les capacités en lançant de vrais micro-encodages (~1 s).
pub fn probe(ffmpeg: &Path) -> Option<Caps> {
    let encoder = if encoder_works(ffmpeg, "h264_nvenc") {
        Encoder::Nvenc
    } else if encoder_works(ffmpeg, "libx264") {
        Encoder::X264
    } else {
        return None;
    };
    let grab = if ddagrab_works(ffmpeg) {
        Grab::Ddagrab
    } else {
        Grab::Gdigrab
    };
    Some(Caps { grab, encoder })
}

pub fn spawn(ffmpeg: &Path, out: &Path, fps: u32, caps: &Caps) -> std::io::Result<Child> {
    let mut args: Vec<String> =
        ["-hide_banner".into(), "-loglevel".into(), "warning".into(), "-y".into()].to_vec();
    match caps.grab {
        Grab::Ddagrab => args.extend(
            ["-f", "ddagrab", "-framerate"].iter().map(|s| s.to_string()),
        ),
        Grab::Gdigrab => args.extend(
            ["-f", "gdigrab", "-draw_mouse", "0", "-framerate"]
                .iter()
                .map(|s| s.to_string()),
        ),
    }
    args.extend([fps.to_string(), "-i".into(), "desktop".into()]);
    match caps.encoder {
        Encoder::Nvenc => args.extend(
            ["-c:v", "h264_nvenc", "-preset", "p4", "-pix_fmt", "yuv420p"]
                .iter()
                .map(|s| s.to_string()),
        ),
        Encoder::X264 => args.extend(
            [
                "-c:v", "libx264", "-preset", "ultrafast", "-tune", "zerolatency", "-crf", "23",
                "-pix_fmt", "yuv420p",
            ]
            .iter()
            .map(|s| s.to_string()),
        ),
    }
    args.push(out.to_string_lossy().into_owned());

    Command::new(ffmpeg)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
}

/// Le child a-t-il survécu à l'amorçage ? (détecte l'échec d'init encodeur)
pub fn alive(child: &mut Child) -> bool {
    matches!(child.try_wait(), Ok(None))
}

/// Finalise proprement le mp4 (envoie `q`), puis tue si ça traîne.
pub fn stop(child: &mut Child) {
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(b"q\n");
        let _ = stdin.flush();
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => std::thread::sleep(Duration::from_millis(80)),
            Err(_) => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

pub fn video_path(dir: &Path) -> PathBuf {
    dir.join("video.mp4")
}
