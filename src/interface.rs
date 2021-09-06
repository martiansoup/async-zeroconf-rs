use std::{ffi, fmt};

use crate::ZeroconfError;

/// Enum to hold the Interface a service should be advertised on.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum Interface {
    /// Advertise on all interfaces
    Unspecified,
    /// Advertise on specified interface, e.g. as obtained by `if_nametoindex(3)`
    Interface(u32),
}

impl fmt::Display for Interface {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Interface::Unspecified => write!(f, "Any"),
            Interface::Interface(i) => write!(f, "Interface:{}", i),
        }
    }
}

impl Interface {
    /// Create an `Interface` instance representing any interface.
    pub fn new() -> Self {
        Interface::Unspecified
    }

    /// Create an `Interface` instance from an interface name.
    ///
    /// # Examples
    /// ```
    /// # tokio_test::block_on(async {
    /// let interface = async_zeroconf::Interface::from_ifname("lo0")?;
    /// println!("{:?}", interface);
    /// let service_ref = async_zeroconf::Service::new("Server", "_http._tcp", 80)
    ///                       .set_interface(interface)
    ///                       .publish().await?;
    /// # let interface2 = async_zeroconf::Interface::from_ifname("unknown_if");
    /// # assert!(interface2.is_err());
    /// # Ok::<(), async_zeroconf::ZeroconfError>(())
    /// # });
    /// ```
    pub fn from_ifname(name: &str) -> Result<Interface, ZeroconfError> {
        let cname = ffi::CString::new(name)?;
        let name_ptr = cname.as_ptr();
        let index = unsafe { libc::if_nametoindex(name_ptr) };

        if index == 0 {
            Err(ZeroconfError::InterfaceNotFound(name.to_string()))
        } else {
            Ok(Interface::Interface(index))
        }
    }
}

impl Default for Interface {
    fn default() -> Self {
        Interface::new()
    }
}
