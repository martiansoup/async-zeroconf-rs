use crate::{
    BonjourError, Interface, OpKind, OpType, ProcessTask, ServiceRef, ServiceRefWrapper, TxtRecord,
    ZeroconfError,
};
use std::{ffi, fmt};
use tokio::sync::mpsc;

use bonjour_sys::{DNSServiceErrorType, DNSServiceFlags, DNSServiceRef};
use std::future::Future;
use std::sync::Arc;

#[derive(Debug)]
struct ServicePublishContext {
    tx: mpsc::UnboundedSender<Result<(), BonjourError>>,
}

impl ServicePublishContext {
    fn send(&self, e: Result<(), BonjourError>) {
        if let Err(e) = self.tx.send(e) {
            log::warn!("Failed to send status, receiver dropped: {}", e);
        }
    }
}

/// Struct representing a `ZeroConf` service. This should be created with all
/// the information that should be associated with the service and then the
/// [`publish`][`Service::publish`] method can be used to register the service.
/// The [`ServiceRef`] returned from [`publish`][`Service::publish`] should be held
/// for as long as the service should continue being advertised, once dropped
/// the service will be deallocated.
///
/// # Examples
///
/// Normally the default values of `domain`, `host` and `interface` don't need
/// to be changed.
/// ```
/// # tokio_test::block_on(async {
/// let service_ref = async_zeroconf::Service::new("Server", "_http._tcp", 80)
///                       .publish().await?;
/// // Service kept alive until service_ref dropped
/// # Ok::<(), async_zeroconf::ZeroconfError>(())
/// # });
/// ```
///
/// These fields can be customised if required. More details are available in
/// the [`DNSServiceRegister`][reg] documentation.
/// ```
/// # tokio_test::block_on(async {
/// let service_ref = async_zeroconf::Service::new("Server", "_http._tcp", 80)
///                       .set_domain("local".to_string())
///                       .set_host("localhost".to_string())
///                       .publish().await?;
/// // Service kept alive until service_ref dropped
/// # Ok::<(), async_zeroconf::ZeroconfError>(())
/// # });
/// ```
/// [reg]: https://developer.apple.com/documentation/dnssd/1804733-dnsserviceregister?language=objc
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Service {
    name: String,
    service_type: String,
    port: u16,
    interface: Interface,
    domain: Option<String>,
    host: Option<String>,
    txt: TxtRecord,
    browse: bool,
    resolve: bool,
    allow_rename: bool,
}

impl fmt::Display for Service {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let host_fmt = self.host.as_deref().unwrap_or("*");
        let txt = if self.txt.is_empty() {
            "".to_string()
        } else {
            format!(" {}", self.txt)
        };
        write!(
            f,
            "[{}:{}] @{}:{}{}",
            self.name, self.service_type, host_fmt, self.port, txt
        )
    }
}

// Callback passed to DNSServiceRegister
unsafe extern "C" fn dns_sd_callback(
    _sd_ref: DNSServiceRef,
    _flags: DNSServiceFlags,
    error: DNSServiceErrorType,
    name: *const libc::c_char,
    regtype: *const libc::c_char,
    domain: *const libc::c_char,
    context: *mut libc::c_void,
) {
    let proxy = &*(context as *const ServicePublishContext);
    if error == 0 {
        let c_name = ffi::CStr::from_ptr(name);
        let c_type = ffi::CStr::from_ptr(regtype);
        let c_domain = ffi::CStr::from_ptr(domain);
        let name = c_name
            .to_str()
            .expect("string originally from rust should be safe");
        let regtype = c_type
            .to_str()
            .expect("string originally from rust should be safe");
        let domain = c_domain
            .to_str()
            .expect("string originally from rust should be safe");
        log::debug!("Service Callback OK ({}:{}:{})", name, regtype, domain);
        proxy.send(Ok(()));
    } else {
        log::debug!(
            "Service Callback Error ({}:{})",
            error,
            Into::<BonjourError>::into(error)
        );
        proxy.send(Err(error.into()));
    }
}

impl Service {
    /// Create a new Service, called `name` of type `service_type` that is
    /// listening on port `port`.
    ///
    /// This must then be published with [`Service::publish`] to advertise the
    /// service.
    ///
    /// # Examples
    ///
    /// ```
    /// // Create a service description
    /// let service = async_zeroconf::Service::new("Web Server", "_http._tcp", 80);
    /// ```
    pub fn new(name: &str, service_type: &str, port: u16) -> Self {
        Service::new_with_txt(name, service_type, port, TxtRecord::new())
    }

    /// Create a new Service, called `name` of type `service_type` that is
    /// listening on port `port` with the TXT records described by `txt`.
    ///
    /// This must then be published with [`Service::publish`] to advertise the
    /// service.
    ///
    /// # Examples
    ///
    /// ```
    /// // Create a TXT record collection
    /// let mut txt = async_zeroconf::TxtRecord::new();
    /// txt.add("version".to_string(), "0.1".to_string());
    /// // Create a service description
    /// let service = async_zeroconf::Service::new_with_txt("Web Server", "_http._tcp", 80, txt);
    /// ```
    pub fn new_with_txt(name: &str, service_type: &str, port: u16, txt: TxtRecord) -> Self {
        Service {
            name: name.to_string(),
            service_type: service_type.to_string(),
            port,
            interface: Default::default(),
            domain: None,
            host: None,
            txt,
            browse: false,
            resolve: false,
            allow_rename: true,
        }
    }

    fn validate_service_type(&self) -> bool {
        if self.service_type.contains('.') {
            let parts: Vec<&str> = self.service_type.split('.').collect();
            if parts[0].starts_with('_') && (parts[1] == "_udp" || parts[1] == "_tcp") {
                return true;
            }
        }
        false
    }

    fn validate(&self) -> Result<(), ZeroconfError> {
        if self.validate_service_type() {
            self.txt.validate()
        } else {
            Err(ZeroconfError::InvalidServiceType(self.service_type.clone()))
        }
    }

    /// Set an interface to advertise the service on rather than all.
    ///
    /// By default the service will be advertised on all interfaces.
    pub fn set_interface(&mut self, interface: Interface) -> &mut Self {
        self.interface = interface;
        self
    }

    /// Get this interface associated with this service
    pub fn interface(&self) -> &Interface {
        &self.interface
    }

    /// Prevent renaming of this service if there is a name collision.
    ///
    /// By default the service will be automatically renamed.
    pub fn prevent_rename(&mut self) -> &mut Self {
        self.allow_rename = false;
        self
    }

    /// Set the (optional) domain for the service.
    ///
    /// If not specified, the default domain is used.
    pub fn set_domain(&mut self, domain: String) -> &mut Self {
        self.domain = Some(domain);
        self
    }

    /// Get the domain of this service
    pub fn domain(&self) -> &Option<String> {
        &self.domain
    }

    /// Set the (optional) hostname for the service.
    ///
    /// If not set, the hostname of the host will be used.
    pub fn set_host(&mut self, host: String) -> &mut Self {
        self.host = Some(host);
        self
    }

    /// Set the from browse flag for this service
    pub(crate) fn set_browse(&mut self) -> &mut Self {
        self.browse = true;
        self
    }

    /// Set the from resolve flag for this service
    pub(crate) fn set_resolve(&mut self) -> &mut Self {
        self.resolve = true;
        self
    }

    /// Get the name of the service
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the type of the service
    pub fn service_type(&self) -> &str {
        &self.service_type
    }

    /// Get the port of the service
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Get the host of the service
    pub fn host(&self) -> &Option<String> { &self.host }

    /// Get the TxtRecord for this service
    pub fn txt(&self) -> &TxtRecord { &self.txt }

    /// Add a TXT entry to the service
    pub fn add_txt(&mut self, k: String, v: String) -> &mut Self {
        self.txt.add(k, v);
        self
    }

    /// Get the browse flag
    pub(crate) fn browse(&self) -> bool {
        self.browse
    }

    /// Get the resolve flag
    pub(crate) fn resolve(&self) -> bool {
        self.resolve
    }

    /// Publish the service, returns a [`ServiceRef`] which should be held to
    /// keep the service alive. Once the [`ServiceRef`] is dropped the service
    /// will be removed and deallocated.
    ///
    /// # Arguments
    ///
    /// * `allow_rename` - Allow the service to be automatically renamed if
    /// a service with the same name already exists
    ///
    /// # Examples
    /// ```
    /// # tokio_test::block_on(async {
    /// // Create a service description
    /// let service = async_zeroconf::Service::new("Server", "_http._tcp", 80);
    /// // Publish the service
    /// let service_ref = service.publish().await?;
    /// // Service kept alive until service_ref dropped
    /// # Ok::<(), async_zeroconf::ZeroconfError>(())
    /// # });
    /// ```
    pub async fn publish(&self) -> Result<ServiceRef, ZeroconfError> {
        let (service, task, future) = self.publish_task()?;

        // Spawn task
        tokio::spawn(task);

        // Get any errors and wait until service started
        future.await?;

        Ok(service)
    }

    /// Publish the service, returns a [`ServiceRef`] which should be held to
    /// keep the service alive and a future which should be awaited on to
    /// respond to any events associated with keeping the service registered.
    /// Once the [`ServiceRef`] is dropped the service will be removed and
    /// deallocated.
    ///
    /// # Note
    /// This method is intended if more control is needed over how the task
    /// is spawned. [`Service::publish`] will automatically spawn the task.
    /// The task should be spawned first to process events, and then the
    /// returned future waited on to collect any errors that occurred.
    ///
    /// # Examples
    /// ```
    /// # tokio_test::block_on(async {
    /// // Create a service description
    /// let service = async_zeroconf::Service::new("Server", "_http._tcp", 80);
    /// // Publish the service
    /// let (service_ref, task, service_ok) = service.publish_task()?;
    /// // Spawn the task to respond to events
    /// tokio::spawn(task);
    /// // Wait to confirm service started ok
    /// service_ok.await?;
    /// // Service kept alive until service_ref dropped
    /// # Ok::<(), async_zeroconf::ZeroconfError>(())
    /// # });
    /// ```
    pub fn publish_task(
        &self,
    ) -> Result<
        (
            ServiceRef,
            impl ProcessTask,
            impl Future<Output = Result<(), ZeroconfError>>,
        ),
        ZeroconfError,
    > {
        self.validate()?;

        let (tx, mut rx) = mpsc::unbounded_channel();

        let callback_context = ServicePublishContext { tx };
        let context = Arc::new(callback_context);
        let context_ptr =
            Arc::as_ptr(&context) as *mut Arc<ServicePublishContext> as *mut libc::c_void;

        let service_ref = crate::c_intf::service_register(
            (&self.name, &self.service_type, self.port),
            &self.interface,
            (self.domain.as_deref(), self.host.as_deref()),
            &self.txt,
            Some(dns_sd_callback),
            self.allow_rename,
            context_ptr,
        )?;

        let (r, task) = ServiceRefWrapper::from_service(
            service_ref,
            OpType::new(&self.service_type, OpKind::Publish),
            Some(Box::new(context)),
            None,
        )?;

        let fut = async move {
            match rx.recv().await {
                Some(v) => v.map_err(|e| e.into()),
                None => Err(ZeroconfError::Dropped),
            }
        };

        Ok((r, task, fut))
    }
}
