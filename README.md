# ATS-AIMER

**The black box for your aim.** ATS-AIMER passively records your VALORANT matches — GPU-encoded video plus raw mouse/keyboard input at full polling rate — then analyzes every duel *after* the match to give you biomechanical evidence, not vibes: first-flick error, micro-adjustment time, click jitter, shots fired while moving, and duel-to-duel consistency. It turns those into a statistical sensitivity recommendation — an equilibrium point plus a tolerance "green zone" — instead of a magic number.

## Why this exists

Aim trainers measure you on synthetic targets. Stat trackers see your scoreboard, not your hand. Between the two, nothing tells you what your mouse and keyboard actually did during a real ranked duel. ATS-AIMER closes that loop by fusing what was on screen with what your hand physically did, millisecond by millisecond, on real games.

## How it works

| Module | Role |
|---|---|
| **1. Black-box recorder** (`recorder/`) | Passive screen capture (Windows Graphics Capture / NVENC) + Raw Input event log (Δx/Δy at polling rate, clicks, movement keys) with microsecond timestamps. Zero interaction with the game process. |
| **2. Duel slicer** (`analysis/`) | Post-match: OpenCV finds every engagement via killfeed/HUD cues and click bursts, then cuts ~3 s clips around each duel. |
| **3. Biomechanical engine** (`analysis/`) | Enemy outline detection (VALORANT's highlight color) fused with the physical mouse curve and keyboard state → per-duel kinematic metrics. |
| **4. Sensitivity lab** (`app/`) | Aggregates duels across sessions, computes your equilibrium point + green zone, and emits data-driven micro-adjustment rules ("over-flick > 60 % on last 3 sessions → −5 %"). |

### Core metrics per duel

- **First-flick signed error** — crosshair-to-head distance when the first big movement stops (`+` overshoot → sens too high, `−` undershoot → too low)
- **Micro-adjustment time** — ms between flick end and head shot (instability / posture)
- **Click jitter** — involuntary cursor displacement during the 20 ms of left-click
- **Moving-shot rate** — shots fired while movement keys still active (anti-noise filter, not a skill score)
- **Consistency index** — σ of target-acquisition time across duels

## Vanguard-safe by design

ATS-AIMER **never** reads game memory, never injects, never sends synthetic input, and never analyzes anything in real-time during play. It sits in the same category as OBS recording + post-match VOD review under Riot's third-party application policy.

*Not affiliated with Riot Games. VALORANT is a trademark of Riot Games, Inc.*

## Stack

`recorder/` Rust (windows-rs) · `analysis/` Python (OpenCV, Ultralytics, NumPy/SciPy) · `app/` Tauri + React

## Roadmap

- [x] **M0 — Recorder**: Raw Input logger + screen capture → session files (video + input log)
- [ ] **M1 — Duel slicer**: killfeed/HUD event detection + click bursts → clip cut
- [ ] **M2 — Biomechanical engine**: enemy outline tracking + the 5 metrics
- [ ] **M3 — Sensitivity lab**: multi-session blind protocol, green zone, coach cards, Tauri app
- [ ] **M4 — Generalization**: CS2 profile, more games

See [`docs/SPEC.md`](docs/SPEC.md) for the full specification.
