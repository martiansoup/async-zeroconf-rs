use crate::{
    BonjourError, Interface, OpKind, OpType, ProcessTask, Service, ServiceRef, ServiceRefWrapper,
    TxtRecord, ZeroconfError,
};

use futures::Future;
use futures::FutureExt;
use std::ffi;
use std::ptr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use bonjour_sys::{
    DNSServiceErrorType, DNSServiceFlags, DNSServiceRef, TXTRecordGetCount, TXTRecordGetItemAtIndex,
};

/// `ServiceResolver` is used resolve a service obtained from a
/// [ServiceBrowser][crate::ServiceBrowser]. Browsing does not obtain all
/// information about a service, for example it doesn't include port
/// information, and resolving the service will fill this information in.
///
/// # Note
/// This should be used only with services from a browse operation to ensure
/// the interface and domain are set correctly.
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
///     let resolved_service = async_zeroconf::ServiceResolver::r(&v).await?;
///     println!("Service = {}", resolved_service);
/// }
/// # Ok::<(), async_zeroconf::ZeroconfError>(())
/// # });
/// ```
#[derive(Debug)]
pub struct ServiceResolver {
    timeout: Option<Duration>,
    checked: bool,
}

impl Default for ServiceResolver {
    fn default() -> Self {
        ServiceResolver::new()
    }
}

impl ServiceResolver {
    /// Create a new `ServiceResolver` with the default settings.
    /// The operation will have no timeout and it will check if the service to
    /// be resolved was from a browser.
    pub fn new() -> Self {
        ServiceResolver {
            timeout: None,
            checked: true,
        }
    }

    /// Create a new `ServiceResolver` with a timeout.
    pub fn new_with_timeout(timeout: Duration) -> Self {
        ServiceResolver {
            timeout: Some(timeout),
            checked: true,
        }
    }

    /// Disable checking if services came from a browser
    pub fn set_unchecked(&mut self) -> &mut Self {
        self.checked = false;
        self
    }

    /// Static method to resolve the specified [Service], the service must have
    /// been produced from a [ServiceBrowser][crate::ServiceBrowser] to ensure
    /// that the required information for the resolve operation is available.
    ///
    /// # Examples
    /// ```
    /// # tokio_test::block_on(async {
    /// let mut browser = async_zeroconf::ServiceBrowserBuilder::new("_http._tcp");
    /// let mut services = browser
    ///     .timeout(tokio::time::Duration::from_secs(2))
    ///     .browse()?;
    ///
    /// while let Some(Ok(service)) = services.recv().await {
    ///     let resolved = async_zeroconf::ServiceResolver::r(&service).await?;
    ///     println!("Service = {}", resolved);
    /// }
    /// # Ok::<(), async_zeroconf::ZeroconfError>(())
    /// # });
    /// ```
    pub async fn r(service: &Service) -> Result<Service, ZeroconfError> {
        let resolver = ServiceResolver::new();
        resolver.resolve(service).await
    }

    /// Resolve the specified [Service] using this `ServiceResolver`. This does
    /// not consume the `ServiceResolver` so more services can be resolved
    /// using the same settings.
    ///
    /// # Examples
    /// ```
    /// # tokio_test::block_on(async {
    /// let mut browser = async_zeroconf::ServiceBrowserBuilder::new("_http._tcp");
    /// let mut services = browser
    ///     .timeout(tokio::time::Duration::from_secs(2))
    ///     .browse()?;
    /// let resolver = async_zeroconf::ServiceResolver::new();
    ///
    /// while let Some(Ok(service)) = services.recv().await {
    ///     let resolved = resolver.resolve(&service).await?;
    ///     println!("Service = {}", resolved);
    /// }
    /// # Ok::<(), async_zeroconf::ZeroconfError>(())
    /// # });
    /// ```
    pub async fn resolve(&self, service: &Service) -> Result<Service, ZeroconfError> {
        let (mut resolver, task) = self.resolve_inner(service)?;
        tokio::spawn(task);
        resolver.get(service).await
    }

    /// Resolve the specified [Service] using this `ServiceResolver`. The
    /// returned [ProcessTask] future must be awaited to process events
    /// associated with the browser.
    ///
    /// If the resolve operation can be constructed, this will return a
    /// future which will produce the result of the resolve operation and
    /// a task which should be awaited on to handle any events associated
    /// with the resolving.
    ///
    /// # Note
    /// This method is intended if more control is needed over how the task
    /// is spawned. [ServiceResolver::resolve] will automatically spawn the
    /// task.
    ///
    /// # Examples
    /// ```
    /// # tokio_test::block_on(async {
    /// let mut browser = async_zeroconf::ServiceBrowserBuilder::new("_http._tcp");
    /// let mut services = browser
    ///     .timeout(tokio::time::Duration::from_secs(2))
    ///     .browse()?;
    /// let resolver = async_zeroconf::ServiceResolver::new();
    ///
    /// while let Some(Ok(service)) = services.recv().await {
    ///     if let Ok((future, task)) = resolver.resolve_task(&service).await {
    ///         tokio::spawn(task);
    ///         let resolved = future.await?;
    ///         println!("Service = {}", resolved);
    ///     }
    /// }
    /// # Ok::<(), async_zeroconf::ZeroconfError>(())
    /// # });
    /// ```
    pub async fn resolve_task(
        &self,
        service: &Service,
    ) -> Result<
        (
            impl Future<Output = Result<Service, ZeroconfError>>,
            impl ProcessTask,
        ),
        ZeroconfError,
    > {
        match self.resolve_inner(service) {
            Ok((mut resolver, task)) => {
                let s = service.clone();
                Ok((async move { resolver.get(&s).await }, task))
            }
            Err(e) => Err(e),
        }
    }

    fn resolve_inner(
        &self,
        service: &Service,
    ) -> Result<(ServiceResolverResult, impl ProcessTask), ZeroconfError> {
        if !self.checked || (service.browse() && !service.resolve()) {
            self.resolve_inner_unchecked(service)
        } else {
            Err(ZeroconfError::NotFromBrowser(service.clone()))
        }
    }

    fn resolve_inner_unchecked(
        &self,
        service: &Service,
    ) -> Result<(ServiceResolverResult, impl ProcessTask), ZeroconfError> {
        let (tx, rx) = mpsc::unbounded_channel();

        let callback_context = ServiceResolverContext { tx };

        let context = Arc::new(callback_context);
        let context_ptr =
            Arc::as_ptr(&context) as *mut Arc<ServiceResolverContext> as *mut libc::c_void;

        let domain = &service
            .domain()
            .as_ref()
            .ok_or_else(|| ZeroconfError::NotFromBrowser(service.clone()))?;

        let service_handle = crate::c_intf::service_resolve(
            service.name(),
            &service.interface(),
            service.service_type(),
            domain,
            Some(resolve_callback),
            context_ptr,
        )?;

        let (delegate, task) = ServiceRefWrapper::from_service(
            service_handle,
            OpType::new(service.service_type(), OpKind::Resolve),
            Some(Box::new(context)),
            self.timeout,
        )?;

        let result = ServiceResolverResult { rx, delegate };

        Ok((result, task))
    }
}

#[derive(Debug)]
struct ResolverInformation {
    interface: Interface,
    fullname: String,
    hosttarget: String,
    port: u16,
    txt_record: TxtRecord,
}

impl ResolverInformation {
    fn merge(self, service: &Service) -> Service {
        assert_eq!(
            &self.interface,
            service.interface(),
            "Interface should match on resolved service"
        );

        let mut s = Service::new_with_txt(
            service.name(),
            service.service_type(),
            self.port,
            self.txt_record,
        );

        s.set_interface(self.interface).set_resolve();

        match service.domain() {
            Some(d) => {
                let host = self.hosttarget.replace(d, "");
                s.set_host(host[0..host.len() - 1].to_string())
                    .set_domain(d.to_string())
            }
            _ => &s,
        };

        s
    }
}

#[derive(Debug)]
struct ServiceResolverResult {
    rx: mpsc::UnboundedReceiver<Result<ResolverInformation, ZeroconfError>>,
    delegate: ServiceRef,
}

impl ServiceResolverResult {
    async fn get(&mut self, service: &Service) -> Result<Service, ZeroconfError> {
        self.rx
            .recv()
            .map(move |res| match res {
                Some(Ok(s)) => Ok(s.merge(service)),
                Some(Err(e)) => Err(e),
                None => Err(ZeroconfError::Timeout(service.clone())),
            })
            .await
    }
}

#[derive(Debug)]
struct ServiceResolverContext {
    tx: mpsc::UnboundedSender<Result<ResolverInformation, ZeroconfError>>,
}

impl ServiceResolverContext {
    fn send(&self, info: Result<ResolverInformation, ZeroconfError>) {
        if self.tx.send(info).is_err() {
            log::warn!("Failed to send resolved information, receiver dropped");
        }
    }
}

unsafe fn resolve_callback_inner(
    intf_index: u32,
    fullname: *const libc::c_char,
    hosttarget: *const libc::c_char,
    port: u16,
    txt_len: u16,
    txt_record: *const libc::c_uchar,
) -> Result<ResolverInformation, ZeroconfError> {
    let c_fullname = ffi::CStr::from_ptr(fullname);
    let c_hosttarget = ffi::CStr::from_ptr(hosttarget);
    let fullname = c_fullname.to_str()?;
    let hosttarget = c_hosttarget.to_str()?;
    let port = port.to_be();

    log::debug!(
        "ServiceResolve Callback OK ({}:{}:{})",
        fullname,
        hosttarget,
        port
    );

    let txt_count = TXTRecordGetCount(txt_len, txt_record as *const libc::c_void);
    let mut txt = TxtRecord::new();
    for i in 0..txt_count {
        let keysize: u16 = 256;
        let mut valsize = 0;
        let mut valptr: *const libc::c_void = ptr::null_mut();
        let mut keybuf = vec![0; (keysize + 1).into()];
        let keyptr = keybuf.as_mut_ptr() as *mut i8;
        let err = TXTRecordGetItemAtIndex(
            txt_len,
            txt_record as *const libc::c_void,
            i,
            keysize,
            keyptr,
            &mut valsize,
            &mut valptr,
        );

        if err == 0 {
            let keylen = keybuf.iter().position(|&c| c == 0).expect(
                "No error reported by TXTRecordGetItemAtIndex but no null byte in key string",
            );
            keybuf.truncate(keylen);

            let key = String::from_utf8_lossy(&keybuf);
            let val_slice =
                std::slice::from_raw_parts(valptr as *const libc::c_uchar, valsize.into());

            txt.add_vec(key.into_owned(), val_slice.to_owned());
        } else {
            log::error!(
                "TXTRecordGetItemAtIndex Callback Error ({}:{})",
                err,
                Into::<BonjourError>::into(err)
            );
            return Err(err.into());
        }
    }

    let info = ResolverInformation {
        interface: Interface::Interface(intf_index),
        fullname: fullname.to_string(),
        hosttarget: hosttarget.to_string(),
        port,
        txt_record: txt,
    };

    Ok(info)
}

// Callback passed to DNSServiceResolve
unsafe extern "C" fn resolve_callback(
    _sd_ref: DNSServiceRef,
    flags: DNSServiceFlags,
    intf_index: u32,
    error: DNSServiceErrorType,
    fullname: *const libc::c_char,
    hosttarget: *const libc::c_char,
    port: u16,
    txt_len: u16,
    txt_record: *const libc::c_uchar,
    context: *mut libc::c_void,
) {
    let proxy = &*(context as *const ServiceResolverContext);
    if error == 0 {
        let more = (flags & 0x1) == 0x1;
        if more {
            log::warn!("Unexpected DNSServiceFlagsMoreComing set on resolve")
        }

        proxy.send(resolve_callback_inner(
            intf_index, fullname, hosttarget, port, txt_len, txt_record,
        ));
    } else {
        proxy.send(Err(error.into()));
        log::error!(
            "ServiceResolve Callback Error ({}:{})",
            error,
            Into::<BonjourError>::into(error)
        )
    }
}
