//! A dedicated Win32 message-loop thread.
//!
//! Tray icons, global hotkeys and WinEvent hooks all deliver their notifications through the
//! message queue of the thread that created them. Hosting them on our own thread keeps them
//! independent of the UI framework's event loop.

use std::sync::mpsc;
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, EVENT_OBJECT_SHOW, EVENT_SYSTEM_FOREGROUND, GetMessageW, MSG, OBJID_WINDOW,
    PM_NOREMOVE, PeekMessageW, PostThreadMessageW, TranslateMessage, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS, WM_APP, WM_QUIT,
};

use crate::window::{WindowHandle, process_id};

const WM_RUN_TASKS: u32 = WM_APP + 1;

type Task = Box<dyn FnOnce() + Send>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinEvent {
    /// A window became the foreground window.
    Foreground { hwnd: WindowHandle, pid: u32 },
    /// A top-level window was shown.
    Shown { hwnd: WindowHandle, pid: u32 },
}

type EventSink = Arc<dyn Fn(WinEvent) + Send + Sync>;
static EVENT_SINK: OnceLock<EventSink> = OnceLock::new();

/// Handle to the message thread. Dropping it does not stop the thread; call [`MessageThread::quit`].
#[derive(Clone)]
pub struct MessageThread {
    thread_id: u32,
    tasks: mpsc::Sender<Task>,
}

impl MessageThread {
    /// Spawn the thread. `setup` runs on it before the loop starts; whatever it returns is kept
    /// alive (on that thread) until the loop exits — put tray icons and hotkey managers there.
    pub fn spawn<T: 'static>(setup: impl FnOnce() -> T + Send + 'static) -> (Self, JoinHandle<()>) {
        let (id_tx, id_rx) = mpsc::channel();
        let (task_tx, task_rx) = mpsc::channel::<Task>();
        let join = std::thread::Builder::new()
            .name("cm-msg".into())
            .spawn(move || {
                let mut msg = MSG::default();
                // Force creation of the message queue before anyone can post to it.
                unsafe {
                    let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
                }
                let _ = id_tx.send(unsafe { GetCurrentThreadId() });
                let _keep = setup();
                loop {
                    let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
                    if r.0 <= 0 {
                        break;
                    }
                    if msg.hwnd.is_invalid() && msg.message == WM_RUN_TASKS {
                        while let Ok(task) = task_rx.try_recv() {
                            task();
                        }
                        continue;
                    }
                    unsafe {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
            })
            .expect("spawn message thread");
        let thread_id = id_rx.recv().expect("message thread started");
        (
            Self {
                thread_id,
                tasks: task_tx,
            },
            join,
        )
    }

    /// Run `f` on the message thread.
    pub fn run(&self, f: impl FnOnce() + Send + 'static) {
        if self.tasks.send(Box::new(f)).is_ok() {
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, WM_RUN_TASKS, WPARAM(0), LPARAM(0));
            }
        }
    }

    pub fn quit(&self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
}

/// Install foreground/show hooks. Must be called on a thread with a message loop (i.e. inside
/// [`MessageThread::spawn`]'s `setup`). The returned guard unhooks on drop.
pub fn install_win_event_hooks(sink: impl Fn(WinEvent) + Send + Sync + 'static) -> WinEventHooks {
    let _ = EVENT_SINK.set(Arc::new(sink));
    let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
    unsafe {
        let fg = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(on_event),
            0,
            0,
            flags,
        );
        let show = SetWinEventHook(
            EVENT_OBJECT_SHOW,
            EVENT_OBJECT_SHOW,
            None,
            Some(on_event),
            0,
            0,
            flags,
        );
        WinEventHooks(vec![fg, show])
    }
}

pub struct WinEventHooks(Vec<HWINEVENTHOOK>);
impl Drop for WinEventHooks {
    fn drop(&mut self) {
        for h in &self.0 {
            unsafe {
                let _ = UnhookWinEvent(*h);
            }
        }
    }
}

unsafe extern "system" fn on_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if hwnd.is_invalid() || id_object != OBJID_WINDOW.0 || id_child != 0 {
        return;
    }
    let Some(sink) = EVENT_SINK.get() else { return };
    let h = WindowHandle::from_hwnd(hwnd);
    let pid = process_id(h);
    match event {
        EVENT_SYSTEM_FOREGROUND => sink(WinEvent::Foreground { hwnd: h, pid }),
        EVENT_OBJECT_SHOW => sink(WinEvent::Shown { hwnd: h, pid }),
        _ => {}
    }
}
