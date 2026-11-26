//! christmas-tree-ultimate-setup.exe: a small installer carrying this app's executable.
//! `--silent` (or `/S`) installs without showing anything, for package managers.

#![windows_subsystem = "windows"]

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, VK_ESCAPE, VK_RETURN};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, Result, w};

use christmas_tree_ultimate::card::{self, Card, Content, Hits};
use christmas_tree_ultimate::install;
use christmas_tree_ultimate::prerequisites::{self, Outcome, Phase};
use snowhush_core::text::{self, tr};

/// The app this installer carries, built before it.
const APP: &[u8] = include_bytes!(env!("CHRISTMAS_TREE_ULTIMATE_APP"));
const VERSION: &str = env!("CARGO_PKG_VERSION");
// These environment variables are needed only while compiling this setup bin.
// The app and ordinary unit tests never embed or require a redistributable.
const VC_REDIST: &[u8] = include_bytes!(env!("CHRISTMAS_TREE_ULTIMATE_VC_REDIST"));
const VC_VERSION: &str = env!("CHRISTMAS_TREE_ULTIMATE_VC_VERSION");
const CLASS: PCWSTR = w!("ChristmasTreeUltimateSetup");
const WIDTH: f32 = 400.0;
const HEIGHT: f32 = 380.0;
const WM_SETUP_EVENT: u32 = WM_APP + 1;
const TIMER_PROGRESS: usize = 1;

enum Stage {
    Ready,
    Installing(Phase),
    Done(PathBuf, bool),
    Failed(String),
}

struct Setup {
    window: HWND,
    card: Card,
    hits: Hits,
    stage: Stage,
    installed: Option<String>,
    pressed: bool,
    holding: bool,
    events: Option<Receiver<WorkerEvent>>,
}

enum WorkerEvent {
    Progress(Phase),
    Finished(std::result::Result<(PathBuf, Outcome), String>),
}

thread_local! {
    static SETUP: RefCell<Option<Setup>> = const { RefCell::new(None) };
}

fn main() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    christmas_tree_ultimate::language::choose();
    if std::env::args().skip(1).any(|argument| argument.eq_ignore_ascii_case("--check-prerequisites")) {
        // Diagnostic mode is read-only and never falls through to installation.
        match prerequisites::check_ready() {
            Ok(()) => std::process::exit(0),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    }
    if std::env::args().skip(1).any(|argument| argument.eq_ignore_ascii_case("--silent") || argument.eq_ignore_ascii_case("/s")) {
        let result = prerequisites::prepare(VC_REDIST, VC_VERSION, |_| {}).and_then(|outcome| install::install(APP, VERSION).map(|_| outcome));
        std::process::exit(match result {
            Ok(outcome) if outcome.restart_required => 3010,
            Ok(_) => 0,
            Err(_) => 1,
        });
    }
    if let Err(error) = open() {
        let message = windows::core::HSTRING::from(text::fill("setup.error.ui", &[("error", &error.to_string())]));
        unsafe {
            MessageBoxW(None, &message, w!("christmas tree ultimate"), MB_OK | MB_ICONERROR);
        }
        return;
    }
    unsafe {
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

fn open() -> Result<()> {
    card::register(CLASS, Some(procedure))?;
    let installed = install::installed_version();
    let (scale, _) = card::placement(WIDTH, HEIGHT);
    let stages = [Stage::Ready, Stage::Done(PathBuf::new(), false), Stage::Done(PathBuf::new(), true)];
    let words: Vec<_> = stages.iter().map(|stage| words(stage, installed.as_deref())).collect();
    let contents: Vec<_> = words.iter().map(|words| content(words, false)).collect();
    let height = Card::new(WIDTH, HEIGHT, scale)?.fitting_height(&contents, HEIGHT)?;
    let (scale, origin) = card::placement(WIDTH, height);
    let card = Card::new(WIDTH, height, scale)?;
    let (pixels, hits) = paint(&card, &Stage::Ready, installed.as_deref(), false)?;
    let title: Vec<u16> = tr("app.name").encode_utf16().chain([0]).collect();
    unsafe {
        let window = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_APPWINDOW,
            CLASS,
            PCWSTR(title.as_ptr()),
            WS_POPUP | WS_SYSMENU | WS_MINIMIZEBOX,
            origin.x,
            origin.y,
            card.size.0,
            card.size.1,
            None,
            None,
            None,
            None,
        )?;
        card::present(window, card.size, &pixels, Some(origin));
        SETUP.set(Some(Setup { window, card, hits, stage: Stage::Ready, installed, pressed: false, holding: false, events: None }));
        SetTimer(Some(window), TIMER_PROGRESS, 100, None);
        let _ = ShowWindow(window, SW_SHOW);
        let _ = SetForegroundWindow(window);
    }
    Ok(())
}

/// The card for a stage of the installation.
fn paint(card: &Card, stage: &Stage, installed: Option<&str>, pressed: bool) -> Result<(Vec<u8>, Hits)> {
    let installing = matches!(stage, Stage::Installing(_));
    card.paint(&content(&words(stage, installed), pressed || installing))
}

/// A stage's title, text and button, the title and button in capitals.
struct Words {
    title: String,
    body: String,
    button: String,
    closable: bool,
}

fn words(stage: &Stage, installed: Option<&str>) -> Words {
    let version = [("version", VERSION)];
    let (title, body, button) = match stage {
        Stage::Ready => {
            let (body, button) = match installed {
                None => (text::fill("setup.fresh", &version), "setup.install"),
                Some(old) if snowhush_core::version::newer(VERSION, old) => {
                    (text::fill("setup.newer", &[("old", old), ("version", VERSION)]), "setup.update")
                }
                Some(old) => (text::fill("setup.same", &[("old", old), ("version", VERSION)]), "setup.reinstall"),
            };
            (tr("app.name"), format!("{body}\n\n{}\n\n{}", tr("setup.prerequisites"), tr("setup.autostart")), button)
        }
        Stage::Installing(phase) => (tr("app.name"), format!("{}\n\n{}", phase.label(), tr("setup.progress.wait")), "setup.installing"),
        Stage::Done(_, false) => (tr("setup.settle"), tr("setup.done").to_string(), "setup.open"),
        Stage::Done(_, true) => (tr("setup.restart.title"), tr("setup.restart.done").to_string(), "setup.finish"),
        Stage::Failed(error) => (tr("app.name"), text::fill("setup.failed", &[("error", error)]), "setup.retry"),
    };
    let button = tr(button).to_uppercase().replace("CHRISTMAS TREE ULTIMATE", "christmas tree ultimate");
    Words { title: title.to_string(), body, button, closable: !matches!(stage, Stage::Installing(_)) }
}

fn content(words: &Words, pressed: bool) -> Content<'_> {
    Content { icon: 96.0, title: &words.title, title_size: 28.0, body: &words.body, button: &words.button, pressed, closable: words.closable }
}

unsafe extern "system" fn procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_SETUP_EVENT => {
            poll_events();
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == TIMER_PROGRESS => {
            poll_events();
            LRESULT(0)
        }
        WM_NCHITTEST => {
            let mut origin = RECT::default();
            unsafe {
                let _ = GetWindowRect(window, &mut origin);
            }
            let point = card::point(lparam);
            let inside = SETUP.with_borrow(|setup| {
                setup.as_ref().map(|setup| {
                    let at = setup.card.dips(POINT { x: point.x - origin.left, y: point.y - origin.top });
                    card::contains(&setup.hits.button, at) || setup.hits.close.is_some_and(|close| card::contains(&close, at))
                })
            });
            // The card drags the window anywhere but on its controls.
            LRESULT(if inside == Some(false) { HTCAPTION as isize } else { HTCLIENT as isize })
        }
        WM_LBUTTONDOWN | WM_MOUSEMOVE | WM_LBUTTONUP => {
            unsafe {
                if message == WM_LBUTTONDOWN {
                    SetCapture(window);
                } else if message == WM_LBUTTONUP {
                    let _ = ReleaseCapture();
                }
            }
            let at = card::point(lparam);
            let outcome = SETUP.with_borrow_mut(|setup| setup.as_mut().map(|setup| setup.press(message, at)));
            if let Some((action, repaint)) = outcome {
                if let Some(pixels) = repaint {
                    show(&pixels);
                }
                match action {
                    Some(Action::Button) => act(),
                    Some(Action::Close) => close(),
                    None => {}
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 == VK_RETURN.0 as usize => {
            act();
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 == VK_ESCAPE.0 as usize => {
            close();
            LRESULT(0)
        }
        WM_CLOSE => {
            close();
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

enum Action {
    Button,
    Close,
}

impl Setup {
    /// Tracks a press on the button or the close mark: what was clicked, and the card redrawn if the button's look changed.
    fn press(&mut self, message: u32, point: POINT) -> (Option<Action>, Option<Vec<u8>>) {
        let at = self.card.dips(point);
        let on_button = card::contains(&self.hits.button, at);
        let on_close = self.hits.close.is_some_and(|close| card::contains(&close, at));
        let mut action = None;
        match message {
            WM_LBUTTONDOWN => self.holding = on_button,
            WM_LBUTTONUP => {
                if self.holding && on_button {
                    action = Some(Action::Button);
                } else if on_close {
                    action = Some(Action::Close);
                }
                self.holding = false;
            }
            _ if !self.holding => return (None, None),
            _ => {}
        }
        let pressed = self.holding && on_button;
        if pressed == self.pressed {
            return (action, None);
        }
        self.pressed = pressed;
        (action, self.repaint())
    }

    fn repaint(&mut self) -> Option<Vec<u8>> {
        let (pixels, hits) = paint(&self.card, &self.stage, self.installed.as_deref(), self.pressed).ok()?;
        self.hits = hits;
        Some(pixels)
    }
}

fn show(pixels: &[u8]) {
    if let Some((window, size)) = SETUP.with_borrow(|setup| setup.as_ref().map(|setup| (setup.window, setup.card.size))) {
        card::present(window, size, pixels, None);
    }
}

/// Moves the card to a new stage and shows it.
fn stage(stage: Stage) {
    if let Some(pixels) = SETUP.with_borrow_mut(|setup| {
        let setup = setup.as_mut()?;
        setup.stage = stage;
        setup.pressed = false;
        // A diagnostic may need more lines than the ready card. Refit this card
        // rather than silently clipping the actionable Windows error message.
        if let Ok(height) = setup.card.fitting_height(&[content(&words(&setup.stage, setup.installed.as_deref()), false)], HEIGHT) {
            if height > setup.card.height {
                if let Ok(card) = Card::new(WIDTH, height, setup.card.scale) {
                    setup.card = card;
                }
            }
        }
        setup.repaint()
    }) {
        show(&pixels);
    }
}

/// The button: installs, or once installed opens the app and leaves.
fn act() {
    let done = SETUP.with_borrow(|setup| match setup.as_ref().map(|setup| &setup.stage) {
        Some(Stage::Done(exe, restart)) => Some(Some((exe.clone(), *restart))),
        Some(Stage::Installing(_)) | None => None,
        _ => Some(None),
    });
    match done {
        Some(Some((exe, restart))) => {
            if !restart {
                install::launch(&exe);
            }
            close();
        }
        Some(None) => {
            begin_installation();
        }
        None => {}
    }
}

fn close() {
    if SETUP.with_borrow(|setup| setup.as_ref().is_some_and(|setup| matches!(setup.stage, Stage::Installing(_)))) {
        return;
    }
    if let Some(setup) = SETUP.take() {
        unsafe {
            let _ = DestroyWindow(setup.window);
        }
    }
}

fn begin_installation() {
    let (sender, receiver) = mpsc::channel();
    let window = SETUP.with_borrow_mut(|setup| {
        let setup = setup.as_mut()?;
        setup.events = Some(receiver);
        Some(setup.window.0 as isize)
    });
    let Some(window) = window else {
        return;
    };
    stage(Stage::Installing(Phase::Windows));
    let started = std::thread::Builder::new().name("setup-worker".into()).spawn(move || {
        let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.ok();
        let result = match initialized {
            Ok(()) => {
                let notify = |phase| {
                    let _ = sender.send(WorkerEvent::Progress(phase));
                    unsafe {
                        let _ = PostMessageW(Some(HWND(window as *mut _)), WM_SETUP_EVENT, WPARAM(0), LPARAM(0));
                    }
                };
                let result = prerequisites::prepare(VC_REDIST, VC_VERSION, notify).and_then(|outcome| {
                    notify(Phase::Files);
                    install::install(APP, VERSION).map(|exe| (exe, outcome))
                });
                unsafe {
                    CoUninitialize();
                }
                result
            }
            Err(error) => Err(text::fill("setup.error.worker", &[("error", &error.to_string())])),
        };
        let _ = sender.send(WorkerEvent::Finished(result));
        unsafe {
            let _ = PostMessageW(Some(HWND(window as *mut _)), WM_SETUP_EVENT, WPARAM(0), LPARAM(0));
        }
    });
    if let Err(error) = started {
        stage(Stage::Failed(text::fill("setup.error.worker", &[("error", &error.to_string())])));
    }
}

fn poll_events() {
    let events = SETUP.with_borrow(|setup| {
        setup.as_ref().and_then(|setup| setup.events.as_ref()).map(|events| events.try_iter().collect::<Vec<_>>()).unwrap_or_default()
    });
    for event in events {
        stage(match event {
            WorkerEvent::Progress(phase) => Stage::Installing(phase),
            WorkerEvent::Finished(Ok((path, outcome))) => Stage::Done(path, outcome.restart_required),
            WorkerEvent::Finished(Err(error)) => Stage::Failed(error),
        });
    }
}
