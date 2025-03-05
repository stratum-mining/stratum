//! This file contains utility functions for working with URLs.

/// Checks if a given string is a valid URL.
///
/// Very naive and simple check, only checks if the URL starts with http:// or https:// and if the
/// port is a valid number.
pub fn is_valid_url(url_str: &str) -> bool {
    extract_host_and_port_from_url(url_str).is_ok()
}

fn is_valid_url_scheme(url_str: &str) -> bool {
    url_str.starts_with("http://") || url_str.starts_with("https://") && url_str.len() > 8
}

fn extract_host_and_port_from_url(url_str: &str) -> Result<(String, u16), &'static str> {
    if !is_valid_url_scheme(url_str) {
        return Err("Invalid URL format: must start with http:// or https://");
    }
    let mut parts = url_str.split("://");
    let scheme = parts.next().unwrap();
    let rest = parts.next().unwrap();
    let mut host_port = rest.split('/');
    let host_and_port = host_port.next().unwrap();
    let mut host_port_parts = host_and_port.split(':');
    let host = host_port_parts.next().unwrap().to_string();
    let port_str = host_port_parts.next();
    let port: u16 = match port_str {
        Some(p) => match p.parse() {
            Ok(port) => port,
            Err(_) => return Err("Invalid port number"),
        },
        None => {
            if scheme == "https" {
                443
            } else {
                80
            }
        }
    };

    Ok((host, port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_url_basic() {
        assert_eq!(is_valid_url_scheme("https://www.example.com"), true);
        assert_eq!(is_valid_url_scheme("http://127.0.0.1:8080"), true);
        assert_eq!(is_valid_url_scheme("invalid-url"), false);
    }

    #[test]
    fn test_extract_host_and_port_from_url() {
        let (host, port) = extract_host_and_port_from_url("http://127.0.0.1:8080").unwrap();
        assert_eq!(host, "127.0.0.1");
        assert_eq!(port, 8080);

        let (host, port) = extract_host_and_port_from_url("http://example.com").unwrap(); // No explicit port
        assert_eq!(host, "example.com");
        assert_eq!(port, 80); // Default HTTP port

        let (host, port) = extract_host_and_port_from_url("https://example.com").unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 443); // Default HTTPS port

        let (host, port) = extract_host_and_port_from_url("https://example.com:8443").unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 8443);
    }
}
