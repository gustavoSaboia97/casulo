//! MCP servers: tools the agent can call, written once in casulo's format
//! and passed to each harness its own way.

use anyhow::{Result, bail};
use serde::Deserialize;
use serde_json::json;

use super::harness::is_environment_variable_name;
use crate::application::shared::domain::ResourceName;

/// A validated MCP server from `~/.casulo/agent/mcp/<name>.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServer {
    /// Name, taken from the file name.
    pub name: ResourceName,
    /// How the agent reaches the server.
    pub transport: McpTransport,
}

/// How the agent reaches an MCP server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpTransport {
    /// A program the agent starts in the container.
    Command {
        /// Program and arguments.
        command: Vec<String>,
        /// Names of variables forwarded from the host into the server.
        env: Vec<String>,
    },
    /// A server over HTTP.
    Url {
        /// Address of the server.
        url: String,
        /// Name of the variable holding a token, sent as `Authorization: Bearer`.
        token_env: Option<String>,
        /// Headers whose value is a variable, as `(header, variable)`. Only
        /// casulo sets them; a server file cannot.
        env_headers: Vec<(String, String)>,
    },
}

/// The file exactly as written in TOML, before validation.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawServer {
    command: Option<Vec<String>>,
    #[serde(default)]
    env: Vec<String>,
    url: Option<String>,
    token_env: Option<String>,
}

impl McpServer {
    /// Parses a server file. `file_name` is its name without `.toml`, which
    /// becomes the server name.
    pub fn parse(file_name: &str, text: &str) -> Result<Self> {
        let name = ResourceName::parse(file_name)?;
        let raw: RawServer = toml::from_str(text)?;
        for variable in raw.env.iter().chain(&raw.token_env) {
            if !is_environment_variable_name(variable) {
                bail!(
                    "invalid variable name in env or token_env; use names like GITHUB_TOKEN, never the value"
                );
            }
        }
        let transport = match (raw.command, raw.url) {
            (Some(command), None) => {
                if command.is_empty() || command.iter().any(|part| part.is_empty()) {
                    bail!("command needs a program and no empty arguments");
                }
                if raw.token_env.is_some() {
                    bail!("token_env only works with url; use env to pass variables to a command");
                }
                McpTransport::Command {
                    command,
                    env: raw.env,
                }
            }
            (None, Some(url)) => {
                if !(url.starts_with("https://") || url.starts_with("http://")) {
                    bail!("url must start with https:// or http://");
                }
                if !raw.env.is_empty() {
                    bail!("env only works with command; use token_env to send a token to a url");
                }
                McpTransport::Url {
                    url,
                    token_env: raw.token_env,
                    env_headers: Vec::new(),
                }
            }
            _ => bail!("set exactly one of command or url"),
        };
        Ok(Self { name, transport })
    }

    /// Names of the host variables this server needs in the container.
    pub fn environment_names(&self) -> Vec<String> {
        match &self.transport {
            McpTransport::Command { env, .. } => env.clone(),
            McpTransport::Url { token_env, .. } => token_env.iter().cloned().collect(),
        }
    }
}

/// The `--mcp-config` JSON Claude Code reads. Secrets and other variables
/// appear as `${NAME}`, which Claude Code expands from its environment, so
/// no value is ever written to the file.
pub fn claude_mcp_config(servers: &[McpServer]) -> String {
    let mut entries = serde_json::Map::new();
    for server in servers {
        let entry = match &server.transport {
            McpTransport::Command { command, env } => {
                let env: serde_json::Map<String, serde_json::Value> = env
                    .iter()
                    .map(|name| (name.clone(), json!(format!("${{{name}}}"))))
                    .collect();
                json!({ "command": command[0], "args": command[1..], "env": env })
            }
            McpTransport::Url {
                url,
                token_env,
                env_headers,
            } => {
                let mut headers = serde_json::Map::new();
                if let Some(name) = token_env {
                    headers.insert(
                        "Authorization".to_string(),
                        json!(format!("Bearer ${{{name}}}")),
                    );
                }
                for (header, name) in env_headers {
                    headers.insert(header.clone(), json!(format!("${{{name}}}")));
                }
                if headers.is_empty() {
                    json!({ "type": "http", "url": url })
                } else {
                    json!({ "type": "http", "url": url, "headers": headers })
                }
            }
        };
        entries.insert(server.name.to_string(), entry);
    }
    let config = json!({ "mcpServers": entries });
    serde_json::to_string_pretty(&config).expect("JSON values always serialize") + "\n"
}

/// The `-c mcp_servers.<name>.<key>=<value>` arguments Codex reads. Values
/// are TOML, as Codex expects, and only name variables.
pub fn codex_config_overrides(servers: &[McpServer]) -> Vec<String> {
    let mut arguments = Vec::new();
    for server in servers {
        let mut set = |key: &str, value: toml::Value| {
            arguments.push("-c".to_string());
            arguments.push(format!("mcp_servers.{}.{key}={value}", server.name));
        };
        match &server.transport {
            McpTransport::Command { command, env } => {
                set("command", toml::Value::from(command[0].as_str()));
                set("args", toml::Value::from(command[1..].to_vec()));
                if !env.is_empty() {
                    set("env_vars", toml::Value::from(env.clone()));
                }
            }
            // Codex gets no variable headers yet: only Claude Code has
            // session hooks, and Codex runs as it did.
            McpTransport::Url { url, token_env, .. } => {
                set("url", toml::Value::from(url.as_str()));
                if let Some(name) = token_env {
                    set("bearer_token_env_var", toml::Value::from(name.as_str()));
                }
            }
        }
    }
    arguments
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTEXT7: &str = r#"
command = ["npx", "-y", "@upstash/context7-mcp"]
env = ["CONTEXT7_TOKEN"]
"#;

    const GITHUB: &str = r#"
url = "https://api.githubcopilot.com/mcp/"
token_env = "GITHUB_TOKEN"
"#;

    fn servers() -> Vec<McpServer> {
        vec![
            McpServer::parse("context7", CONTEXT7).unwrap(),
            McpServer::parse("github", GITHUB).unwrap(),
            McpServer::parse("docs", "url = \"http://localhost:3000/mcp\"\n").unwrap(),
        ]
    }

    #[test]
    fn parses_command_and_url_servers() {
        let servers = servers();

        assert_eq!(
            servers[0].transport,
            McpTransport::Command {
                command: vec![
                    "npx".to_string(),
                    "-y".to_string(),
                    "@upstash/context7-mcp".to_string()
                ],
                env: vec!["CONTEXT7_TOKEN".to_string()],
            }
        );
        assert_eq!(servers[1].environment_names(), ["GITHUB_TOKEN"]);
        assert!(servers[2].environment_names().is_empty());
    }

    #[test]
    fn rejects_invalid_servers() {
        let invalid = [
            ("a", ""),
            ("a", "command = [\"x\"]\nurl = \"https://x\"\n"),
            ("a", "command = []\n"),
            ("a", "command = [\"\"]\n"),
            ("a", "command = [\"x\"]\nenv = [\"TOKEN=secret\"]\n"),
            ("a", "command = [\"x\"]\ntoken_env = \"TOKEN\"\n"),
            ("a", "url = \"https://x\"\nenv = [\"TOKEN\"]\n"),
            ("a", "url = \"https://x\"\ntoken_env = \"ghp_abc def\"\n"),
            ("a", "url = \"file:///etc/passwd\"\n"),
            ("a", "url = \"https://x\"\nheaders = {}\n"),
            ("Bad", "url = \"https://x\"\n"),
            ("a--b", "url = \"https://x\"\n"),
        ];
        for (file_name, text) in invalid {
            assert!(
                McpServer::parse(file_name, text).is_err(),
                "should be invalid: {file_name}\n{text}"
            );
        }
    }

    #[test]
    fn claude_config_references_variables_by_name() {
        let json = claude_mcp_config(&servers());

        insta::assert_snapshot!(json);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed["mcpServers"]["context7"]["env"]["CONTEXT7_TOKEN"],
            "${CONTEXT7_TOKEN}"
        );
    }

    #[test]
    fn claude_config_sends_variable_headers_by_name() {
        // Given a server with a token and a header taken from a variable
        let server = McpServer {
            name: ResourceName::parse("memory").unwrap(),
            transport: McpTransport::Url {
                url: "http://memory:7373/mcp/claude".to_string(),
                token_env: Some("MEMORY_TOKEN".to_string()),
                env_headers: vec![("X-Casulo-Session".to_string(), "CASULO_SESSION".to_string())],
            },
        };

        // When the configs are rendered
        let json = claude_mcp_config(std::slice::from_ref(&server));
        let arguments = codex_config_overrides(&[server]);

        // Then Claude Code gets both headers by name, and Codex none
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed["mcpServers"]["memory"]["headers"],
            json!({ "Authorization": "Bearer ${MEMORY_TOKEN}", "X-Casulo-Session": "${CASULO_SESSION}" })
        );
        assert!(!arguments.join(" ").contains("CASULO_SESSION"));
    }

    #[test]
    fn codex_overrides_reference_variables_by_name() {
        let arguments = codex_config_overrides(&servers());

        insta::assert_snapshot!(arguments.join("\n"));
    }

    #[test]
    fn rendered_config_never_holds_a_value() {
        // Rendering takes no environment at all, so a value can only appear
        // by mistake as a literal. Every mention of a variable must be its name.
        let json = claude_mcp_config(&servers());
        let arguments = codex_config_overrides(&servers()).join("\n");

        for name in ["CONTEXT7_TOKEN", "GITHUB_TOKEN"] {
            for text in [&json, &arguments] {
                for (index, _) in text.match_indices(name) {
                    let before = &text[..index];
                    assert!(
                        before.ends_with("${") || before.ends_with('"'),
                        "{name} is not referenced by name in:\n{text}"
                    );
                }
            }
        }
    }
}
