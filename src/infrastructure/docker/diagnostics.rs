//! What Docker says about itself, and how to install, start, permit and
//! upgrade it on each system.

use crate::application::doctor::domain::{Fix, HostOs, LinuxDistro, ServerStatus};

const MACOS_DOCS: &str = "https://docs.docker.com/desktop/setup/install/mac-install/";
const COLIMA_DOCS: &str = "https://github.com/abiosoft/colima";
const WINDOWS_DOCS: &str = "https://docs.docker.com/desktop/setup/install/windows-install/";
const LINUX_DOCS: &str = "https://docs.docker.com/engine/install/";
const POST_INSTALL_DOCS: &str = "https://docs.docker.com/engine/install/linux-postinstall/";
const ROOTLESS_DOCS: &str = "https://docs.docker.com/engine/security/rootless/";

/// Turns the result of `docker info --format {{.ServerVersion}}` into a status.
pub(super) fn classify_info(success: bool, stdout: &str, stderr: &str) -> ServerStatus {
    let version = stdout.trim();
    if success && !version.is_empty() {
        return ServerStatus::Reachable {
            version: version.to_string(),
        };
    }
    let message = first_line(stderr);
    if stderr.to_lowercase().contains("permission denied") {
        ServerStatus::PermissionDenied { message }
    } else {
        ServerStatus::Unreachable { message }
    }
}

pub(super) fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("no answer from the daemon")
        .to_string()
}

pub(super) fn install_fixes(os: HostOs) -> Vec<Fix> {
    match os {
        HostOs::MacOs => vec![
            Fix::commands(
                "install Colima and the Docker CLI with Homebrew (free and open source)",
                &["brew install colima docker", "colima start"],
                COLIMA_DOCS,
            ),
            Fix::commands(
                "install Docker Desktop with Homebrew (paid subscription required for larger companies)",
                &["brew install --cask docker-desktop", "open -a Docker"],
                MACOS_DOCS,
            ),
        ],
        HostOs::Windows => vec![Fix::commands(
            "install Docker Desktop with winget (needs WSL 2)",
            &["winget install --exact --id Docker.DockerDesktop"],
            WINDOWS_DOCS,
        )],
        HostOs::Linux(LinuxDistro::Ubuntu) => vec![apt_install_fix("ubuntu")],
        HostOs::Linux(LinuxDistro::Debian) => vec![apt_install_fix("debian")],
        HostOs::Linux(LinuxDistro::Fedora) => vec![Fix::commands(
            "install Docker Engine from Docker's official repository with dnf",
            &[
                "sudo dnf config-manager addrepo --from-repofile https://download.docker.com/linux/fedora/docker-ce.repo",
                "sudo dnf install docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin",
                "sudo systemctl enable --now docker",
            ],
            "https://docs.docker.com/engine/install/fedora/",
        )],
        HostOs::Linux(LinuxDistro::Other) | HostOs::Other => vec![Fix::manual(
            "install Docker Engine following the official guide for your system",
            LINUX_DOCS,
        )],
    }
}

/// Docker's official apt repository, as documented for Debian and Ubuntu.
pub(super) fn apt_install_fix(distro: &str) -> Fix {
    // Ubuntu derivatives (Linux Mint, Pop!_OS) need the Ubuntu codename.
    let suite = if distro == "ubuntu" {
        "${UBUNTU_CODENAME:-$VERSION_CODENAME}"
    } else {
        "$VERSION_CODENAME"
    };
    let sources = format!(
        "sudo tee /etc/apt/sources.list.d/docker.sources <<EOF\n\
         Types: deb\n\
         URIs: https://download.docker.com/linux/{distro}\n\
         Suites: $(. /etc/os-release && echo \"{suite}\")\n\
         Components: stable\n\
         Architectures: $(dpkg --print-architecture)\n\
         Signed-By: /etc/apt/keyrings/docker.asc\n\
         EOF"
    );
    let key = format!(
        "sudo curl -fsSL https://download.docker.com/linux/{distro}/gpg -o /etc/apt/keyrings/docker.asc"
    );
    Fix {
        summary: "install Docker Engine from Docker's official repository with apt".to_string(),
        commands: vec![
            "sudo apt update".to_string(),
            "sudo apt install ca-certificates curl".to_string(),
            "sudo install -m 0755 -d /etc/apt/keyrings".to_string(),
            key,
            "sudo chmod a+r /etc/apt/keyrings/docker.asc".to_string(),
            sources,
            "sudo apt update".to_string(),
            "sudo apt install docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin"
                .to_string(),
        ],
        docs_url: format!("https://docs.docker.com/engine/install/{distro}/"),
    }
}

pub(super) fn start_fixes(os: HostOs) -> Vec<Fix> {
    match os {
        HostOs::MacOs => vec![
            Fix::commands("start Colima", &["colima start"], COLIMA_DOCS),
            Fix::commands("start Docker Desktop", &["open -a Docker"], MACOS_DOCS),
        ],
        HostOs::Windows => vec![Fix::manual(
            "start Docker Desktop from the Start menu and wait until it says it is running",
            WINDOWS_DOCS,
        )],
        HostOs::Linux(LinuxDistro::Other) | HostOs::Other => vec![Fix::manual(
            "start the Docker service as your system's guide describes",
            LINUX_DOCS,
        )],
        HostOs::Linux(_) => vec![Fix::commands(
            "start the Docker service now and at every boot",
            &["sudo systemctl enable --now docker"],
            POST_INSTALL_DOCS,
        )],
    }
}

pub(super) fn permission_fixes(os: HostOs) -> Vec<Fix> {
    match os {
        HostOs::Linux(_) => vec![
            Fix::commands(
                "add your user to the docker group, then log out and back in \
                 (warning: this group grants root-level access to the machine)",
                &[
                    "sudo groupadd -f docker",
                    "sudo usermod -aG docker \"$USER\"",
                ],
                POST_INSTALL_DOCS,
            ),
            Fix::manual(
                "or run Docker in rootless mode, which does not need the docker group",
                ROOTLESS_DOCS,
            ),
        ],
        HostOs::MacOs | HostOs::Windows | HostOs::Other => vec![Fix::manual(
            "restart Docker and check that your user may use it",
            LINUX_DOCS,
        )],
    }
}

pub(super) fn upgrade_fixes(os: HostOs) -> Vec<Fix> {
    match os {
        HostOs::MacOs => vec![
            Fix::commands(
                "upgrade Colima and the Docker CLI",
                &["brew upgrade colima docker", "colima restart"],
                COLIMA_DOCS,
            ),
            Fix::commands(
                "upgrade Docker Desktop",
                &["brew upgrade --cask docker-desktop"],
                MACOS_DOCS,
            ),
        ],
        HostOs::Windows => vec![Fix::commands(
            "upgrade Docker Desktop with winget",
            &["winget upgrade --exact --id Docker.DockerDesktop"],
            WINDOWS_DOCS,
        )],
        // Installing from Docker's repository also upgrades an older package.
        HostOs::Linux(_) | HostOs::Other => install_fixes(os),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reachable_daemon_reports_its_version() {
        let status = classify_info(true, "28.3.2\n", "");

        assert_eq!(
            status,
            ServerStatus::Reachable {
                version: "28.3.2".to_string()
            }
        );
    }

    #[test]
    fn permission_error_is_recognized() {
        let stderr = "permission denied while trying to connect to the Docker daemon socket at unix:///var/run/docker.sock\n";

        assert!(matches!(
            classify_info(false, "", stderr),
            ServerStatus::PermissionDenied { .. }
        ));
    }

    #[test]
    fn stopped_daemon_is_unreachable_with_the_first_error_line() {
        let stderr = "\nCannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?\nmore\n";

        assert_eq!(
            classify_info(false, "", stderr),
            ServerStatus::Unreachable {
                message: "Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?".to_string()
            }
        );
    }

    #[test]
    fn empty_version_is_unreachable() {
        assert!(matches!(
            classify_info(true, "  \n", ""),
            ServerStatus::Unreachable { .. }
        ));
    }

    #[test]
    fn macos_offers_colima_first_then_docker_desktop() {
        let fixes = install_fixes(HostOs::MacOs);

        assert_eq!(fixes.len(), 2);
        assert_eq!(
            fixes[0].commands,
            ["brew install colima docker", "colima start"]
        );
        assert_eq!(fixes[1].commands[0], "brew install --cask docker-desktop");
        assert!(fixes.iter().all(|fix| !fix.uses_sudo()));
    }

    #[test]
    fn windows_uses_winget() {
        let fixes = install_fixes(HostOs::Windows);

        assert_eq!(
            fixes[0].commands,
            ["winget install --exact --id Docker.DockerDesktop"]
        );
    }

    #[test]
    fn debian_and_ubuntu_use_their_own_docker_repository() {
        let ubuntu = &install_fixes(HostOs::Linux(LinuxDistro::Ubuntu))[0];
        let debian = &install_fixes(HostOs::Linux(LinuxDistro::Debian))[0];

        assert!(ubuntu.uses_sudo());
        assert!(
            ubuntu
                .commands
                .iter()
                .any(|command| command.contains("download.docker.com/linux/ubuntu/gpg"))
        );
        assert!(
            ubuntu
                .commands
                .iter()
                .any(|command| command.contains("UBUNTU_CODENAME"))
        );
        assert!(
            debian
                .commands
                .iter()
                .any(|command| command.contains("download.docker.com/linux/debian/gpg"))
        );
        assert!(
            !debian
                .commands
                .iter()
                .any(|command| command.contains("UBUNTU_CODENAME"))
        );
        assert!(
            debian
                .commands
                .last()
                .is_some_and(|command| command.starts_with("sudo apt install docker-ce"))
        );
    }

    #[test]
    fn fedora_uses_dnf_and_starts_the_service() {
        let fix = &install_fixes(HostOs::Linux(LinuxDistro::Fedora))[0];

        assert!(fix.commands[0].starts_with("sudo dnf config-manager addrepo"));
        assert_eq!(fix.commands[2], "sudo systemctl enable --now docker");
    }

    #[test]
    fn unknown_systems_only_link_the_docs() {
        for os in [HostOs::Linux(LinuxDistro::Other), HostOs::Other] {
            let fixes = install_fixes(os);

            assert_eq!(fixes.len(), 1);
            assert!(fixes[0].commands.is_empty());
            assert_eq!(fixes[0].docs_url, LINUX_DOCS);
        }
    }

    #[test]
    fn linux_permission_fix_warns_about_root_access() {
        let fixes = permission_fixes(HostOs::Linux(LinuxDistro::Ubuntu));

        assert!(fixes[0].summary.contains("root-level access"));
        assert!(
            fixes[0]
                .commands
                .iter()
                .any(|command| command.contains("usermod -aG docker"))
        );
        assert_eq!(fixes[1].docs_url, ROOTLESS_DOCS);
    }

    #[test]
    fn every_fix_links_official_docs() {
        let systems = [
            HostOs::MacOs,
            HostOs::Windows,
            HostOs::Linux(LinuxDistro::Debian),
            HostOs::Linux(LinuxDistro::Ubuntu),
            HostOs::Linux(LinuxDistro::Fedora),
            HostOs::Linux(LinuxDistro::Other),
            HostOs::Other,
        ];
        for os in systems {
            let fixes = [
                install_fixes(os),
                start_fixes(os),
                permission_fixes(os),
                upgrade_fixes(os),
            ]
            .concat();

            assert!(!fixes.is_empty());
            assert!(fixes.iter().all(|fix| fix.docs_url.starts_with("https://")));
        }
    }
}
