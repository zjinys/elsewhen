use anyhow::{Context, Result};
use rdev::{Event, EventType, Key};
use std::{
    process::Command,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const MIN_INTERVAL: Duration = Duration::from_millis(80);
const MAX_INTERVAL: Duration = Duration::from_millis(300);

#[derive(Default)]
struct DoublePressDetector {
    last_release: Option<Instant>,
    ctrl_down: bool,
    chorded: bool,
}

impl DoublePressDetector {
    fn handle(&mut self, event: EventType, now: Instant) -> bool {
        match event {
            EventType::KeyPress(Key::ControlLeft) if !self.ctrl_down => {
                self.ctrl_down = true;
                self.chorded = false;
                false
            }
            EventType::KeyPress(_) if self.ctrl_down => {
                self.chorded = true;
                false
            }
            EventType::KeyRelease(Key::ControlLeft) if self.ctrl_down => {
                self.ctrl_down = false;
                if self.chorded {
                    self.last_release = None;
                    return false;
                }
                let trigger = self
                    .last_release
                    .map(|last| now.duration_since(last))
                    .is_some_and(|gap| gap >= MIN_INTERVAL && gap <= MAX_INTERVAL);
                self.last_release = if trigger { None } else { Some(now) };
                trigger
            }
            _ => false,
        }
    }
}

pub fn run_daemon() -> Result<()> {
    let detector = Arc::new(Mutex::new(DoublePressDetector::default()));
    println!("Elsewhen 正在监听双击 Left Ctrl…");
    rdev::listen(move |Event { event_type, .. }| {
        if detector.lock().expect("hotkey detector poisoned").handle(event_type, Instant::now()) {
            if let Ok(executable) = std::env::current_exe() { let _ = Command::new(executable).arg("capture").spawn(); }
        }
    }).map_err(|error| anyhow::anyhow!("global keyboard listener failed: {error:?}"))
        .context("Linux Wayland may block global keyboard observation; use an X11 session or grant the required input permission")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn triggers_only_in_window() {
        let start = Instant::now();
        let mut d = DoublePressDetector::default();
        assert!(!d.handle(EventType::KeyPress(Key::ControlLeft), start));
        assert!(!d.handle(EventType::KeyRelease(Key::ControlLeft), start));
        assert!(!d.handle(
            EventType::KeyPress(Key::ControlLeft),
            start + Duration::from_millis(150)
        ));
        assert!(d.handle(
            EventType::KeyRelease(Key::ControlLeft),
            start + Duration::from_millis(150)
        ));
    }
    #[test]
    fn rejects_chords() {
        let start = Instant::now();
        let mut d = DoublePressDetector::default();
        d.handle(EventType::KeyPress(Key::ControlLeft), start);
        d.handle(EventType::KeyPress(Key::KeyC), start);
        d.handle(EventType::KeyRelease(Key::ControlLeft), start);
        d.handle(
            EventType::KeyPress(Key::ControlLeft),
            start + Duration::from_millis(150),
        );
        assert!(!d.handle(
            EventType::KeyRelease(Key::ControlLeft),
            start + Duration::from_millis(150)
        ));
    }
}
