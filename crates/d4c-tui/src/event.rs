use crossterm::event::{self, Event, KeyEvent};
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum AppEvent {
    KeyInput(KeyEvent),
    Resize(u16, u16),
}

pub enum EventResult {
    Event(AppEvent),
    Timeout,
}

/// Poll duration. 50 ms (down from 100) keeps streamed token arrivals
/// feeling live without burning measurable CPU.
pub fn poll_event(timeout: Duration) -> Result<EventResult, anyhow::Error> {
    if event::poll(timeout)? {
        match event::read()? {
            Event::Key(key) => Ok(EventResult::Event(AppEvent::KeyInput(key))),
            Event::Resize(w, h) => Ok(EventResult::Event(AppEvent::Resize(w, h))),
            _ => Ok(EventResult::Timeout),
        }
    } else {
        Ok(EventResult::Timeout)
    }
}
