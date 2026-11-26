//! Pompe Win32 : fenêtre message-only + Raw Input (RIDEV_INPUTSINK) + hotkey F8.
//!
//! `RIDEV_INPUTSINK` fait arriver `WM_INPUT` à notre fenêtre même quand le jeu
//! est au premier plan — lecture passive, aucune interception ni réécriture.

use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use windows::core::{w, Result, BOOL};
use windows::Win32::Devices::HumanInterfaceDevice::{
    HID_USAGE_GENERIC_KEYBOARD, HID_USAGE_GENERIC_MOUSE, HID_USAGE_PAGE_GENERIC,
};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Console::{
    SetConsoleCtrlHandler, CTRL_BREAK_EVENT, CTRL_CLOSE_EVENT, CTRL_C_EVENT,
};
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, VK_F8, MOD_NOREPEAT,
};
use windows::Win32::UI::Input::{
    GetRawInputData, RegisterRawInputDevices, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE,
    RAWKEYBOARD, RAWMOUSE, RID_INPUT, RIM_TYPEKEYBOARD, RIM_TYPEMOUSE,
    RIDEV_INPUTSINK,
};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::event::{Ev, Msg, EV_BUTTON, EV_KEY, EV_MOUSE_MOVE, EV_WHEEL};

static TX: OnceLock<Sender<Msg>> = OnceLock::new();
static CTRL_TX: OnceLock<Sender<Msg>> = OnceLock::new();
static SINK: AtomicIsize = AtomicIsize::new(0);

const TIMER_FOCUS: usize = 1;

/// Touches loguées — mouvement + touches de jeu courantes.
/// La whitelist évite de devenir un keylogger (chat, mots de passe).
const KEYS: &[u16] = &[
    0x5A, 0x57, 0x41, 0x53, 0x44, 0x51, // Z W A S D Q
    0x20,       // Space (jump)
    0xA0, 0xA1, // L/R Shift (walk)
    0xA2, 0xA3, // L/R Ctrl (crouch)
    0x09, 0x14, // Tab, CapsLock
    0x45, 0x43, 0x58, 0x52, 0x46, 0x47, 0x42, 0x56, // E C X R F G B V
    0x31, 0x32, 0x33, 0x34, 0x35, // 1-5 armes
    0x25, 0x26, 0x27, 0x28, // flèches
];

pub fn qpc_now() -> u64 {
    let mut v = 0i64;
    unsafe {
        let _ = QueryPerformanceCounter(&mut v);
    }
    v as u64
}

pub fn qpc_freq() -> u64 {
    let mut v = 0i64;
    unsafe {
        let _ = QueryPerformanceFrequency(&mut v);
    }
    v as u64
}

pub fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[inline]
fn send(m: Msg) {
    if let Some(tx) = TX.get() {
        let _ = tx.send(m);
    }
}

#[inline]
fn send_ev(qpc: u64, kind: u16, flags: u16, a: i32, b: i32, c: i32) {
    send(Msg::Ev(Ev { qpc, kind, flags, a, b, c }));
}

unsafe fn handle_mouse(m: &RAWMOUSE, qpc: u64) {
    unsafe {
        // Deltas relatifs (ce que le jeu consomme). Mode absolu rare -> logué
        // aussi, flags conservés pour que l'analyse puisse distinguer.
        if m.lLastX != 0 || m.lLastY != 0 {
            send_ev(qpc, EV_MOUSE_MOVE, m.usFlags.0, m.lLastX, m.lLastY, 0);
        }
        let b = m.Anonymous.ulButtons;
        const BTNS: &[(u32, i32, i32)] = &[
            (RI_MOUSE_LEFT_BUTTON_DOWN, 1, 1),
            (RI_MOUSE_LEFT_BUTTON_UP, 1, 0),
            (RI_MOUSE_RIGHT_BUTTON_DOWN, 2, 1),
            (RI_MOUSE_RIGHT_BUTTON_UP, 2, 0),
            (RI_MOUSE_MIDDLE_BUTTON_DOWN, 3, 1),
            (RI_MOUSE_MIDDLE_BUTTON_UP, 3, 0),
            (RI_MOUSE_BUTTON_4_DOWN, 4, 1),
            (RI_MOUSE_BUTTON_4_UP, 4, 0),
            (RI_MOUSE_BUTTON_5_DOWN, 5, 1),
            (RI_MOUSE_BUTTON_5_UP, 5, 0),
        ];
        for &(flag, id, down) in BTNS {
            if b & flag != 0 {
                send_ev(qpc, EV_BUTTON, 0, id, down, 0);
            }
        }
        if b & (RI_MOUSE_WHEEL | RI_MOUSE_HWHEEL) != 0 {
            send_ev(qpc, EV_WHEEL, 0, m.Anonymous.Anonymous.usButtonData as i16 as i32, 0, 0);
        }
    }
}

unsafe fn handle_key(k: &RAWKEYBOARD, qpc: u64) {
    let vk = k.VKey;
    if vk == 255 || !KEYS.contains(&vk) {
        return;
    }
    // RI_KEY_MAKE=0 (down), RI_KEY_BREAK=1 (up). Les auto-repeat sont gardés —
    // le filtrage se fait à l'analyse (champ Message disponible).
    let down = (k.Flags as u32 & RI_KEY_BREAK) == 0;
    send_ev(qpc, EV_KEY, k.Flags, vk as i32, down as i32, 0);
}

unsafe fn handle_input(lp: LPARAM) {
    unsafe {
        let hraw = HRAWINPUT(lp.0 as *mut _);
        let hdr = std::mem::size_of::<windows::Win32::UI::Input::RAWINPUTHEADER>() as u32;
        let mut size = 0u32;
        if GetRawInputData(hraw, RID_INPUT, None, &mut size, hdr) != 0 || size == 0 {
            return;
        }
        let mut buf = vec![0u8; size as usize];
        let got = GetRawInputData(
            hraw,
            RID_INPUT,
            Some(buf.as_mut_ptr() as *mut _),
            &mut size,
            hdr,
        );
        if got != size {
            return;
        }
        let raw = &*(buf.as_ptr() as *const RAWINPUT);
        let qpc = qpc_now();
        match raw.header.dwType {
            t if t == RIM_TYPEMOUSE.0 => handle_mouse(&raw.data.mouse, qpc),
            t if t == RIM_TYPEKEYBOARD.0 => handle_key(&raw.data.keyboard, qpc),
            _ => {}
        }
    }
}

thread_local! {
    static LAST_FOCUS: RefCell<String> = const { RefCell::new(String::new()) };
}

unsafe fn check_focus() {
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut buf = [0u16; 256];
        let n = GetWindowTextW(hwnd, &mut buf).max(0) as usize;
        let title = String::from_utf16_lossy(&buf[..n]);
        LAST_FOCUS.with(|last| {
            let mut last = last.borrow_mut();
            if *last != title {
                *last = title.clone();
                send(Msg::Focus(title));
            }
        });
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_INPUT => {
                handle_input(lp);
                LRESULT(0)
            }
            WM_HOTKEY => {
                send(Msg::Toggle);
                LRESULT(0)
            }
            WM_TIMER => {
                if wp.0 == TIMER_FOCUS {
                    check_focus();
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

unsafe extern "system" fn ctrl_handler(t: u32) -> BOOL {
    unsafe {
        match t {
            CTRL_C_EVENT | CTRL_BREAK_EVENT | CTRL_CLOSE_EVENT => {
                if let Some(tx) = CTRL_TX.get() {
                    let _ = tx.send(Msg::Quit);
                }
                let h = SINK.load(Ordering::Relaxed);
                if h != 0 {
                    let _ = PostMessageW(Some(HWND(h as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
                // TRUE = on gère nous-mêmes : le process survit jusqu'à la fin
                // du pump (flush log + fermeture propre ffmpeg).
                BOOL(1)
            }
            _ => BOOL(0),
        }
    }
}

/// Installe le handler Ctrl+C — à appeler une fois, depuis main.
pub fn install_ctrl_handler(tx: Sender<Msg>) {
    let _ = CTRL_TX.set(tx);
    unsafe {
        let _ = SetConsoleCtrlHandler(Some(ctrl_handler), true);
    }
}

/// Fenêtre message-only + devices + hotkey + pompe. Bloquant : à lancer sur un
/// thread dédié.
pub fn run_pump(tx: Sender<Msg>) -> Result<()> {
    TX.set(tx.clone()).ok();
    unsafe {
        let class = w!("ats_aimer_input_sink");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassExW(&wc);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            w!("ats-aimer-input"),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            None,
            None,
        )?;
        SINK.store(hwnd.0 as isize, Ordering::Relaxed);

        RegisterHotKey(Some(hwnd), 1, MOD_NOREPEAT, VK_F8.0 as u32).ok();
        RegisterRawInputDevices(
            &[
                RAWINPUTDEVICE {
                    usUsagePage: HID_USAGE_PAGE_GENERIC,
                    usUsage: HID_USAGE_GENERIC_MOUSE,
                    dwFlags: RIDEV_INPUTSINK,
                    hwndTarget: hwnd,
                },
                RAWINPUTDEVICE {
                    usUsagePage: HID_USAGE_PAGE_GENERIC,
                    usUsage: HID_USAGE_GENERIC_KEYBOARD,
                    dwFlags: RIDEV_INPUTSINK,
                    hwndTarget: hwnd,
                },
            ],
            std::mem::size_of::<RAWINPUTDEVICE>() as u32,
        )?;
        SetTimer(Some(hwnd), TIMER_FOCUS, 1000, None);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }
    }
    Ok(())
}
