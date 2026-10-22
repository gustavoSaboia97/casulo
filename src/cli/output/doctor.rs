//! Terminal output of `casulo doctor`: the report of every requirement.

use crate::application::doctor::domain::{Finding, Status};

/// The report printed by `casulo doctor`.
pub fn format_report(findings: &[Finding], runtime: &str) -> String {
    let mut report = String::new();
    for finding in findings {
        let label = finding.requirement.label(runtime);
        match &finding.status {
            Status::Ok(detail) => report.push_str(&format!("[ok]      {label}: {detail}\n")),
            Status::NotChecked => report.push_str(&format!(
                "[skipped] {label}: needs the problem above fixed first\n"
            )),
            Status::Problem(message) => {
                report.push_str(&format!("[problem] {label}: {message}\n"));
                for fix in &finding.fixes {
                    report.push_str(&format!("          fix: {}\n", fix.summary));
                    for command in &fix.commands {
                        report.push_str(&indent(command, "            $ "));
                        report.push('\n');
                    }
                    report.push_str(&format!("            docs: {}\n", fix.docs_url));
                }
            }
        }
    }
    report
}

/// Prefixes the first line of a possibly multi-line command, and aligns the rest.
pub fn indent(command: &str, prefix: &str) -> String {
    let padding = " ".repeat(prefix.len());
    command
        .lines()
        .enumerate()
        .map(|(index, line)| {
            if index == 0 {
                format!("{prefix}{line}")
            } else {
                format!("{padding}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::doctor::domain::{Fix, Requirement};

    #[test]
    fn report_shows_status_fixes_and_docs() {
        let findings = vec![
            Finding {
                requirement: Requirement::Git,
                status: Status::Ok("git version 2.50.0".to_string()),
                fixes: vec![],
            },
            Finding {
                requirement: Requirement::RuntimeCli,
                status: Status::Problem("cannot run docker".to_string()),
                fixes: vec![Fix::commands(
                    "install it",
                    &["brew install colima docker", "colima start"],
                    "https://docs.example",
                )],
            },
            Finding {
                requirement: Requirement::RuntimeDaemon,
                status: Status::NotChecked,
                fixes: vec![],
            },
        ];

        let expected = "\
[ok]      git: git version 2.50.0
[problem] docker command-line tool: cannot run docker
          fix: install it
            $ brew install colima docker
            $ colima start
            docs: https://docs.example
[skipped] docker daemon running: needs the problem above fixed first
";
        assert_eq!(format_report(&findings, "docker"), expected);
    }

    #[test]
    fn multi_line_commands_stay_aligned() {
        assert_eq!(
            indent("tee x <<EOF\na\nEOF", "$ "),
            "$ tee x <<EOF\n  a\n  EOF"
        );
    }
}
