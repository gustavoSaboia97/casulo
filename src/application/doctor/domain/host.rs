//! Facts about the host machine: operating system, tool versions and fixes.

/// Operating system casulo is running on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostOs {
    MacOs,
    Windows,
    Linux(LinuxDistro),
    /// Any other system, for which casulo only links the documentation.
    Other,
}

impl HostOs {
    /// Builds the host OS from `std::env::consts::OS` and, on Linux, the
    /// contents of `/etc/os-release` (empty when it cannot be read).
    pub fn from_parts(os_name: &str, os_release: &str) -> Self {
        match os_name {
            "macos" => HostOs::MacOs,
            "windows" => HostOs::Windows,
            "linux" => HostOs::Linux(LinuxDistro::from_os_release(os_release)),
            _ => HostOs::Other,
        }
    }
}

/// Linux distributions casulo knows how to install Docker on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxDistro {
    Debian,
    Ubuntu,
    Fedora,
    /// Any other distribution, for which casulo only links the documentation.
    Other,
}

impl LinuxDistro {
    /// Reads the distribution from the text of `/etc/os-release`, using
    /// `ID` first and then `ID_LIKE`, so derivatives such as Linux Mint
    /// are treated like the distribution they are based on.
    pub fn from_os_release(os_release: &str) -> Self {
        let mut ids = Vec::new();
        for key in ["ID", "ID_LIKE"] {
            if let Some(value) = os_release_value(os_release, key) {
                ids.extend(value.split_whitespace().map(str::to_string));
            }
        }
        for id in ids {
            match id.as_str() {
                "ubuntu" => return LinuxDistro::Ubuntu,
                "debian" => return LinuxDistro::Debian,
                "fedora" => return LinuxDistro::Fedora,
                _ => {}
            }
        }
        LinuxDistro::Other
    }
}

/// Returns the value of `key` in os-release text, without surrounding quotes.
fn os_release_value(os_release: &str, key: &str) -> Option<String> {
    os_release.lines().find_map(|line| {
        let (line_key, value) = line.trim().split_once('=')?;
        if line_key == key {
            Some(
                value
                    .trim_matches(|character| character == '"' || character == '\'')
                    .to_lowercase(),
            )
        } else {
            None
        }
    })
}

/// A way to fix a problem on the host: shell commands to run, or only
/// documentation to read when casulo has no command for the system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    /// What the fix does, for example "install Docker Desktop with Homebrew".
    pub summary: String,
    /// Shell commands to run in order. Empty when the fix is manual.
    pub commands: Vec<String>,
    /// Official documentation for the fix.
    pub docs_url: String,
}

impl Fix {
    /// A fix made of shell commands.
    pub fn commands(summary: &str, commands: &[&str], docs_url: &str) -> Self {
        Self {
            summary: summary.to_string(),
            commands: commands.iter().map(|command| command.to_string()).collect(),
            docs_url: docs_url.to_string(),
        }
    }

    /// A fix the user does by hand, following the documentation.
    pub fn manual(summary: &str, docs_url: &str) -> Self {
        Self::commands(summary, &[], docs_url)
    }

    /// Whether any command asks for administrator rights through sudo.
    pub fn uses_sudo(&self) -> bool {
        self.commands
            .iter()
            .any(|command| command.split_whitespace().any(|word| word == "sudo"))
    }
}

/// A `major.minor` version, enough to compare tool versions.
// `PartialOrd`/`Ord` compare the fields in order, so `major` decides first,
// which is exactly how versions compare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
}

impl Version {
    /// Reads the leading `major.minor` of a version such as `28.3.2` or
    /// `24.0.7+dfsg1`. Returns `None` when there is no such prefix.
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.trim().split('.');
        let major = parts.next()?.parse().ok()?;
        let minor_part = parts.next()?;
        let minor_digits: String = minor_part
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        let minor = minor_digits.parse().ok()?;
        Some(Self { major, minor })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "{}.{}", self.major, self.minor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_os_from_its_name() {
        assert_eq!(HostOs::from_parts("macos", ""), HostOs::MacOs);
        assert_eq!(HostOs::from_parts("windows", ""), HostOs::Windows);
        assert_eq!(HostOs::from_parts("freebsd", ""), HostOs::Other);
        assert_eq!(
            HostOs::from_parts("linux", ""),
            HostOs::Linux(LinuxDistro::Other)
        );
    }

    #[test]
    fn reads_distribution_from_id() {
        let ubuntu = "NAME=\"Ubuntu\"\nID=ubuntu\nID_LIKE=debian\n";
        let debian = "PRETTY_NAME=\"Debian GNU/Linux 13\"\nID=debian\n";
        let fedora = "NAME=\"Fedora Linux\"\nID=fedora\n";

        assert_eq!(LinuxDistro::from_os_release(ubuntu), LinuxDistro::Ubuntu);
        assert_eq!(LinuxDistro::from_os_release(debian), LinuxDistro::Debian);
        assert_eq!(LinuxDistro::from_os_release(fedora), LinuxDistro::Fedora);
    }

    #[test]
    fn falls_back_to_id_like_for_derivatives() {
        let mint = "ID=linuxmint\nID_LIKE=\"ubuntu debian\"\n";
        let raspberry = "ID=raspbian\nID_LIKE=debian\n";

        assert_eq!(LinuxDistro::from_os_release(mint), LinuxDistro::Ubuntu);
        assert_eq!(LinuxDistro::from_os_release(raspberry), LinuxDistro::Debian);
    }

    #[test]
    fn unknown_distribution_is_other() {
        assert_eq!(
            LinuxDistro::from_os_release("ID=arch\n"),
            LinuxDistro::Other
        );
        assert_eq!(LinuxDistro::from_os_release(""), LinuxDistro::Other);
    }

    #[test]
    fn parses_versions_with_suffixes() {
        assert_eq!(
            Version::parse("28.3.2"),
            Some(Version {
                major: 28,
                minor: 3
            })
        );
        assert_eq!(
            Version::parse("24.0.7+dfsg1"),
            Some(Version {
                major: 24,
                minor: 0
            })
        );
        assert_eq!(
            Version::parse("25.0-rc1\n"),
            Some(Version {
                major: 25,
                minor: 0
            })
        );
    }

    #[test]
    fn rejects_text_without_a_version() {
        assert_eq!(Version::parse(""), None);
        assert_eq!(Version::parse("dev"), None);
        assert_eq!(Version::parse("28"), None);
    }

    #[test]
    fn compares_versions_by_major_then_minor() {
        let minimum = Version {
            major: 24,
            minor: 0,
        };

        assert!(
            Version {
                major: 24,
                minor: 0
            } >= minimum
        );
        assert!(
            Version {
                major: 23,
                minor: 9
            } < minimum
        );
        assert!(
            Version {
                major: 25,
                minor: 0
            } > Version {
                major: 24,
                minor: 10
            }
        );
    }

    #[test]
    fn detects_sudo_in_commands() {
        let with_sudo = Fix::commands("x", &["sudo apt install git"], "https://example.com");
        let without_sudo = Fix::commands("x", &["brew install git"], "https://example.com");

        assert!(with_sudo.uses_sudo());
        assert!(!without_sudo.uses_sudo());
        assert!(!Fix::manual("x", "https://example.com").uses_sudo());
    }
}
