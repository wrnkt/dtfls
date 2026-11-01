//! OS / host detection.

/// Platform as `(os, distro)`, e.g. `("darwin", None)` or `("linux", Some("ubuntu"))`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Platform {
    pub os: String,
    pub distro: Option<String>,
}

impl Platform {
    pub fn new(os: &str, distro: Option<&str>) -> Self {
        Platform {
            os: os.to_string(),
            distro: distro.map(str::to_string),
        }
    }

    pub fn detect() -> Self {
        match std::env::consts::OS {
            "macos" => Platform::new("darwin", None),
            "linux" => Platform {
                os: "linux".into(),
                distro: linux_distro(),
            },
            other => Platform::new(other, None),
        }
    }

    /// True if an entry's optional `os` / `distro` filters match this platform.
    /// Empty strings count as "no filter".
    pub fn matches(&self, os: Option<&str>, distro: Option<&str>) -> bool {
        if let Some(os) = os.filter(|s| !s.is_empty()) {
            if os != self.os {
                return false;
            }
        }
        if let Some(distro) = distro.filter(|s| !s.is_empty()) {
            if Some(distro) != self.distro.as_deref() {
                return false;
            }
        }
        true
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.distro {
            Some(d) => write!(f, "{} / {d}", self.os),
            None => write!(f, "{}", self.os),
        }
    }
}

fn linux_distro() -> Option<String> {
    let text = std::fs::read_to_string("/etc/os-release").ok()?;
    parse_os_release_id(&text)
}

fn parse_os_release_id(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.strip_prefix("ID="))
        .map(|v| v.trim().trim_matches('"').to_lowercase())
}

/// Short hostname, no domain suffix.
pub fn hostname() -> String {
    let full = gethostname::gethostname().to_string_lossy().into_owned();
    full.split('.').next().unwrap_or_default().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_os_release_id() {
        let text = "NAME=\"Fedora Linux\"\nID=fedora\nVERSION_ID=40\n";
        assert_eq!(parse_os_release_id(text).as_deref(), Some("fedora"));
        assert_eq!(
            parse_os_release_id("ID=\"Ubuntu\"\n").as_deref(),
            Some("ubuntu")
        );
        assert_eq!(parse_os_release_id("ID_LIKE=debian\n"), None);
    }

    #[test]
    fn filters_by_os_and_distro() {
        let p = Platform::new("linux", Some("fedora"));
        assert!(p.matches(None, None));
        assert!(p.matches(Some("linux"), Some("fedora")));
        assert!(p.matches(Some(""), Some("")));
        assert!(!p.matches(Some("darwin"), None));
        assert!(!p.matches(Some("linux"), Some("ubuntu")));
        assert!(!Platform::new("darwin", None).matches(None, Some("fedora")));
    }
}
