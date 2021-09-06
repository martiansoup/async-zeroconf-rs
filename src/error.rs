use std::ffi::NulError;
use std::fmt;

use crate::Service;
use bonjour_sys::DNSServiceErrorType;
use std::error::Error;
use std::str::Utf8Error;
use std::sync::PoisonError;

/// Custom error holding any potential errors from publishing or browsing for
/// services.
#[derive(Debug)]
pub enum ZeroconfError {
    /// An error from the Bonjour API ([BonjourError])
    Bonjour(BonjourError),
    /// An IO error on the internal socket used to poll for events
    Io(std::io::Error),
    /// A timeout occurred before the operation was complete
    ///
    /// Contains the service that could not be resolved. This is only valid
    /// for resolve operations as browsing does not have a specific endpoint
    /// and so timing out is not an error.
    Timeout(Service),
    /// The service type specified is invalid
    InvalidServiceType(String),
    /// The TXT record specified is invalid
    InvalidTxtRecord(String),
    /// A service was passed to resolve that was not from a
    /// [ServiceBrowser][crate::ServiceBrowser].
    ///
    /// This is an error as the resolve operation requires the information
    /// about domain, interface and so on to be in the format provided from
    /// the browse operation.
    NotFromBrowser(Service),
    /// Null byte in a string conversion
    NullString(NulError),
    /// Poisoned Mutex
    Poison,
    /// Failed to convert to a UTF-8 string
    Utf8(Utf8Error),
    /// Interface not found
    InterfaceNotFound(String),
    /// Dropped a task
    Dropped,
}

impl From<PoisonError<std::sync::MutexGuard<'_, ()>>> for ZeroconfError {
    fn from(_: PoisonError<std::sync::MutexGuard<'_, ()>>) -> Self {
        ZeroconfError::Poison
    }
}

impl From<NulError> for ZeroconfError {
    fn from(s: NulError) -> Self {
        ZeroconfError::NullString(s)
    }
}

impl From<Utf8Error> for ZeroconfError {
    fn from(s: Utf8Error) -> Self {
        ZeroconfError::Utf8(s)
    }
}

impl From<std::io::Error> for ZeroconfError {
    fn from(s: std::io::Error) -> Self {
        ZeroconfError::Io(s)
    }
}

impl From<i32> for ZeroconfError {
    fn from(s: i32) -> Self {
        ZeroconfError::Bonjour(s.into())
    }
}

impl From<BonjourError> for ZeroconfError {
    fn from(s: BonjourError) -> Self {
        ZeroconfError::Bonjour(s)
    }
}

impl fmt::Display for ZeroconfError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match self {
            ZeroconfError::Bonjour(e) => format!("error from bonjour - {}", e),
            ZeroconfError::Io(e) => e.to_string(),
            ZeroconfError::Timeout(s) => format!("timeout on {}", s.service_type()),
            ZeroconfError::InvalidServiceType(s) => format!("invalid service type '{}'", s),
            ZeroconfError::InvalidTxtRecord(s) => format!("invalid txt record '{}'", s),
            ZeroconfError::NotFromBrowser(s) => {
                format!("'{}' service not from browser", s.service_type())
            }
            ZeroconfError::NullString(s) => s.to_string(),
            ZeroconfError::Poison => "mutex was poisoned".to_string(),
            ZeroconfError::Utf8(e) => e.to_string(),
            ZeroconfError::InterfaceNotFound(s) => format!("interface not found '{}'", s),
            ZeroconfError::Dropped => "task dropped before expected".to_string(),
        };
        write!(f, "{}", s)
    }
}

impl Error for ZeroconfError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ZeroconfError::Bonjour(e) => Some(e),
            ZeroconfError::Io(e) => Some(e),
            ZeroconfError::NullString(e) => Some(e),
            ZeroconfError::Utf8(e) => Some(e),
            _ => None,
        }
    }
}

/// An error from the Bonjour API
///
/// Further information about the requirements for service parameters can be
/// found in the [Bonjour API][b] documentation.
///
/// [b]: https://developer.apple.com/documentation/dnssd/dns_service_discovery_c
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum BonjourError {
    /// Unknown error
    Unknown,
    /// No such name
    NoSuchName,
    /// Out of memory
    NoMemory,
    /// Bad parameter passed to function
    BadParam,
    /// Bad reference
    BadReference,
    /// Bad state
    BadState,
    /// Unexpected flags to function
    BadFlags,
    /// Unsupported
    Unsupported,
    /// Not initialized
    NotInitialized,
    /// Already registered
    AlreadyRegistered,
    /// Name conflicts with existing service
    NameConflict,
    /// Invalid index or character
    Invalid,
    /// Firewall
    Firewall,
    /// Client library incompatible with daemon
    Incompatible,
    /// Interface index doesn't exist
    BadInterfaceIndex,
    /// Refused
    Refused,
    /// No such record
    NoSuchRecord,
    /// No auth
    NoAuth,
    /// Key does not exist in TXT record
    NoSuchKey,
    /// NAT traversal
    NATTraversal,
    /// More than one NAT gateway between source and destination
    DoubleNAT,
    /// Bad time
    BadTime,
    /// Undefined error
    Undefined,
}

impl fmt::Display for BonjourError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match self {
            BonjourError::Unknown => "unknown error",
            BonjourError::NoSuchName => "no such name",
            BonjourError::NoMemory => "no memory",
            BonjourError::BadParam => "bad parameter",
            BonjourError::BadReference => "bad reference",
            BonjourError::BadState => "bad state",
            BonjourError::BadFlags => "bad flags",
            BonjourError::Unsupported => "unsupported",
            BonjourError::NotInitialized => "not initialized",
            BonjourError::AlreadyRegistered => "already registered",
            BonjourError::NameConflict => "name conflict",
            BonjourError::Invalid => "invalid",
            BonjourError::Firewall => "firewall",
            BonjourError::Incompatible => "incompatible",
            BonjourError::BadInterfaceIndex => "bad interface index",
            BonjourError::Refused => "refused",
            BonjourError::NoSuchRecord => "no such record",
            BonjourError::NoAuth => "no auth",
            BonjourError::NoSuchKey => "no such key",
            BonjourError::NATTraversal => "NAT traversal",
            BonjourError::DoubleNAT => "double NAT",
            BonjourError::BadTime => "bad time",
            BonjourError::Undefined => "undefined error",
        };
        write!(f, "{}", s)
    }
}

impl std::error::Error for BonjourError {}

impl From<DNSServiceErrorType> for BonjourError {
    fn from(err: DNSServiceErrorType) -> Self {
        match err {
            -65537 => BonjourError::Unknown,
            -65538 => BonjourError::NoSuchName,
            -65539 => BonjourError::NoMemory,
            -65540 => BonjourError::BadParam,
            -65541 => BonjourError::BadReference,
            -65542 => BonjourError::BadState,
            -65543 => BonjourError::BadFlags,
            -65544 => BonjourError::Unsupported,
            -65545 => BonjourError::NotInitialized,
            -65547 => BonjourError::AlreadyRegistered,
            -65548 => BonjourError::NameConflict,
            -65549 => BonjourError::Invalid,
            -65550 => BonjourError::Firewall,
            -65551 => BonjourError::Incompatible,
            -65552 => BonjourError::BadInterfaceIndex,
            -65553 => BonjourError::Refused,
            -65554 => BonjourError::NoSuchRecord,
            -65555 => BonjourError::NoAuth,
            -65556 => BonjourError::NoSuchKey,
            -65557 => BonjourError::NATTraversal,
            -65558 => BonjourError::DoubleNAT,
            -65559 => BonjourError::BadTime,
            _ => BonjourError::Undefined,
        }
    }
}
