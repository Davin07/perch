use std::fmt;

#[derive(Debug)]
pub enum Error {
    /// The device is not plugged in (or not in the expected mode).
    NotFound(&'static str),
    /// A Win32 call failed.
    Win32(&'static str, u32),
    /// The device answered, but not with success.
    Status(u8),
    /// The feature exists but this model does not implement it.
    Unsupported(String),
    /// A value outside what the device accepts.
    Invalid(String),
    /// Windows-side setup failed (COM, DirectShow).
    System(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotFound(what) => write!(f, "{what} not found (is it plugged in?)"),
            Error::Win32(call, code) => write!(f, "{call} failed (Win32 error {code})"),
            Error::Status(0x01) => write!(f, "device busy"),
            Error::Status(0x03) => write!(f, "device rejected the request"),
            Error::Status(0x04) => write!(f, "device timed out (asleep?)"),
            Error::Status(0x05) => write!(f, "device says this feature is unsupported"),
            Error::Status(s) => write!(f, "device returned status {s:#04x}"),
            Error::Unsupported(why) => write!(f, "{why}"),
            Error::Invalid(why) => write!(f, "{why}"),
            Error::System(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for Error {}
