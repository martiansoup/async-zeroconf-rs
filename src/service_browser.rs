use crate::{
    BonjourError, Interface, OpKind, OpType, ProcessTask, Service, ServiceRef, ServiceRefWrapper,
    ServiceResolver, ZeroconfError,
};

use core::pin::Pin;
use core::task::{Context, Poll};
use futures::stream::StreamExt;
use futures_core::Stream;
use std::ffi;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use bonjour_sys::{DNSServiceErrorType, DNSServiceFlags, DNSServiceRef};

/// `ServiceBrowserBuilder` is used to browse for services. Once all the
/// required information is added to the `ServiceBrowserBuilder` the
/// [`browse`][`ServiceBrowserBuilder::browse`] method will produce a
/// [`ServiceBrowser`] which can be used as a stream, or the
/// [`ServiceBrowser::recv`] method will produce the next service found.
///
/// # Note
/// This does not resolve the services so does not contain all information
/// associated with the service. A further resolve operation is required to
/// fully populate the service. This can be done with a [`ServiceResolver`].
/// Alternatively, the [`ServiceBrowser::recv_resolve`] method can be
/// used to resolve the services inline, or [`ServiceBrowser::resolving`] used
/// to convert the stream into one that resolves services before returning
/// them.
///
/// # Examples
/// ```
/// # tokio_test::block_on(async {
/// let mut browser = async_zeroconf::ServiceBrowserBuilder::new("_http._tcp");
/// let mut services = browser
///     .timeout(tokio::time::Duration::from_secs(2))
///     .browse()?;
///
/// while let Some(v) = services.recv().await {
///     println!("Service = {:?}", v);
/// }
/// # Ok::<(), async_zeroconf::ZeroconfError>(())
/// # });
/// ```
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct ServiceBrowserBuilder {
    interface: Interface,
    service_type: String,
    domain: Option<String>,
    timeout: Option<Duration>,
    close_on_end: bool,
}

/// Struct used to get the results of a service browser which should be
/// constructed with a [`ServiceBrowserBuilder`].
#[derive(Debug)]
pub struct ServiceBrowser {
    // Channel to receive found services
    rx: mpsc::UnboundedReceiver<(Result<Service, ZeroconfError>, bool)>,
    // Reference to the socket used to process events
    delegate: ServiceRef,
    // Close if no more events
    close_on_end: bool,
}

impl Stream for ServiceBrowser {
    type Item = Result<Service, ZeroconfError>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<<Self as futures_core::Stream>::Item>> {
        self.rx.poll_recv(cx).map(|p| {
            p.map(|s| {
                if s.1 {
                    self.close()
                };
                s.0
            })
        })
    }
}

impl ServiceBrowser {
    // Close the underlying receiver
    fn close(&mut self) {
        if self.close_on_end {
            log::debug!("Got end of events ({})", self.delegate.op_type());
            self.rx.close();
        }
    }

    /// Receive a service from the browser.
    ///
    /// A response of `None` indicates that the browse operation has
    /// finished, for example due to a timeout or error.
    ///
    /// # Examples
    /// ```
    /// # tokio_test::block_on(async {
    /// let mut browser = async_zeroconf::ServiceBrowserBuilder::new("_http._tcp");
    /// let mut services = browser
    ///     .timeout(tokio::time::Duration::from_secs(2))
    ///     .browse()?;
    ///
    /// while let Some(v) = services.recv().await {
    ///     println!("Service = {:?}", v);
    /// }
    /// # Ok::<(), async_zeroconf::ZeroconfError>(())
    /// # });
    /// ```
    pub async fn recv(&mut self) -> Option<Result<Service, ZeroconfError>> {
        self.rx.recv().await.map(|s| {
            if s.1 {
                self.close()
            };
            s.0
        })
    }

    /// Receive a service from the browser, resolving it before returning it
    ///
    /// A response of `None` indicates that the browse operation has
    /// finished, for example due to a timeout or error. If the resolve
    /// operation fails the error will be contained in the inner `Result`.
    ///
    /// # Examples
    /// ```
    /// # tokio_test::block_on(async {
    /// let mut browser = async_zeroconf::ServiceBrowserBuilder::new("_http._tcp");
    /// let mut services = browser
    ///     .timeout(tokio::time::Duration::from_secs(2))
    ///     .browse()?;
    ///
    /// while let Some(Ok(v)) = services.recv_resolve().await {
    ///     println!("Resolved Service = {:?}", v);
    /// }
    /// # Ok::<(), async_zeroconf::ZeroconfError>(())
    /// # });
    /// ```
    pub async fn recv_resolve(&mut self) -> Option<Result<Service, ZeroconfError>> {
        match self.recv().await {
            Some(Ok(service)) => Some(ServiceResolver::r(&service).await),
            Some(Err(e)) => Some(Err(e)),
            None => None,
        }
    }

    /// Return a stream that includes the resolve operation before returning
    /// results. The [`ServiceBrowser`] is consumed to produce the new stream.
    ///
    /// The values produced by the stream are equivalent to those produced by
    /// [`recv_resolve`][`ServiceBrowser::recv_resolve`].
    ///
    /// # Examples
    /// ```
    /// use tokio_stream::StreamExt;
    /// # tokio_test::block_on(async {
    /// let mut browser = async_zeroconf::ServiceBrowserBuilder::new("_http._tcp");
    /// let mut services = browser
    ///     .timeout(tokio::time::Duration::from_secs(2))
    ///     .browse()?;
    ///
    /// let mut stream = services.resolving();
    /// while let Some(Ok(v)) = stream.next().await {
    ///     println!("Resolved Service = {:?}", v);
    /// }
    /// # Ok::<(), async_zeroconf::ZeroconfError>(())
    /// # });
    /// ```
    pub fn resolving(self) -> impl Stream<Item = Result<Service, ZeroconfError>> + Unpin {
        Box::pin(self.then(|service| async move {
            match service {
                Ok(s) => ServiceResolver::r(&s).await,
                Err(e) => Err(e),
            }
        }))
    }
}

#[derive(Debug)]
struct ServiceBrowserContext {
    tx: mpsc::UnboundedSender<(Result<Service, ZeroconfError>, bool)>,
}

impl ServiceBrowserContext {
    fn send(&self, result: Result<Service, ZeroconfError>, last: bool) {
        if let Err(e) = self.tx.send((result, last)) {
            log::warn!("Failed to send Service, receiver dropped: {}", e);
        }
    }
}

impl Drop for ServiceBrowserContext {
    fn drop(&mut self) {
        log::trace!("Dropping ServiceBrowserContext");
    }
}

unsafe fn browse_callback_inner(
    intf_index: u32,
    name: *const libc::c_char,
    regtype: *const libc::c_char,
    domain: *const libc::c_char,
) -> Result<Service, ZeroconfError> {
    let c_name = ffi::CStr::from_ptr(name);
    let c_type = ffi::CStr::from_ptr(regtype);
    let c_domain = ffi::CStr::from_ptr(domain);
    let name = c_name.to_str()?;
    let regtype = c_type.to_str()?;
    let domain = c_domain.to_str()?;

    log::debug!(
        "ServiceBrowse Callback OK ({}:{}:{})",
        name,
        regtype,
        domain
    );
    let mut service = Service::new(name, regtype, 0);
    service
        .set_interface(Interface::Interface(intf_index))
        .set_domain(domain.to_string())
        .set_browse();
    Ok(service)
}

// Callback passed to DNSServiceBrowse
unsafe extern "C" fn browse_callback(
    _sd_ref: DNSServiceRef,
    flags: DNSServiceFlags,
    intf_index: u32,
    error: DNSServiceErrorType,
    name: *const libc::c_char,
    regtype: *const libc::c_char,
    domain: *const libc::c_char,
    context: *mut libc::c_void,
) {
    let proxy = &*(context as *const ServiceBrowserContext);
    if error == 0 {
        let more = (flags & 0x1) == 0x1;
        let add = (flags & 0x2) == 0x2;

        if add {
            let service = browse_callback_inner(intf_index, name, regtype, domain);
            if !more {
                log::trace!("End of services (for now)");
            }

            proxy.send(service, !more);
        } else {
            let c_name = ffi::CStr::from_ptr(name);
            if let Ok(s) = c_name.to_str() {
                log::debug!("ServiceBrowse Remove {}", s);
            }
        }
    } else {
        proxy.send(Err(error.into()), false);

        log::error!(
            "ServiceBrowse Callback Error ({}:{})",
            error,
            Into::<BonjourError>::into(error)
        )
    }
}

impl ServiceBrowserBuilder {
    /// Create a new `ServiceBrowserBuilder` for the specified service type
    pub fn new(service_type: &str) -> Self {
        ServiceBrowserBuilder {
            interface: Default::default(),
            service_type: service_type.to_string(),
            domain: None,
            timeout: None,
            close_on_end: false,
        }
    }

    /// Set the timeout
    pub fn timeout(&mut self, timeout: Duration) -> &mut Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set the browser to close if no more [`Service`]s are found.
    ///
    /// # Note
    /// The browser can only detect the end of the [`Service`]s if
    /// any are found. A timeout can be used in combination with closing on
    /// end to ensure that the browser will terminate.
    pub fn close_on_end(&mut self) -> &mut Self {
        self.close_on_end = true;
        self
    }

    /// Set the interface for service discovery rather than all
    pub fn interface(&mut self, interface: Interface) -> &mut Self {
        self.interface = interface;
        self
    }

    /// Set the domain for service discovery rather than all
    pub fn domain(&mut self, domain: String) -> &mut Self {
        self.domain = Some(domain);
        self
    }

    /// Start the browsing operation, which will continue until the specified
    /// timeout or until the [`ServiceBrowser`] is dropped.
    ///
    /// # Examples
    /// ```
    /// # tokio_test::block_on(async {
    /// let mut browser = async_zeroconf::ServiceBrowserBuilder::new("_http._tcp");
    /// let mut services = browser
    ///     .timeout(tokio::time::Duration::from_secs(2))
    ///     .browse()?;
    ///
    /// while let Some(Ok(v)) = services.recv().await {
    ///     println!("Service = {:?}", v);
    /// }
    /// # Ok::<(), async_zeroconf::ZeroconfError>(())
    /// # });
    /// ```
    pub fn browse(&self) -> Result<ServiceBrowser, ZeroconfError> {
        let (browser, task) = self.browse_task()?;

        tokio::spawn(task);

        Ok(browser)
    }

    /// Start the browsing operation, which will continue until the specified
    /// timeout or until the [`ServiceBrowser`] is dropped. The returned
    /// [`ProcessTask`] future must be awaited to process events associated with
    /// the browser.
    ///
    /// # Note
    /// This method is intended if more control is needed over how the task
    /// is spawned. [`ServiceBrowserBuilder::browse`] will automatically spawn
    /// the task.
    ///
    /// # Examples
    /// ```
    /// # tokio_test::block_on(async {
    /// let mut browser = async_zeroconf::ServiceBrowserBuilder::new("_http._tcp");
    /// let (mut services, task) = browser
    ///     .timeout(tokio::time::Duration::from_secs(2))
    ///     .browse_task()?;
    ///
    /// tokio::spawn(task);
    ///
    /// while let Some(Ok(v)) = services.recv().await {
    ///     println!("Service = {:?}", v);
    /// }
    /// # Ok::<(), async_zeroconf::ZeroconfError>(())
    /// # });
    /// ```
    pub fn browse_task(&self) -> Result<(ServiceBrowser, impl ProcessTask), ZeroconfError> {
        let (tx, rx) = mpsc::unbounded_channel();

        let callback_context = ServiceBrowserContext { tx };

        let context = Arc::new(callback_context);
        let context_ptr =
            Arc::as_ptr(&context) as *mut Arc<ServiceBrowserContext> as *mut libc::c_void;

        let service_handle = crate::c_intf::service_browse(
            &self.interface,
            &self.service_type,
            self.domain.as_deref(),
            Some(browse_callback),
            context_ptr,
        )?;

        let (service_ref, task) = ServiceRefWrapper::from_service(
            service_handle,
            OpType::new(&self.service_type, OpKind::Browse),
            Some(Box::new(context)),
            self.timeout,
        )?;

        log::debug!("Created ServiceBrowser");
        let browser = ServiceBrowser {
            rx,
            delegate: service_ref,
            close_on_end: self.close_on_end,
        };

        Ok((browser, task))
    }
}
