//! Independent, click-through Christmas layer. Only its small placement handle accepts input.

use std::cell::RefCell;
use std::ffi::c_void;
use std::mem::size_of;
use std::sync::Once;

use windows::Win32::Foundation::{COLORREF, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{BLACK_BRUSH, GetStockObject, HBRUSH};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, ReleaseCapture, SetCapture, VK_ESCAPE, VK_LBUTTON};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, Result, w};

use snowhush_core::christmas::{HolidaySettings, TransportSprite};

use crate::christmas::{Scene, tree_rect};
use crate::render::{Gpu, Surface};
use crate::screen::Monitor;

const CLASS: PCWSTR = w!("ChristmasTreeUltimateDecorations");
const INPUT_CLASS: PCWSTR = w!("ChristmasTreeUltimateTreePlacement");
pub const TREE_MOVED: u32 = WM_APP + 40;
pub const PLACEMENT_FINISHED: u32 = WM_APP + 41;

struct Input {
    bounds: RECT,
    size: [f32; 2],
    settings: HolidaySettings,
    controller: Option<HWND>,
    placing: bool,
    drag: Option<[f32; 2]>,
    moved: bool,
    finished: bool,
}

impl Input {
    fn move_to_pointer(&mut self) {
        if let (Some([x, y]), Some([dx, dy])) = (self.pointer(), self.drag) {
            let [_, _, width, height] = tree_rect(self.size, &self.settings);
            self.settings.tree_x = (((x - dx) / (self.size[0] - width).max(1.0)).clamp(0.0, 1.0) * 1000.0).round() as u32;
            self.settings.tree_y = (((y - dy) / (self.size[1] - height).max(1.0)).clamp(0.0, 1.0) * 1000.0).round() as u32;
            self.moved = true;
        }
    }

    fn pointer(&self) -> Option<[f32; 2]> {
        let mut point = POINT::default();
        unsafe { GetCursorPos(&mut point) }.ok()?;
        Some([
            (point.x - self.bounds.left) as f32 * self.size[0] / (self.bounds.right - self.bounds.left).max(1) as f32,
            (self.bounds.bottom - point.y) as f32 * self.size[1] / (self.bounds.bottom - self.bounds.top).max(1) as f32,
        ])
    }

    fn finish(&mut self) {
        self.drag = None;
        self.placing = false;
        self.finished = true;
        unsafe {
            let _ = ReleaseCapture();
            if let Some(controller) = self.controller {
                let _ = PostMessageW(Some(controller), TREE_MOVED, WPARAM(self.settings.tree_x as usize), LPARAM(self.settings.tree_y as isize));
                let _ = PostMessageW(Some(controller), PLACEMENT_FINISHED, WPARAM(0), LPARAM(0));
            }
        }
    }
}

pub struct Decorations {
    window: HWND,
    handle: HWND,
    surface: Surface,
    input: Box<RefCell<Input>>,
    shown: bool,
}

impl Decorations {
    pub fn new(gpu: &Gpu, monitor: &Monitor, settings: &HolidaySettings, screenshots: bool) -> Result<Self> {
        static REGISTER: Once = Once::new();
        let instance = unsafe { GetModuleHandleW(None)? };
        REGISTER.call_once(|| {
            let class = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(procedure),
                hInstance: instance.into(),
                lpszClassName: CLASS,
                ..Default::default()
            };
            unsafe { RegisterClassExW(&class) };
            let input_class = WNDCLASSEXW { lpszClassName: INPUT_CLASS, hbrBackground: HBRUSH(unsafe { GetStockObject(BLACK_BRUSH) }.0), ..class };
            unsafe { RegisterClassExW(&input_class) };
        });
        let bounds = monitor.bounds;
        let physical = [(bounds.right - bounds.left).max(1), (bounds.bottom - bounds.top).max(1)];
        let scale = monitor.dpi.max(96) as f32 / 96.0;
        let size = [(physical[0] as f32 / scale).ceil(), (physical[1] as f32 / scale).ceil()];
        let base_style = WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
        let window = unsafe {
            CreateWindowExW(
                base_style | WS_EX_NOREDIRECTIONBITMAP | WS_EX_TRANSPARENT,
                CLASS,
                w!("christmas tree ultimate"),
                WS_POPUP,
                bounds.left,
                bounds.top,
                physical[0],
                physical[1],
                None,
                None,
                Some(instance.into()),
                None,
            )?
        };
        let surface = match Surface::new(gpu, window, size[0] as u32, size[1] as u32, (physical[0] as f32 / size[0], physical[1] as f32 / size[1])) {
            Ok(surface) => surface,
            Err(error) => {
                unsafe {
                    let _ = DestroyWindow(window);
                }
                return Err(error);
            }
        };
        let handle = match unsafe {
            CreateWindowExW(base_style, INPUT_CLASS, w!("christmas tree ultimate"), WS_POPUP, 0, 0, 1, 1, None, None, Some(instance.into()), None)
        } {
            Ok(handle) => handle,
            Err(error) => {
                unsafe {
                    let _ = DestroyWindow(window);
                }
                return Err(error);
            }
        };
        let mut input = Box::new(RefCell::new(Input {
            bounds,
            size,
            settings: *settings,
            controller: unsafe { FindWindowW(w!("ChristmasTreeUltimateController"), None) }.ok(),
            placing: false,
            drag: None,
            moved: false,
            finished: false,
        }));
        unsafe {
            SetWindowLongPtrW(handle, GWLP_USERDATA, (&raw mut *input) as isize);
            let _ = SetLayeredWindowAttributes(window, COLORREF(0), 255, LWA_ALPHA);
            // A nearly transparent, tree-sized handle; the rest of the screen stays click-through.
            let _ = SetLayeredWindowAttributes(handle, COLORREF(0), 2, LWA_ALPHA);
            let _ = SetPropW(window, w!("NonRudeHWND"), Some(HANDLE(1 as *mut c_void)));
            let _ = SetPropW(handle, w!("NonRudeHWND"), Some(HANDLE(1 as *mut c_void)));
        }
        let result = Self { window, handle, surface, input, shown: false };
        result.show_in_screenshots(screenshots);
        Ok(result)
    }

    pub fn draw(
        &mut self,
        gpu: &Gpu,
        settings: &mut HolidaySettings,
        selected: bool,
        placing: &mut bool,
        transports: Vec<TransportSprite>,
        time: f32,
        allowed: bool,
    ) -> Result<bool> {
        let mut input = self.input.borrow_mut();
        // The handle never takes focus. Poll as well as receiving mouse messages so a fast
        // pointer can leave its rectangle without losing the drag or the eventual release.
        if input.drag.is_some() {
            input.move_to_pointer();
            if unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } >= 0 {
                input.finish();
            }
        }
        if input.moved {
            settings.tree_x = input.settings.tree_x;
            settings.tree_y = input.settings.tree_y;
            input.moved = false;
        }
        if input.finished {
            *placing = false;
            input.finished = false;
        }
        input.settings = *settings;
        input.placing = allowed && *placing && selected && settings.tree;
        if !input.placing && input.drag.is_some() {
            input.finish();
        }
        if input.placing && unsafe { GetAsyncKeyState(VK_ESCAPE.0 as i32) } < 0 {
            input.finish();
            *placing = false;
        }
        let visible = allowed && (settings.garlands || (selected && settings.tree) || !transports.is_empty());
        if visible != self.shown {
            unsafe {
                let _ = ShowWindow(self.window, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
            }
            self.shown = visible;
        }
        if input.placing {
            let [left, bottom, width, height] = tree_rect(input.size, settings);
            let px = (input.bounds.right - input.bounds.left) as f32 / input.size[0];
            let py = (input.bounds.bottom - input.bounds.top) as f32 / input.size[1];
            unsafe {
                let _ = SetWindowPos(
                    self.handle,
                    Some(crate::screen::lowest_open_popup().unwrap_or(HWND_TOPMOST)),
                    input.bounds.left + (left * px).round() as i32,
                    input.bounds.bottom - ((bottom + height) * py).round() as i32,
                    (width * px).ceil() as i32,
                    (height * py).ceil() as i32,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
        } else {
            unsafe {
                let _ = ShowWindow(self.handle, SW_HIDE);
            }
        }
        if !visible {
            return Ok(false);
        }
        self.surface.draw_christmas(gpu, &Scene { size: input.size, settings: *settings, selected, transports, placing: input.placing }, time)?;
        Ok(true)
    }

    pub fn raise(&self) {
        if self.shown {
            unsafe {
                let above = crate::screen::lowest_open_popup().unwrap_or(HWND_TOPMOST);
                let _ = SetWindowPos(self.window, Some(above), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
        }
    }

    pub fn show_in_screenshots(&self, shown: bool) {
        unsafe {
            let affinity = if shown { WDA_NONE } else { WDA_EXCLUDEFROMCAPTURE };
            let _ = SetWindowDisplayAffinity(self.window, affinity);
            let _ = SetWindowDisplayAffinity(self.handle, affinity);
        }
    }

    pub fn size(&self) -> [f32; 2] {
        self.input.borrow().size
    }

    pub fn hide(&mut self) {
        let mut input = self.input.borrow_mut();
        if input.drag.is_some() {
            input.finish();
        }
        unsafe {
            let _ = ShowWindow(self.window, SW_HIDE);
            let _ = ShowWindow(self.handle, SW_HIDE);
            if input.drag.is_some() {
                let _ = ReleaseCapture();
            }
        }
        input.drag = None;
        self.shown = false;
    }
}

impl Drop for Decorations {
    fn drop(&mut self) {
        unsafe {
            SetWindowLongPtrW(self.handle, GWLP_USERDATA, 0);
            let _ = DestroyWindow(self.handle);
            let _ = DestroyWindow(self.window);
        }
    }
}

unsafe extern "system" fn procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let pointer = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *const RefCell<Input>;
    if message == WM_NCHITTEST {
        return LRESULT(if pointer.is_null() { HTTRANSPARENT as isize } else { HTCLIENT as isize });
    }
    if message == WM_MOUSEACTIVATE {
        return LRESULT(MA_NOACTIVATE as isize);
    }
    if !pointer.is_null() {
        let state = unsafe { &*pointer };
        let Ok(mut input) = state.try_borrow_mut() else {
            return unsafe { DefWindowProcW(window, message, wparam, lparam) };
        };
        match message {
            WM_LBUTTONDOWN if input.placing => {
                if let Some([x, y]) = input.pointer() {
                    let [left, bottom, _, _] = tree_rect(input.size, &input.settings);
                    input.drag = Some([x - left, y - bottom]);
                    unsafe {
                        SetCapture(window);
                    }
                }
                return LRESULT(0);
            }
            WM_MOUSEMOVE if input.drag.is_some() => {
                input.move_to_pointer();
                return LRESULT(0);
            }
            WM_LBUTTONUP if input.drag.is_some() => {
                input.finish();
                return LRESULT(0);
            }
            WM_RBUTTONUP => {
                input.finish();
                return LRESULT(0);
            }
            WM_CAPTURECHANGED if input.drag.is_some() => {
                input.finish();
                return LRESULT(0);
            }
            _ => {}
        }
    }
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}
