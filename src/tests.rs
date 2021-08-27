use crate::{Service, TxtRecord};

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
        let service = Service::new("", s, 0).publish();
        println!("Testing '{}'", s);
        assert!(service.is_err());
    }
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
