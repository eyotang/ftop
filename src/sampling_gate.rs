//! Sleep the sampler until native visibility allows work. X11 compositors may keep
//! iconic windows mapped, so the Linux adapter also watches WM_STATE properties.
use gpui_kit::Window;
use std::sync::{Arc, Condvar, Mutex};
#[derive(Clone, Copy)]
struct State {
    visible: bool,
    minimized: bool,
}
pub struct SamplingGate {
    state: Mutex<State>,
    wake: Condvar,
}
impl SamplingGate {
    pub fn new(window: &Window) -> Arc<Self> {
        let gate = Arc::new(Self {
            state: Mutex::new(State {
                visible: window.is_visible(),
                minimized: false,
            }),
            wake: Condvar::new(),
        });
        #[cfg(target_os = "linux")]
        watch_x11(window, gate.clone());
        gate
    }
    pub fn set_visible(&self, visible: bool) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).visible = visible;
        self.wake.notify_one();
    }
    #[cfg(target_os = "linux")]
    fn set_minimized(&self, minimized: bool) {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .minimized = minimized;
        self.wake.notify_one();
    }
    pub fn wait(&self) {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        drop(
            self.wake
                .wait_while(state, |s| !s.visible || s.minimized)
                .unwrap_or_else(|e| e.into_inner()),
        );
    }
}
#[cfg(target_os = "linux")]
fn watch_x11(window: &Window, gate: Arc<SamplingGate>) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use x11rb::{
        connection::Connection,
        protocol::{
            Event,
            xproto::{AtomEnum, ChangeWindowAttributesAux, ConnectionExt, EventMask},
        },
    };
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let id = match handle.as_raw() {
        RawWindowHandle::Xcb(handle) => handle.window.get(),
        RawWindowHandle::Xlib(handle) => handle.window as u32,
        _ => return,
    };
    let _ = std::thread::Builder::new()
        .name("ftop-visibility".into())
        .spawn(move || {
            let run = || -> Result<(), Box<dyn std::error::Error>> {
                let (connection, _) = x11rb::connect(None)?;
                let state = connection
                    .intern_atom(false, b"_NET_WM_STATE")?
                    .reply()?
                    .atom;
                let hidden = connection
                    .intern_atom(false, b"_NET_WM_STATE_HIDDEN")?
                    .reply()?
                    .atom;
                let wm_state = connection.intern_atom(false, b"WM_STATE")?.reply()?.atom;
                connection
                    .change_window_attributes(
                        id,
                        &ChangeWindowAttributesAux::new()
                            .event_mask(EventMask::PROPERTY_CHANGE | EventMask::STRUCTURE_NOTIFY),
                    )?
                    .check()?;
                connection.flush()?;
                let refresh = || -> Result<(), Box<dyn std::error::Error>> {
                    let net = connection
                        .get_property(false, id, state, AtomEnum::ATOM, 0, 64)?
                        .reply()?;
                    let iconic = connection
                        .get_property(false, id, wm_state, AtomEnum::ANY, 0, 2)?
                        .reply()?;
                    let minimized = net
                        .value32()
                        .is_some_and(|mut values| values.any(|value| value == hidden))
                        || iconic.value32().and_then(|mut values| values.next()) == Some(3);
                    gate.set_minimized(minimized);
                    Ok(())
                };
                refresh()?;
                loop {
                    match connection.wait_for_event()? {
                        Event::PropertyNotify(event)
                            if event.atom == state || event.atom == wm_state =>
                        {
                            refresh()?
                        }
                        Event::MapNotify(_) | Event::UnmapNotify(_) => refresh()?,
                        Event::DestroyNotify(_) => break,
                        _ => {}
                    }
                }
                Ok(())
            };
            if let Err(error) = run() {
                eprintln!("ftop: X11 visibility adapter unavailable: {error}");
            }
        });
}
#[cfg(test)]
mod tests {
    use super::{SamplingGate, State};
    use std::sync::{Arc, Condvar, Mutex, mpsc};
    use std::time::Duration;
    #[test]
    fn hidden_gate_blocks_until_restore() {
        let gate = Arc::new(SamplingGate {
            state: Mutex::new(State {
                visible: false,
                minimized: false,
            }),
            wake: Condvar::new(),
        });
        let worker = gate.clone();
        let (sender, receiver) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            worker.wait();
            sender.send(()).unwrap();
        });
        assert!(receiver.recv_timeout(Duration::from_millis(30)).is_err());
        gate.set_visible(true);
        receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        thread.join().unwrap();
    }
}
