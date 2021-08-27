// Private helper structures to wrap the service reference

use crate::{BonjourError, ZeroconfError};

use bonjour_sys::{
    DNSServiceProcessResult, DNSServiceRef, DNSServiceRefDeallocate, DNSServiceRefSockFD,
};
use futures::Future;
use std::any::Any;
use std::fmt::Display;
use std::sync::Mutex;
use std::time::Duration;
use tokio::io::unix::AsyncFd;
use tokio::sync::oneshot;

/// `OpType` is used to indicate the service type and the kind of operation
/// associated with a [ServiceRef]. Primarily intended for debug.
///
/// # Examples
/// ```
/// # tokio_test::block_on(async {
/// let service = async_zeroconf::Service::new("Server", "_http._tcp", 80);
/// let service_ref = service.publish()?;
///
/// assert_eq!(service_ref.op_type().service_type(), "_http._tcp");
/// assert_eq!(service_ref.op_type().kind(), &async_zeroconf::OpKind::Publish);
/// # Ok::<(), async_zeroconf::ZeroconfError>(())
/// # });
/// ```
#[derive(Debug, Clone)]
pub struct OpType {
    service_type: String,
    kind: OpKind,
}

impl OpType {
    pub(crate) fn new(service_type: &str, kind: OpKind) -> Self {
        OpType {
            service_type: service_type.to_string(),
            kind,
        }
    }

    /// The associated service type (e.g. `"_http._tcp"`).
    pub fn service_type(&self) -> &str {
        &self.service_type
    }

    /// The associated type of operation (e.g. publishing a service).
    pub fn kind(&self) -> &OpKind {
        &self.kind
    }
}

impl Display for OpType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{:?}[{}]", self.kind, self.service_type)
    }
}

/// `OpKind` represents the possible kinds of operation associated with a
/// [ServiceRef], primarily used for debug and obtained from the [OpType]
/// returned by [ServiceRef::op_type].
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum OpKind {
    /// An operation publishing a service.
    Publish,
    /// An operation to browse for a given type of service.
    Browse,
    /// An operation to resolve a service.
    Resolve,
}

/// Struct to hold a published service, which keeps the service alive while a
/// reference to it is held.
/// When dropped the Service will be removed and any associated resources
/// deallocated.
///
/// This should be created via a [Service][crate::Service] or a
/// [ServiceResolver][crate::ServiceResolver]. For a browse
/// operation the `ServiceRef` is held by the `ServiceBrowser` created by a
/// [ServiceBrowserBuilder][crate::ServiceBrowserBuilder].
#[derive(Debug)]
#[must_use]
pub struct ServiceRef {
    shutdown_tx: Option<oneshot::Sender<()>>,
    op_type: OpType,
}

impl ServiceRef {
    /// Return a descriptive type of the operation associated with this
    /// reference.
    pub fn op_type(&self) -> &OpType {
        &self.op_type
    }
}

impl Drop for ServiceRef {
    fn drop(&mut self) {
        log::debug!("Dropping ServiceRef ({})", self.op_type);
        // Send shutdown to end process task if idle
        // Should only fail if rx already dropped
        if self
            .shutdown_tx
            .take()
            .expect("shutdown taken before drop")
            .send(())
            .is_err()
        {}
    }
}

// Internal type to hold the file descriptor for the socket associated with the
// service.
#[derive(Debug)]
pub(crate) struct ServiceFileDescriptor {
    pub fd: i32,
}

// Allow ServiceFileDescriptor to be convered to a AsyncFd by implementing the
// AsRawFd trait.
impl std::os::unix::prelude::AsRawFd for ServiceFileDescriptor {
    fn as_raw_fd(&self) -> i32 {
        self.fd
    }
}

/// The `ProcessTask` trait represents the future that is returned from some
/// functions that is awaited on to process events associated with a published
/// service or a browse operation.
pub trait ProcessTask: Future<Output = ()> + Send + Sync {}

impl<T> ProcessTask for T where T: Future<Output = ()> + Send + Sync {}

#[derive(Debug)]
pub(crate) struct ServiceRefWrapper {
    // Pointer to reference returned by C API
    pub inner: DNSServiceRef,
    // Mutex to protect service reference
    pub lock: Mutex<()>,
    // Async file descriptor to detect new events asynchronously
    pub fd: AsyncFd<ServiceFileDescriptor>,
    // Hold a reference to an (optional) context used for C API callbacks
    context: Option<Box<dyn Any + Send>>,
    // Operation type that created this reference
    op_type: OpType,
}

impl ServiceRefWrapper {
    pub fn from_service(
        service_ref: DNSServiceRef,
        op_type: OpType,
        context: Option<Box<dyn Any + Send>>,
        timeout: Option<Duration>,
    ) -> Result<(ServiceRef, impl ProcessTask), std::io::Error> {
        log::trace!("Call DNSServiceRefSockFD");
        let fd = unsafe { DNSServiceRefSockFD(service_ref) };
        log::trace!("  FD:{}", fd);

        log::debug!("Creating ServiceRef ({})", op_type);

        match AsyncFd::new(ServiceFileDescriptor { fd }) {
            Ok(async_fd) => {
                // Create channel for shutdown
                let (tx, rx) = oneshot::channel::<()>();

                // Create the wrapper for processing events
                let wrapper = ServiceRefWrapper {
                    inner: service_ref,
                    lock: Mutex::new(()),
                    fd: async_fd,
                    context,
                    op_type: op_type.clone(),
                };

                // Spawn the task that will process events
                let task = async move {
                    match ServiceRefWrapper::process(rx, wrapper, timeout).await {
                        Ok(_) => (),
                        Err(e) => log::error!("Error on processing: {}", e),
                    }
                };

                // Create the reference that will hold the service active
                let s_ref = ServiceRef {
                    shutdown_tx: Some(tx),
                    op_type,
                };

                Ok((s_ref, task))
            }
            Err(e) => Err(e),
        }
    }

    /// A future to wait for any pending events related to the service,
    /// handling them and then completing the future.
    async fn process_events(service_ref: &ServiceRefWrapper) -> Result<bool, ZeroconfError> {
        // Wait on indication that file descriptor is readable
        let mut fd = service_ref.fd.readable().await?;

        log::trace!("Call DNSServiceProcessResult");

        // Process any pending events
        let process_err = {
            let mut _guard = service_ref.lock.lock()?;
            unsafe { DNSServiceProcessResult(service_ref.inner) }
        };
        // Clear ready flag for socket to wait for next event
        // As there is no await point or polling between processing above and
        // clearing the flag, there should be no opportunity to 'miss' an event
        // between the DNSServiceProcessResult and clear_ready().
        fd.clear_ready();
        if process_err != 0 {
            return Err(Into::<BonjourError>::into(process_err).into());
        }

        Ok(true)
    }

    /// Processing wrapper to keep processing events as available
    async fn process(
        mut rx: oneshot::Receiver<()>,
        service_ref: ServiceRefWrapper,
        timeout: Option<Duration>,
    ) -> Result<(), ZeroconfError> {
        let (tx_time, mut rx_time) = oneshot::channel();

        if let Some(t) = timeout {
            tokio::spawn(async move {
                tokio::time::sleep(t).await;
                match tx_time.send(()) {
                    Ok(_) => {
                        log::debug!("Sending timeout");
                    }
                    Err(_) => {
                        log::trace!("Sending timeout failed - processing ended due to shutdown");
                    }
                }
            });
        }

        loop {
            tokio::select! {
                // Shutdown event
                _ = &mut rx => {
                    log::debug!("Process task got shutdown");
                    return Ok(());
                }
                // Timeout future
                _ = &mut rx_time => {
                    log::debug!("Process task got timeout");
                    return Ok(());
                }
                // Event processing
                e = Self::process_events(&service_ref) => {
                    match e {
                        Ok(b) => {
                            if b {
                                log::trace!("Events processed");
                            } else {
                                log::trace!("Got null pointer due to shutdown");
                                return Ok(());
                            }
                        },
                        Err(e) => return Err(e)
                    }
                }
            }
        }
    }
}

// Implement Send as reference is thread-safe
unsafe impl Send for ServiceRefWrapper {}
// Implement Sync as reference is protected by mutex
unsafe impl Sync for ServiceRefWrapper {}

impl Drop for ServiceRefWrapper {
    fn drop(&mut self) {
        log::debug!(
            "Dropping and deallocating service reference ({})",
            self.op_type
        );
        {
            match self.lock.lock() {
                Ok(_guard) => {
                    unsafe { DNSServiceRefDeallocate(self.inner) };
                }
                Err(_) => {
                    log::warn!("Service reference mutex was poisoned");
                    unsafe { DNSServiceRefDeallocate(self.inner) };
                }
            }
        }
        if self.context.is_some() {
            log::debug!("Context to be dropped ({})", self.op_type);
        }
    }
}
