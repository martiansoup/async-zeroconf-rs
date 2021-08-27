use crate::{Interface, TxtRecord, ZeroconfError};
use std::convert::TryInto;
use std::{ffi, mem, ptr};

use bonjour_sys::{
    kDNSServiceFlagsNoAutoRename, DNSServiceBrowse, DNSServiceBrowseReply, DNSServiceRef,
    DNSServiceRegister, DNSServiceRegisterReply, DNSServiceResolve, DNSServiceResolveReply,
    TXTRecordCreate, TXTRecordDeallocate, TXTRecordGetBytesPtr, TXTRecordGetLength, TXTRecordRef,
    TXTRecordSetValue,
};

pub(crate) fn service_register(
    name: &str,
    reqtype: (&str, u16),
    interface: &Interface,
    domain_host: (Option<&str>, Option<&str>),
    txt: &TxtRecord,
    callback: DNSServiceRegisterReply,
    allow_rename: bool,
) -> Result<DNSServiceRef, ZeroconfError> {
    log::trace!("Formatting C arguments for DNSServiceRegister");
    let mut service_ref: DNSServiceRef = ptr::null_mut();
    let flags = if allow_rename {
        0
    } else {
        kDNSServiceFlagsNoAutoRename
    };
    let intf_index = match interface {
        Interface::Unspecified => 0,
        Interface::Interface(id) => *id,
    };

    let (reqtype, port) = reqtype;
    let (domain, host) = domain_host;

    let cname = ffi::CString::new(name)?;
    let name = if !name.is_empty() {
        cname.as_ptr()
    } else {
        ptr::null()
    };
    let creqtype = ffi::CString::new(reqtype)?;
    let reqtype = creqtype.as_ptr();

    let cdomain = ffi::CString::new(domain.unwrap_or(""))?;
    let domain = match domain {
        Some(_) => cdomain.as_ptr(),
        None => ptr::null(),
    };
    let chost = ffi::CString::new(host.unwrap_or(""))?;
    let host = match host {
        Some(_) => chost.as_ptr(),
        None => ptr::null(),
    };
    let port = port.to_be();

    let mut txt_record = unsafe {
        let mut txt: TXTRecordRef = mem::zeroed();
        TXTRecordCreate(&mut txt, 0, ptr::null_mut());
        txt
    };
    for (k, v) in txt.iter() {
        let key = ffi::CString::new(k.clone())?;
        let value_size = v.len().try_into().expect("max size should be checked");
        let value = ffi::CString::new(v.clone())?;
        unsafe {
            TXTRecordSetValue(
                &mut txt_record,
                key.as_ptr(),
                value_size,
                value.as_ptr() as *const libc::c_void,
            );
        }
    }

    let txt_len = unsafe { TXTRecordGetLength(&txt_record) };
    let txt_ptr = unsafe { TXTRecordGetBytesPtr(&txt_record) };
    let context = ptr::null_mut();
    log::trace!("Call DNSServiceRegister");
    let err = unsafe {
        DNSServiceRegister(
            &mut service_ref as *mut DNSServiceRef,
            flags,
            intf_index,
            name,
            reqtype,
            domain,
            host,
            port,
            txt_len,
            txt_ptr,
            callback,
            context,
        )
    };
    // Deallocate any TXT resources
    unsafe { TXTRecordDeallocate(&mut txt_record) };

    if err == 0 {
        Ok(service_ref)
    } else {
        Err(err.into())
    }
}

pub(crate) fn service_browse(
    intf: &Interface,
    reqtype: &str,
    domain: Option<&str>,
    callback: DNSServiceBrowseReply,
    context: *mut libc::c_void,
) -> Result<DNSServiceRef, ZeroconfError> {
    log::trace!("Formatting C arguments for DNSServiceBrowse");
    let mut service_ref: DNSServiceRef = ptr::null_mut();

    let intf_index = match intf {
        Interface::Unspecified => 0,
        Interface::Interface(id) => *id,
    };

    let creqtype = ffi::CString::new(reqtype)?;
    let reqtype = creqtype.as_ptr();

    let cdomain = ffi::CString::new(domain.unwrap_or(""))?;
    let domain = match domain {
        Some(_) => cdomain.as_ptr(),
        None => ptr::null(),
    };

    log::trace!("Call DNSServiceBrowse");
    let err = unsafe {
        DNSServiceBrowse(
            &mut service_ref as *mut DNSServiceRef,
            0,
            intf_index,
            reqtype,
            domain,
            callback,
            context,
        )
    };

    if err == 0 {
        Ok(service_ref)
    } else {
        Err(err.into())
    }
}

pub(crate) fn service_resolve(
    name: &str,
    intf: &Interface,
    reqtype: &str,
    domain: &str,
    callback: DNSServiceResolveReply,
    context: *mut libc::c_void,
) -> Result<DNSServiceRef, ZeroconfError> {
    log::trace!("Formatting C arguments for DNSServiceResolve");
    let mut service_ref: DNSServiceRef = ptr::null_mut();

    let cname = ffi::CString::new(name)?;
    let name = cname.as_ptr();

    let intf_index = match intf {
        Interface::Unspecified => 0,
        Interface::Interface(id) => *id,
    };

    let creqtype = ffi::CString::new(reqtype)?;
    let reqtype = creqtype.as_ptr();

    let cdomain = ffi::CString::new(domain)?;
    let domain = cdomain.as_ptr();

    log::trace!("Call DNSServiceResolve");
    let err = unsafe {
        DNSServiceResolve(
            &mut service_ref as *mut DNSServiceRef,
            0,
            intf_index,
            name,
            reqtype,
            domain,
            callback,
            context,
        )
    };

    if err == 0 {
        Ok(service_ref)
    } else {
        Err(err.into())
    }
}
