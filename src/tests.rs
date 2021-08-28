use crate::{Interface, Service, ServiceBrowserBuilder, ServiceResolver, TxtRecord, ZeroconfError};

#[test]
fn create_service() {
    let name = "test service";
    let service_type = "_http._tcp";
    let port = 80;
    let service = Service::new(name, service_type, port);
    assert_eq!(service.name(), name);
    assert_eq!(service.port(), port);
    assert_eq!(service.service_type(), service_type);
}

#[test]
fn create_invalid_service() {
    let strings = ["http.tcp", "http.http", "http"];
    for s in strings {
        let service = Service::new_with_txt("", s, 0, Default::default()).publish();
        println!("Testing '{}'", s);
        assert!(service.is_err());
    }
}

#[tokio::test]
async fn publish_service() -> Result<(), ZeroconfError> {
    let service = Service::new("Server", "_http._tcp", 80);
    let _service_ref = service.publish()?;

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    Ok(())
}

#[tokio::test]
async fn publish_service_alt() -> Result<(), ZeroconfError> {
    let mut service = Service::new("", "_http._tcp", 80);
    let _service_ref = service
        .prevent_rename()
        .set_interface(Interface::from_ifname("lo0")?)
        .set_host("localhost".to_string())
        .set_domain("local".to_string())
        .add_txt("k".to_string(), "v".to_string())
        .publish()?;
    Ok(())
}

#[tokio::test]
async fn publish_service_err_name() {
    let service = Service::new("Server\0", "_http._tcp", 80);
    let service_ref = service.publish();
    assert!(matches!(service_ref.unwrap_err(), ZeroconfError::NullString(_)))
}

#[tokio::test]
async fn publish_service_err_req() {
    let service = Service::new("Server", "_http\0._tcp", 80);
    let service_ref = service.publish();
    assert!(matches!(service_ref.unwrap_err(), ZeroconfError::NullString(_)))
}

#[tokio::test]
async fn publish_service_err_domain() {
    let mut service = Service::new("Server", "_http._tcp", 80);
    let service_ref = service.set_domain("\0".to_string()).publish();
    assert!(matches!(service_ref.unwrap_err(), ZeroconfError::NullString(_)))
}

#[tokio::test]
async fn browser() -> Result<(), ZeroconfError> {
    let mut browser = ServiceBrowserBuilder::new("_smb._tcp");
    let mut services = browser
        .timeout(tokio::time::Duration::from_secs(2))
        .browse()?;

    while let Some(Ok(v)) = services.recv().await {
        println!("Service = {}", v);
    }
    Ok(())
}

#[tokio::test]
async fn resolve() -> Result<(), ZeroconfError> {
    let mut browser = ServiceBrowserBuilder::new("_smb._tcp");
    let mut services = browser
        .timeout(tokio::time::Duration::from_secs(2))
        .browse()?;

    while let Some(Ok(v)) = services.recv().await {
        let resolved_service = ServiceResolver::r(&v).await?;
        println!("Service = {:?}", resolved_service);
    }
    Ok(())
}

/// TXT record validation
#[test]
fn txt_validate_key_len_ok() {
    let mut t = TxtRecord::new();
    t.add("123456789".to_string(), "v".to_string());
    assert!(t.validate().is_ok())
}

#[test]
fn txt_validate_key_len_err() {
    let mut t = TxtRecord::new();
    t.add("1234567890".to_string(), "v".to_string());
    assert!(t.validate().is_err())
}

#[test]
fn txt_validate_key_eq_err() {
    let mut t = TxtRecord::new();
    t.add("1=1".to_string(), "v".to_string());
    assert!(t.validate().is_err())
}

#[test]
fn txt_validate_key_non_ascii_print_err() {
    for i in 0..0x20 {
        let mut t = TxtRecord::new();
        let v = vec![i];
        t.add(String::from_utf8_lossy(&v).to_string(), "v".to_string());
        assert!(t.validate().is_err(), "{}:{}", i, t);
    }
}

#[test]
fn txt_validate_key_non_ascii_err() {
    let mut t = TxtRecord::new();
    t.add("🐳".to_string(), "v".to_string());
    assert!(t.validate().is_err())
}

#[test]
fn txt_validate_val_len_ok() {
    let mut t = TxtRecord::new();
    let mut vec = Vec::new();
    vec.resize(255, 0x20);
    t.add_vec("k".to_string(), vec);
    assert!(t.validate().is_ok())
}

#[test]
fn txt_validate_val_len_err() {
    let mut t = TxtRecord::new();
    let mut vec = Vec::new();
    vec.resize(256, 0x20);
    t.add_vec("k".to_string(), vec);
    assert!(t.validate().is_err())
}

/// TXT record iterators
#[test]
fn txt_compare_string_iter_to_iter() {
    let mut txt = TxtRecord::new();
    txt.add("key".to_string(), "value".to_string());
    txt.add("key2".to_string(), "va\0lue".to_string());
    // String iterator
    let s_iter = txt.iter_string();
    // Equivalent iterator
    let iter = txt.iter().map(|(k, v)| (k, std::str::from_utf8(v)));
    assert_eq!(
        s_iter.collect::<Vec<(&String, Result<&str, std::str::Utf8Error>)>>(),
        iter.collect::<Vec<(&String, Result<&str, std::str::Utf8Error>)>>()
    )
}

#[test]
fn txt_compare_string_iter_lossy_to_iter() {
    let mut txt = TxtRecord::new();
    txt.add("key".to_string(), "value".to_string());
    txt.add("key2".to_string(), "va\0lue".to_string());
    // String iterator
    let s_iter = txt.iter_string_lossy();
    // Equivalent iterator
    let iter = txt
        .iter()
        .map(|(k, v)| (k, std::str::from_utf8(v).unwrap_or("�")));
    assert_eq!(
        s_iter.collect::<Vec<(&String, &str)>>(),
        iter.collect::<Vec<(&String, &str)>>()
    )
}
