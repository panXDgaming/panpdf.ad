use std::sync::OnceLock;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub struct HttpCall<'a> {
    pub method: &'a str,
    pub url: &'a str,
    pub headers: &'a [String],
    pub body: Option<&'a [u8]>,
    pub idle: Duration,
    pub cap: Duration,
    pub cancel: &'a AtomicBool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HttpAnswer {
    pub status: u16,
    pub headers: String,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransportError {
    Cancelled,
    TimedOut,
    Unreachable(String),
}

pub type Transport = fn(&HttpCall<'_>) -> Result<HttpAnswer, TransportError>;

static TRANSPORT: OnceLock<Transport> = OnceLock::new();

pub fn send_with(transport: Transport) {
    let _ = TRANSPORT.set(transport);
}

#[cfg(test)]
thread_local! {
    static TRIAL: std::cell::Cell<Option<Transport>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(crate) fn try_with(transport: Option<Transport>) {
    TRIAL.with(|trial| trial.set(transport));
}

pub(crate) fn registered() -> Option<Transport> {
    #[cfg(test)]
    if let Some(trial) = TRIAL.with(std::cell::Cell::get) {
        return Some(trial);
    }
    TRANSPORT.get().copied()
}
