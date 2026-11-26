//! ATS-AIMER recorder — boîte noire passive (Raw Input + capture écran GPU).
//! Zéro lecture mémoire, zéro injection, zéro input synthétique.

mod event;
mod input;
mod session;
mod video;

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use clap::{Args, Parser, Subcommand};
use event::{Msg, EV_BUTTON, EV_KEY, EV_MOUSE_MOVE, EV_WHEEL};
use session::{Controller, RecordArgs};

#[derive(Parser)]
#[command(name = "ats-aimer", version, about = "Black-box aim telemetry recorder")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
    #[command(flatten)]
    rec: CliRecordArgs,
}

#[derive(Subcommand)]
enum Cmd {
    /// Stats d'entrée en direct (polling rate, compteurs)
    InputTest,
}

#[derive(Args)]
struct CliRecordArgs {
    /// Profil jeu (stocké dans meta.json)
    #[arg(long, default_value = "valorant")]
    game: String,
    /// Sensibilité déclarée en jeu (baseline)
    #[arg(long)]
    sens: Option<f64>,
    /// DPI déclaré de la souris
    #[arg(long)]
    dpi: Option<u32>,
    /// Dossier de sortie des sessions
    #[arg(long, default_value = "recordings")]
    out: PathBuf,
    /// Exécutable ffmpeg
    #[arg(long, default_value = "ffmpeg")]
    ffmpeg: PathBuf,
    /// Framerate de capture écran
    #[arg(long, default_value_t = 60)]
    fps: u32,
    /// Télémétrie seule, pas de vidéo
    #[arg(long)]
    input_only: bool,
    /// Index du moniteur capturé
    #[arg(long, default_value_t = 0)]
    monitor: u32,
}

fn main() {
    let cli = Cli::parse();
    let (tx, rx) = mpsc::channel::<Msg>();
    input::install_ctrl_handler(tx.clone());

    let pump = {
        let tx = tx.clone();
        std::thread::spawn(move || {
            if let Err(e) = input::run_pump(tx) {
                eprintln!("[!] input pump: {e}");
            }
        })
    };

    match cli.cmd {
        Some(Cmd::InputTest) => run_input_test(rx),
        None => {
            let args = RecordArgs {
                game: cli.rec.game,
                sens: cli.rec.sens,
                dpi: cli.rec.dpi,
                out: cli.rec.out,
                ffmpeg: cli.rec.ffmpeg,
                fps: cli.rec.fps,
                input_only: cli.rec.input_only,
                monitor: cli.rec.monitor,
            };
            let _ = tx.send(Msg::Toggle); // démarre l'enregistrement
            Controller::new(args).run(rx);
        }
    }
    let _ = pump.join();
}

fn run_input_test(rx: mpsc::Receiver<Msg>) {
    println!("input test — bouge la souris / touches whitelistées. Ctrl+C pour quitter.");
    let mut counts = [0u64; 4];
    let mut window = 0u64;
    let mut t0 = Instant::now();
    let total_t0 = Instant::now();
    loop {
        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Msg::Ev(e)) => {
                let i = match e.kind {
                    EV_MOUSE_MOVE => 0,
                    EV_BUTTON => 1,
                    EV_KEY => 2,
                    EV_WHEEL => 3,
                    _ => continue,
                };
                counts[i] += 1;
                window += 1;
            }
            Ok(Msg::Quit) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Ok(_) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let dt = t0.elapsed().as_secs_f64();
                let hz = if dt > 0.0 { window as f64 / dt } else { 0.0 };
                println!(
                    "[{:>5.1}s] {:>6.0} ev/s | move={} btn={} key={} wheel={}",
                    total_t0.elapsed().as_secs_f32(),
                    hz,
                    counts[0],
                    counts[1],
                    counts[2],
                    counts[3]
                );
                window = 0;
                t0 = Instant::now();
            }
        }
    }
    println!("bye.");
}
