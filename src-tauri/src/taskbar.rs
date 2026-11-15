//! Owns the hidden windows that give each separator its taskbar button.
//!
//! Each separator is a top-level window that is fully transparent, minimized and parked
//! off-screen, so the only visible part of it is its taskbar button. Every window gets its
//! own AppUserModelID, which makes Windows give it a separate button (instead of grouping
//! it with the others) that the user can drag to any position and pin. The relaunch
//! properties make a pinned separator keep its icon after the app exits, and clicking it
//! just launches `taskbar-separators.exe --separator <id>`, which exits immediately.
//!
//! All windows live on one dedicated thread with its own message loop; the rest of the
//! app talks to it through a channel.

use std::{
    cell::RefCell,
    collections::HashMap,
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use windows::{
    core::{w, BOOL, GUID, HSTRING, PCWSTR, PWSTR},
    Win32::{
        Foundation::{COLORREF, HWND, LPARAM, LRESULT, PROPERTYKEY, WPARAM},
        Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_DISALLOW_PEEK, DWMWA_EXCLUDED_FROM_PEEK},
        System::{
            Com::{
                CoInitializeEx, CoTaskMemAlloc, StructuredStorage::PROPVARIANT,
                COINIT_APARTMENTTHREADED,
            },
            LibraryLoader::GetModuleHandleW,
            Threading::GetCurrentThreadId,
            Variant::VT_LPWSTR,
        },
        UI::{
            Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow},
            WindowsAndMessaging::*,
        },
    },
};

const CLASS_NAME: PCWSTR = w!("TaskbarSeparatorWindow");
const WM_APP_WAKE: u32 = WM_APP + 1;

/// {9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3} — the System.AppUserModel property set.
const AUM_FMTID: GUID = GUID::from_u128(0x9F4C2855_9F79_4B39_A8D0_E1D42DE1D5F3);
const PKEY_ID: PROPERTYKEY = PROPERTYKEY {
    fmtid: AUM_FMTID,
    pid: 5,
};
const PKEY_RELAUNCH_COMMAND: PROPERTYKEY = PROPERTYKEY {
    fmtid: AUM_FMTID,
    pid: 2,
};
const PKEY_RELAUNCH_ICON: PROPERTYKEY = PROPERTYKEY {
    fmtid: AUM_FMTID,
    pid: 3,
};
const PKEY_RELAUNCH_NAME: PROPERTYKEY = PROPERTYKEY {
    fmtid: AUM_FMTID,
    pid: 4,
};

pub struct Spec {
    pub id: String,
    pub name: String,
    pub icon_path: PathBuf,
    /// Full command line used when a pinned copy is clicked.
    pub relaunch: String,
}

enum Cmd {
    /// Create the window, or update its icon/name if it already exists.
    Show(Spec),
    Hide(String),
}

pub struct Taskbar {
    tx: Sender<Cmd>,
    thread_id: u32,
}

struct Entry {
    hwnd: HWND,
    icons: [HICON; 2],
}

type ClosedCallback = Box<dyn Fn(String)>;

thread_local! {
    static WINDOWS: RefCell<HashMap<String, Entry>> = RefCell::new(HashMap::new());
    static ON_CLOSED: RefCell<Option<ClosedCallback>> = RefCell::new(None);
}

pub fn aumid(id: &str) -> String {
    format!("TaskbarSeparators.Separator.{id}")
}

impl Taskbar {
    /// Starts the window thread. `on_closed` runs (on that thread) when the user closes a
    /// separator from its taskbar right-click menu.
    pub fn start(on_closed: impl Fn(String) + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel::<Cmd>();
        let (ready_tx, ready_rx) = mpsc::channel::<u32>();
        thread::Builder::new()
            .name("taskbar-separators".into())
            .spawn(move || unsafe { run(rx, ready_tx, Box::new(on_closed)) })
            .expect("spawn taskbar thread");
        let thread_id = ready_rx.recv().expect("taskbar thread failed to start");
        Taskbar { tx, thread_id }
    }

    pub fn show(&self, spec: Spec) {
        self.send(Cmd::Show(spec));
    }

    pub fn hide(&self, id: &str) {
        self.send(Cmd::Hide(id.to_string()));
    }

    fn send(&self, cmd: Cmd) {
        if self.tx.send(cmd).is_ok() {
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, WM_APP_WAKE, WPARAM(0), LPARAM(0));
            }
        }
    }
}

unsafe fn run(rx: Receiver<Cmd>, ready: Sender<u32>, on_closed: Box<dyn Fn(String)>) {
    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    // Force creation of this thread's message queue before anyone posts to it.
    let mut msg = MSG::default();
    let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);

    let hinstance = GetModuleHandleW(None).unwrap_or_default();
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(wndproc),
        hInstance: hinstance.into(),
        lpszClassName: CLASS_NAME,
        ..Default::default()
    };
    RegisterClassExW(&class);
    ON_CLOSED.with(|c| *c.borrow_mut() = Some(on_closed));
    let _ = ready.send(GetCurrentThreadId());

    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
        if msg.hwnd.is_invalid() && msg.message == WM_APP_WAKE {
            while let Ok(cmd) = rx.try_recv() {
                match cmd {
                    Cmd::Show(spec) => {
                        if let Err(e) = show(&spec) {
                            eprintln!("could not show separator {}: {e}", spec.id);
                        }
                    }
                    Cmd::Hide(id) => {
                        if let Some(entry) = WINDOWS.with(|w| w.borrow_mut().remove(&id)) {
                            destroy(entry);
                        }
                    }
                }
            }
            continue;
        }
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}

unsafe fn show(spec: &Spec) -> windows::core::Result<()> {
    let existing = WINDOWS.with(|w| w.borrow().get(&spec.id).map(|e| e.hwnd));
    if let Some(hwnd) = existing {
        // Already in the taskbar: refresh icon, tooltip and pin metadata in place, so it
        // keeps whatever position the user dragged it to.
        let _ = SetWindowTextW(hwnd, &HSTRING::from(&spec.name));
        set_relaunch_props(hwnd, spec)?;
        let icons = load_icons(spec);
        apply_icons(hwnd, icons);
        WINDOWS.with(|w| {
            if let Some(e) = w.borrow_mut().get_mut(&spec.id) {
                for old in std::mem::replace(&mut e.icons, icons) {
                    if !old.is_invalid() {
                        let _ = DestroyIcon(old);
                    }
                }
            }
        });
        return Ok(());
    }

    let hinstance = GetModuleHandleW(None)?;
    let hwnd = CreateWindowExW(
        WS_EX_APPWINDOW | WS_EX_LAYERED | WS_EX_NOACTIVATE,
        CLASS_NAME,
        &HSTRING::from(&spec.name),
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
        -32000,
        -32000,
        1,
        1,
        None,
        None,
        Some(hinstance.into()),
        None,
    )?;

    // Identity must be set before the window is first shown, or it groups with the app.
    let store: IPropertyStore = SHGetPropertyStoreForWindow(hwnd)?;
    store.SetValue(&PKEY_ID, &lpwstr(&aumid(&spec.id)))?;
    store.Commit()?;
    set_relaunch_props(hwnd, spec)?;

    // Fully transparent and excluded from Aero Peek, so it never shows anything but the button.
    SetLayeredWindowAttributes(hwnd, COLORREF(0), 0, LWA_ALPHA)?;
    let yes = BOOL(1);
    for attr in [DWMWA_DISALLOW_PEEK, DWMWA_EXCLUDED_FROM_PEEK] {
        let _ = DwmSetWindowAttribute(
            hwnd,
            attr,
            &yes as *const _ as _,
            std::mem::size_of::<BOOL>() as u32,
        );
    }

    let icons = load_icons(spec);
    apply_icons(hwnd, icons);
    let _ = ShowWindow(hwnd, SW_SHOWMINNOACTIVE);
    WINDOWS.with(|w| {
        w.borrow_mut()
            .insert(spec.id.clone(), Entry { hwnd, icons })
    });
    Ok(())
}

unsafe fn set_relaunch_props(hwnd: HWND, spec: &Spec) -> windows::core::Result<()> {
    let store: IPropertyStore = SHGetPropertyStoreForWindow(hwnd)?;
    store.SetValue(&PKEY_RELAUNCH_COMMAND, &lpwstr(&spec.relaunch))?;
    store.SetValue(&PKEY_RELAUNCH_NAME, &lpwstr(&spec.name))?;
    store.SetValue(
        &PKEY_RELAUNCH_ICON,
        &lpwstr(&format!("{},0", spec.icon_path.display())),
    )?;
    store.Commit()
}

unsafe fn load_icons(spec: &Spec) -> [HICON; 2] {
    let path = HSTRING::from(spec.icon_path.as_os_str());
    let load = |cx| {
        LoadImageW(None, &path, IMAGE_ICON, cx, cx, LR_LOADFROMFILE)
            .map(|h| HICON(h.0))
            .unwrap_or_default()
    };
    [
        load(GetSystemMetrics(SM_CXICON)),
        load(GetSystemMetrics(SM_CXSMICON)),
    ]
}

unsafe fn apply_icons(hwnd: HWND, [big, small]: [HICON; 2]) {
    SendMessageW(
        hwnd,
        WM_SETICON,
        Some(WPARAM(ICON_BIG as usize)),
        Some(LPARAM(big.0 as isize)),
    );
    SendMessageW(
        hwnd,
        WM_SETICON,
        Some(WPARAM(ICON_SMALL as usize)),
        Some(LPARAM(small.0 as isize)),
    );
}

unsafe fn destroy(entry: Entry) {
    // Windows asks for the AppUserModel properties to be cleared before the window goes away.
    if let Ok(store) = SHGetPropertyStoreForWindow::<IPropertyStore>(entry.hwnd) {
        let empty = PROPVARIANT::default();
        for key in [
            PKEY_RELAUNCH_COMMAND,
            PKEY_RELAUNCH_NAME,
            PKEY_RELAUNCH_ICON,
            PKEY_ID,
        ] {
            let _ = store.SetValue(&key, &empty);
        }
        let _ = store.Commit();
    }
    let _ = DestroyWindow(entry.hwnd);
    for icon in entry.icons {
        if !icon.is_invalid() {
            let _ = DestroyIcon(icon);
        }
    }
}

/// A VT_LPWSTR PROPVARIANT (the type the AppUserModel properties require; the
/// `From<&str>` impl in the windows crate makes a VT_BSTR).
unsafe fn lpwstr(s: &str) -> PROPVARIANT {
    let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    let mem = CoTaskMemAlloc(wide.len() * 2) as *mut u16;
    let mut pv = PROPVARIANT::default();
    if mem.is_null() {
        return pv;
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr(), mem, wide.len());
    let inner = &mut pv.Anonymous.Anonymous;
    inner.vt = VT_LPWSTR;
    inner.Anonymous.pwszVal = PWSTR(mem);
    pv // dropping it later calls PropVariantClear, which frees `mem`
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        // Clicking the taskbar button tries to restore the window: refuse, stay minimized.
        WM_SYSCOMMAND => {
            let cmd = (wparam.0 & 0xFFF0) as u32;
            if cmd == SC_RESTORE || cmd == SC_MAXIMIZE {
                return LRESULT(0);
            }
        }
        // (Only once shown: CreateWindowExW sends a WM_SIZE before the identity is set.)
        WM_SIZE if wparam.0 as u32 != SIZE_MINIMIZED && IsWindowVisible(hwnd).as_bool() => {
            let _ = ShowWindow(hwnd, SW_SHOWMINNOACTIVE);
            return LRESULT(0);
        }
        // "Close window" from the taskbar right-click menu.
        WM_CLOSE => {
            let id = WINDOWS.with(|w| {
                let mut w = w.borrow_mut();
                let id = w
                    .iter()
                    .find(|(_, e)| e.hwnd == hwnd)
                    .map(|(id, _)| id.clone());
                id.and_then(|id| w.remove(&id).map(|e| (id, e)))
            });
            match id {
                Some((id, entry)) => {
                    destroy(entry);
                    ON_CLOSED.with(|c| {
                        if let Some(cb) = c.borrow().as_ref() {
                            cb(id);
                        }
                    });
                }
                None => {
                    let _ = DestroyWindow(hwnd);
                }
            }
            return LRESULT(0);
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}
