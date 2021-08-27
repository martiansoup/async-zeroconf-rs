use std::collections::HashMap;
use std::fmt;
use std::str::Utf8Error;

use crate::ZeroconfError;

/// Struct containing the entries for TXT records associated with a service
///
/// # Examples
/// ```
/// # tokio_test::block_on(async {
/// let mut txt = async_zeroconf::TxtRecord::new();
/// txt.add("key1".to_string(), "value1".to_string());
/// txt.add("key2".to_string(), "value2".to_string());
/// let service_ref = async_zeroconf::Service::new_with_txt("Server", "_http._tcp", 80, txt)
///                       .publish()?;
/// # Ok::<(), async_zeroconf::ZeroconfError>(())
/// # });
/// ```
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TxtRecord {
    records: HashMap<String, Vec<u8>>,
}

impl TxtRecord {
    /// Create a new TXT record collection
    pub fn new() -> Self {
        TxtRecord {
            records: HashMap::new(),
        }
    }

    /// Add an entry from a string
    pub fn add(&mut self, k: String, v: String) {
        self.records.insert(k, v.as_bytes().to_vec());
    }

    /// Add an entry from a slice of u8's
    pub fn add_vec(&mut self, k: String, v: Vec<u8>) {
        self.records.insert(k, v);
    }

    /// Get Iterator
    ///
    /// # Examples
    /// ```
    /// let mut txt = async_zeroconf::TxtRecord::new();
    /// txt.add("key".to_string(), "value".to_string());
    /// // Iterator
    /// let iter = txt.iter();
    /// for (k, v) in iter {
    ///     println!("{}, {:?}", k, v);
    /// }
    /// ```
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Vec<u8>)> {
        self.records.iter()
    }

    /// Get Iterator including conversion to string. As the conversion to a
    /// UTF-8 string could fail the value is returned as a `Result`.
    ///
    /// # Examples
    /// ```
    /// let mut txt = async_zeroconf::TxtRecord::new();
    /// txt.add("key".to_string(), "value".to_string());
    /// // String iterator
    /// let iter = txt.iter_string();
    /// for (k, v) in iter {
    ///     match v {
    ///         Ok(v) => println!("{}, {}", k, v),
    ///         Err(_) => println!("{} not valid UTF-8", k)
    ///     }
    /// }
    /// ```
    pub fn iter_string(&self) -> impl Iterator<Item = (&String, Result<&str, Utf8Error>)> {
        self.records
            .iter()
            .map(|(k, v)| (k, std::str::from_utf8(v)))
    }

    /// Get Iterator including conversion to string. If the conversion to UTF-8
    /// fails, '�' will be returned instead.
    ///
    /// # Examples
    /// ```
    /// let mut txt = async_zeroconf::TxtRecord::new();
    /// txt.add("key".to_string(), "value".to_string());
    /// // String iterator
    /// let iter = txt.iter_string_lossy();
    /// for (k, v) in iter {
    ///     println!("{}, {}", k, v);
    /// }
    /// ```
    pub fn iter_string_lossy(&self) -> impl Iterator<Item = (&String, &str)> {
        self.records
            .iter()
            .map(|(k, v)| (k, std::str::from_utf8(v).unwrap_or("�")))
    }

    /// Validate if this TXT record collection contains all valid values.
    /// This checks that the key is 9 characters or less, the value is 255
    /// characters or less and that the key only has printable ASCII characters
    /// excluding '='.
    ///
    /// # Examples
    /// ```
    /// let mut valid_txt = async_zeroconf::TxtRecord::new();
    /// valid_txt.add("key".to_string(), "value".to_string());
    /// assert!(valid_txt.validate().is_ok());
    ///
    /// let mut invalid_txt = async_zeroconf::TxtRecord::new();
    /// invalid_txt.add("k\0".to_string(), "value".to_string());
    /// assert!(invalid_txt.validate().is_err());
    /// ```
    pub fn validate(&self) -> Result<(), ZeroconfError> {
        for (k, v) in self.iter() {
            let all_printable_ascii = k.chars().all(|c| (0x20..=0x7E).contains(&(c as u32)));
            if k.len() > 9 || v.len() > 255 || k.contains('=') || !all_printable_ascii {
                return Err(ZeroconfError::InvalidTxtRecord(format!(
                    "{}={}",
                    k,
                    String::from_utf8_lossy(v)
                )));
            }
        }
        Ok(())
    }

    /// Empty if no records are associated
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

impl Default for TxtRecord {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for TxtRecord {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut string = "{".to_string();
        let mut first = true;
        for (k, v) in self.iter_string_lossy() {
            if !first {
                string.push_str(", ");
            }
            string.push_str(format!("\"{}\": \"{}\"", k, v).as_str());
            first = false;
        }
        string.push('}');
        write!(f, "{}", string)
    }
}
