//! Mark-of-the-Web: tagging a finished file as downloaded from the internet,
//! as browsers do, so SmartScreen, Office protected view and antivirus give
//! it the same extra checks.

use std::io;
use std::path::Path;

/// Mark `path` as downloaded from `url` with a `Zone.Identifier` stream.
/// Fails where the file system has no alternate data streams (FAT32, exFAT,
/// some network shares).
#[cfg(windows)]
pub(super) fn mark(path: &Path, url: &str) -> io::Result<()> {
    let mut stream = path.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    std::fs::write(stream, zone_identifier(url))
}

/// Only Windows has Mark-of-the-Web.
#[cfg(not(windows))]
pub(super) fn mark(_path: &Path, _url: &str) -> io::Result<()> {
    Ok(())
}

/// The `Zone.Identifier` text for a file from `url`: the Internet zone (3)
/// and the URL it came from, minus its login and query (which often carries
/// a token).
#[cfg(any(windows, test))]
fn zone_identifier(url: &str) -> String {
    let mut text = String::from("[ZoneTransfer]\r\nZoneId=3\r\n");
    if let Some(host_url) = super::redact::strip_url_secrets(url) {
        text.push_str(&format!("HostUrl={host_url}\r\n"));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::zone_identifier;

    #[test]
    fn zone_identifier_names_the_internet_zone_and_a_clean_url() {
        assert_eq!(
            zone_identifier("https://u:pw@example.com/f.zip?token=SECRET"),
            "[ZoneTransfer]\r\nZoneId=3\r\nHostUrl=https://example.com/f.zip\r\n"
        );
        assert_eq!(
            zone_identifier("not a url"),
            "[ZoneTransfer]\r\nZoneId=3\r\n"
        );
    }
}
