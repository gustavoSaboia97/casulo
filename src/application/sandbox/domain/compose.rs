//! Services from a docker compose file: what casulo allows them to ask for.
//!
//! The compose file comes from the repository, and the agent can edit it, so
//! it is never trusted. casulo validates the model `docker compose config`
//! prints, with an allowlist: anything it does not know is rejected.

use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};

use crate::application::shared::domain::ProjectName;

/// Keys allowed at the top of the model.
const TOP_LEVEL_KEYS: [&str; 4] = ["name", "services", "networks", "volumes"];

/// Keys allowed in a service. Anything else (`privileged`, `cap_add`,
/// `network_mode`, `build`, `secrets`, ...) is rejected.
const SERVICE_KEYS: [&str; 14] = [
    "image",
    "command",
    "entrypoint",
    "environment",
    "healthcheck",
    "depends_on",
    "restart",
    "working_dir",
    "user",
    "labels",
    "networks",
    "volumes",
    "ports",
    "stop_grace_period",
];

const HEALTHCHECK_KEYS: [&str; 7] = [
    "test",
    "interval",
    "timeout",
    "retries",
    "start_period",
    "start_interval",
    "disable",
];
const DEPENDS_ON_KEYS: [&str; 3] = ["condition", "required", "restart"];
const SERVICE_NETWORK_KEYS: [&str; 1] = ["aliases"];
const VOLUME_MOUNT_KEYS: [&str; 5] = ["type", "source", "target", "read_only", "volume"];
const VOLUME_OPTION_KEYS: [&str; 1] = ["nocopy"];
const PORT_KEYS: [&str; 7] = [
    "mode",
    "host_ip",
    "target",
    "published",
    "protocol",
    "name",
    "app_protocol",
];
const NETWORK_KEYS: [&str; 6] = ["name", "ipam", "driver", "internal", "labels", "external"];
const VOLUME_KEYS: [&str; 4] = ["name", "driver", "labels", "external"];

/// Addresses a port may be published on: the host's loopback only, so the
/// services are not reachable from the rest of the network.
const LOOPBACK_ADDRESSES: [&str; 2] = ["127.0.0.1", "::1"];

/// The compose project name casulo uses for `project`. casulo always sets it,
/// so nothing in the repository (such as `.env`) can choose another one.
pub fn compose_project_name(project: &ProjectName) -> String {
    format!("casulo-{project}")
}

/// Whether the compose file text has an `include` key. `docker compose config`
/// fetches remote includes (git, OCI) before casulo could see them, so a file
/// with one is rejected before `config` runs. It matches `include:` at any
/// indentation and quoted or not, so a few harmless files are rejected too.
pub fn has_include(text: &str) -> bool {
    text.lines().any(|line| {
        let key = line
            .trim_start()
            .trim_start_matches(['-', '{', ',', ' '])
            .trim_start_matches(['"', '\'']);
        key.strip_prefix("include")
            .map(|rest| rest.trim_start_matches(['"', '\'']).trim_start())
            .is_some_and(|rest| rest.starts_with(':'))
    })
}

/// A validated compose model: what the services are allowed to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposeModel {
    /// The services, sorted by name.
    pub services: Vec<ComposeService>,
    /// The Docker name of the network the agent joins: the project's
    /// `default` network, else its first one.
    pub network: Option<String>,
}

/// One validated service, as shown to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposeService {
    /// Service name, which is also its host name on the network.
    pub name: String,
    /// Image it runs.
    pub image: String,
    /// Published ports, such as `127.0.0.1:5432 -> 5432/tcp`.
    pub ports: Vec<String>,
    /// Named volumes, such as `pgdata -> /var/lib/postgresql/data`.
    pub volumes: Vec<String>,
    /// Names of its environment variables. Values are never kept, since they
    /// often hold passwords.
    pub environment: Vec<String>,
}

impl ComposeModel {
    /// Validates the JSON printed by `docker compose config --format json`.
    /// Every problem is reported at once, each naming the service and field.
    pub fn validate(json: &str) -> Result<Self> {
        let model: Value = serde_json::from_str(json)
            .context("cannot read the output of docker compose config")?;
        let Some(top) = model.as_object() else {
            bail!("the output of docker compose config is not an object");
        };
        let mut problems = Vec::new();
        check_keys(top, &TOP_LEVEL_KEYS, "the compose file", &mut problems);

        let networks = object_or_empty(top.get("networks"));
        for (name, network) in &networks {
            check_network(name, network, &mut problems);
        }
        for (name, volume) in &object_or_empty(top.get("volumes")) {
            check_volume(name, volume, &mut problems);
        }

        let mut services = Vec::new();
        let declared = object_or_empty(top.get("services"));
        if declared.is_empty() {
            problems.push("the compose file has no services".to_string());
        }
        for (name, service) in &declared {
            if let Some(service) = check_service(name, service, &networks, &mut problems) {
                services.push(service);
            }
        }

        if !problems.is_empty() {
            bail!(
                "the compose file asks for things casulo does not allow:\n  - {}\n\
                 casulo allows images, environment, health checks, named volumes, ports on 127.0.0.1 and the project's own networks",
                problems.join("\n  - ")
            );
        }
        let network = networks
            .get("default")
            .or_else(|| networks.values().next())
            .and_then(|network| network.get("name"))
            .and_then(Value::as_str)
            .map(str::to_string);
        Ok(Self { services, network })
    }

    /// The model as lines a person can review. Environment values are
    /// shown as `***`.
    pub fn summary(&self) -> String {
        let mut text = String::new();
        for service in &self.services {
            text.push_str(&format!("service {}\n", service.name));
            for line in service.details() {
                text.push_str(&format!("  {line}\n"));
            }
        }
        text
    }

    /// What changed from `previous` to this model, for the user to approve.
    /// Added lines start with `+`, removed ones with `-`. A change that is
    /// not shown (such as an environment value) is still reported.
    pub fn changes_since(&self, previous: &ComposeModel) -> String {
        let mut text = String::new();
        let mut names: Vec<&str> = previous
            .services
            .iter()
            .chain(&self.services)
            .map(|service| service.name.as_str())
            .collect();
        names.sort();
        names.dedup();
        for name in names {
            match (previous.service(name), self.service(name)) {
                (None, Some(added)) => {
                    text.push_str(&format!("+ service {name}\n"));
                    for line in added.details() {
                        text.push_str(&format!("+   {line}\n"));
                    }
                }
                (Some(_), None) => text.push_str(&format!("- service {name}\n")),
                (Some(before), Some(after)) => {
                    let before_lines = before.details();
                    let after_lines = after.details();
                    let removed: Vec<&String> = before_lines
                        .iter()
                        .filter(|line| !after_lines.contains(line))
                        .collect();
                    let added: Vec<&String> = after_lines
                        .iter()
                        .filter(|line| !before_lines.contains(line))
                        .collect();
                    if removed.is_empty() && added.is_empty() {
                        continue;
                    }
                    text.push_str(&format!("~ service {name}\n"));
                    for line in removed {
                        text.push_str(&format!("-   {line}\n"));
                    }
                    for line in added {
                        text.push_str(&format!("+   {line}\n"));
                    }
                }
                (None, None) => {}
            }
        }
        if text.is_empty() {
            text.push_str("~ settings that are not shown changed, such as environment values\n");
        }
        text
    }

    fn service(&self, name: &str) -> Option<&ComposeService> {
        self.services.iter().find(|service| service.name == name)
    }
}

impl ComposeService {
    /// One line per detail a person reviews: image, ports, volumes and
    /// environment names, with values as `***`.
    pub fn details(&self) -> Vec<String> {
        let mut lines = vec![format!("image {}", self.image)];
        lines.extend(self.ports.iter().map(|port| format!("port {port}")));
        lines.extend(self.volumes.iter().map(|volume| format!("volume {volume}")));
        lines.extend(
            self.environment
                .iter()
                .map(|name| format!("env {name}=***")),
        );
        lines
    }
}

/// A JSON object, or an empty one when the value is missing or `null`.
fn object_or_empty(value: Option<&Value>) -> Map<String, Value> {
    value
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

/// Whether a value means "not set": the normalized output writes unset
/// fields as `null`, `false`, `[]` or `{}`.
fn is_empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Bool(set) => !set,
        Value::Array(items) => items.is_empty(),
        Value::Object(fields) => fields.is_empty(),
        Value::String(text) => text.is_empty(),
        Value::Number(_) => false,
    }
}

/// Reports every key of `object` that is not in `allowed` and is set.
fn check_keys(
    object: &Map<String, Value>,
    allowed: &[&str],
    place: &str,
    problems: &mut Vec<String>,
) {
    for (key, value) in object {
        if !allowed.contains(&key.as_str()) && !is_empty(value) {
            problems.push(format!("{place}: {key} is not allowed"));
        }
    }
}

fn check_network(name: &str, network: &Value, problems: &mut Vec<String>) {
    let place = format!("network \"{name}\"");
    let Some(network) = network.as_object() else {
        return;
    };
    check_keys(network, &NETWORK_KEYS, &place, problems);
    if network
        .get("external")
        .is_some_and(|value| !is_empty(value))
    {
        problems.push(format!(
            "{place}: external networks are not allowed; they would connect to another project's services"
        ));
    }
    if network.get("ipam").is_some_and(|value| !is_empty(value)) {
        problems.push(format!("{place}: ipam settings are not allowed"));
    }
    if let Some(driver) = network.get("driver").and_then(Value::as_str)
        && driver != "bridge"
    {
        problems.push(format!(
            "{place}: only the bridge driver is allowed, not {driver}"
        ));
    }
}

fn check_volume(name: &str, volume: &Value, problems: &mut Vec<String>) {
    let place = format!("volume \"{name}\"");
    let Some(volume) = volume.as_object() else {
        return;
    };
    // `driver_opts` is not allowed: with the local driver it can bind any
    // host directory (`type: none`, `o: bind`, `device: /`).
    check_keys(volume, &VOLUME_KEYS, &place, problems);
    if volume.get("external").is_some_and(|value| !is_empty(value)) {
        problems.push(format!("{place}: external volumes are not allowed"));
    }
    if let Some(driver) = volume.get("driver").and_then(Value::as_str)
        && driver != "local"
    {
        problems.push(format!(
            "{place}: only the local driver is allowed, not {driver}"
        ));
    }
}

fn check_service(
    name: &str,
    service: &Value,
    networks: &Map<String, Value>,
    problems: &mut Vec<String>,
) -> Option<ComposeService> {
    let place = format!("service \"{name}\"");
    let Some(service) = service.as_object() else {
        problems.push(format!("{place}: is not an object"));
        return None;
    };
    let before = problems.len();
    check_keys(service, &SERVICE_KEYS, &place, problems);
    if service.get("build").is_some_and(|value| !is_empty(value)) {
        problems.push(format!(
            "{place}: build is not supported yet; use a published image"
        ));
    }

    let image = service
        .get("image")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if image.is_empty() {
        problems.push(format!("{place}: needs an image"));
    }

    if let Some(healthcheck) = service.get("healthcheck").and_then(Value::as_object) {
        check_keys(
            healthcheck,
            &HEALTHCHECK_KEYS,
            &format!("{place} healthcheck"),
            problems,
        );
    }
    for (dependency, settings) in &object_or_empty(service.get("depends_on")) {
        if let Some(settings) = settings.as_object() {
            check_keys(
                settings,
                &DEPENDS_ON_KEYS,
                &format!("{place} depends_on {dependency}"),
                problems,
            );
        }
    }
    for (network, settings) in &object_or_empty(service.get("networks")) {
        if !networks.contains_key(network) {
            problems.push(format!("{place}: network {network} is not declared"));
        }
        if let Some(settings) = settings.as_object() {
            check_keys(
                settings,
                &SERVICE_NETWORK_KEYS,
                &format!("{place} network {network}"),
                problems,
            );
        }
    }

    let ports = check_ports(&place, service.get("ports"), problems);
    let volumes = check_volume_mounts(&place, service.get("volumes"), problems);
    let environment: BTreeSet<String> = object_or_empty(service.get("environment"))
        .keys()
        .cloned()
        .collect();

    (problems.len() == before).then(|| ComposeService {
        name: name.to_string(),
        image,
        ports,
        volumes,
        environment: environment.into_iter().collect(),
    })
}

fn check_ports(place: &str, ports: Option<&Value>, problems: &mut Vec<String>) -> Vec<String> {
    let mut shown = Vec::new();
    for port in ports.and_then(Value::as_array).into_iter().flatten() {
        let Some(port) = port.as_object() else {
            problems.push(format!("{place}: a port is not in the expected form"));
            continue;
        };
        check_keys(port, &PORT_KEYS, &format!("{place} port"), problems);
        let target = port.get("target").map(text_of).unwrap_or_default();
        let published = port.get("published").map(text_of).unwrap_or_default();
        let protocol = port
            .get("protocol")
            .and_then(Value::as_str)
            .unwrap_or("tcp");
        match port.get("host_ip").and_then(Value::as_str) {
            Some(address) if LOOPBACK_ADDRESSES.contains(&address) => {
                shown.push(format!("{address}:{published} -> {target}/{protocol}"));
            }
            _ if published.is_empty() => {}
            _ => problems.push(format!(
                "{place}: port {published} is published on every network interface; publish it on 127.0.0.1 only, such as \"127.0.0.1:{published}:{target}\""
            )),
        }
    }
    shown
}

fn check_volume_mounts(
    place: &str,
    volumes: Option<&Value>,
    problems: &mut Vec<String>,
) -> Vec<String> {
    let mut shown = Vec::new();
    for mount in volumes.and_then(Value::as_array).into_iter().flatten() {
        let Some(mount) = mount.as_object() else {
            problems.push(format!("{place}: a volume is not in the expected form"));
            continue;
        };
        let kind = mount
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let target = mount.get("target").map(text_of).unwrap_or_default();
        if kind != "volume" {
            problems.push(format!(
                "{place}: the {kind} mount on {target} is not allowed; only named volumes are, such as \"data:{target}\""
            ));
            continue;
        }
        check_keys(
            mount,
            &VOLUME_MOUNT_KEYS,
            &format!("{place} volume {target}"),
            problems,
        );
        if let Some(options) = mount.get("volume").and_then(Value::as_object) {
            check_keys(
                options,
                &VOLUME_OPTION_KEYS,
                &format!("{place} volume {target}"),
                problems,
            );
        }
        let source = mount.get("source").map(text_of).unwrap_or_default();
        if source.is_empty() {
            shown.push(format!("(anonymous) -> {target}"));
        } else {
            shown.push(format!("{source} -> {target}"));
        }
    }
    shown
}

/// A string or number as text.
fn text_of(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// The output of `docker compose config --format json` (Compose v5.6) for
    /// a postgres and a redis service.
    fn good() -> Value {
        json!({
            "name": "casulo-app",
            "networks": { "default": { "name": "casulo-app_default", "ipam": {} } },
            "services": {
                "cache": {
                    "command": ["redis-server", "--save", ""],
                    "entrypoint": null,
                    "image": "redis:7",
                    "networks": { "default": null },
                    "depends_on": { "db": { "condition": "service_healthy", "required": true } }
                },
                "db": {
                    "command": null,
                    "entrypoint": null,
                    "environment": { "POSTGRES_PASSWORD": "secret" },
                    "image": "postgres:16",
                    "networks": { "default": null },
                    "volumes": [{
                        "type": "volume", "source": "pgdata",
                        "target": "/var/lib/postgresql/data", "volume": {}
                    }],
                    "healthcheck": { "test": ["CMD", "pg_isready"], "interval": "5s" },
                    "ports": [{
                        "mode": "ingress", "host_ip": "127.0.0.1", "target": 5432,
                        "published": "5432", "protocol": "tcp"
                    }]
                }
            },
            "volumes": { "pgdata": { "name": "casulo-app_pgdata" } }
        })
    }

    fn problems_of(model: Value) -> String {
        format!(
            "{:#}",
            ComposeModel::validate(&model.to_string()).unwrap_err()
        )
    }

    #[test]
    fn accepts_images_volumes_and_loopback_ports() {
        let model = ComposeModel::validate(&good().to_string()).unwrap();

        insta::assert_snapshot!(model.summary());
        assert!(!model.summary().contains("secret"));
        assert_eq!(model.network.as_deref(), Some("casulo-app_default"));
    }

    #[test]
    fn rejects_an_escape_to_the_host() {
        // The normalized form of: privileged, cap_add, network_mode: host,
        // volumes ["/:/host", "./data:/data"] and ports ["8080:80"].
        let escape = json!({
            "name": "casulo-app",
            "services": { "escape": {
                "cap_add": ["SYS_ADMIN"],
                "command": null,
                "entrypoint": null,
                "image": "alpine",
                "network_mode": "host",
                "privileged": true,
                "volumes": [
                    { "type": "bind", "source": "/", "target": "/host", "bind": {} },
                    { "type": "bind", "source": "/home/you/app/data", "target": "/data", "bind": {} }
                ],
                "ports": [{ "mode": "ingress", "target": 80, "published": "8080", "protocol": "tcp" }]
            }}
        });

        insta::assert_snapshot!(problems_of(escape));
    }

    #[test]
    fn rejects_each_unsafe_field() {
        let cases = [
            (
                "/services/db/build",
                json!({ "context": "/home/you/app" }),
                "build is not supported",
            ),
            (
                "/services/db/secrets",
                json!([{ "source": "tok" }]),
                "secrets is not allowed",
            ),
            ("/services/db/pid", json!("host"), "pid is not allowed"),
            (
                "/services/db/devices",
                json!(["/dev/kvm"]),
                "devices is not allowed",
            ),
            (
                "/services/db/security_opt",
                json!(["seccomp=unconfined"]),
                "security_opt is not allowed",
            ),
            ("/services/db/image", json!(""), "needs an image"),
            (
                "/networks/default/external",
                json!(true),
                "external networks are not allowed",
            ),
            (
                "/networks/default/driver",
                json!("host"),
                "only the bridge driver",
            ),
            (
                "/volumes/pgdata/driver_opts",
                json!({ "o": "bind", "device": "/" }),
                "driver_opts is not allowed",
            ),
            (
                "/volumes/pgdata/external",
                json!(true),
                "external volumes are not allowed",
            ),
            (
                "/secrets",
                json!({ "tok": { "file": "/etc/passwd" } }),
                "secrets is not allowed",
            ),
            (
                "/services/db/volumes/0/type",
                json!("tmpfs"),
                "the tmpfs mount",
            ),
            (
                "/services/db/ports/0/host_ip",
                json!("0.0.0.0"),
                "every network interface",
            ),
            (
                "/services/db/healthcheck/privileged",
                json!(true),
                "privileged is not allowed",
            ),
        ];
        for (pointer, value, expected) in cases {
            let mut model = good();
            let (parent, key) = pointer.rsplit_once('/').unwrap();
            let target = if parent.is_empty() {
                &mut model
            } else {
                model.pointer_mut(parent).unwrap()
            };
            target
                .as_object_mut()
                .unwrap()
                .insert(key.to_string(), value);

            let problems = problems_of(model);

            assert!(problems.contains(expected), "{pointer}: {problems}");
        }
    }

    #[test]
    fn tolerates_unset_fields() {
        let mut model = good();
        let db = model
            .pointer_mut("/services/db")
            .unwrap()
            .as_object_mut()
            .unwrap();
        db.insert("privileged".to_string(), json!(false));
        db.insert("cap_add".to_string(), json!([]));
        db.insert("network_mode".to_string(), json!(""));
        db.insert("secrets".to_string(), json!(null));

        assert!(ComposeModel::validate(&model.to_string()).is_ok());
    }

    #[test]
    fn rejects_undeclared_networks_and_no_services() {
        let mut model = good();
        model
            .pointer_mut("/services/db/networks")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("other".to_string(), Value::Null);
        assert!(problems_of(model).contains("network other is not declared"));

        assert!(problems_of(json!({ "name": "x", "services": {} })).contains("no services"));
        assert!(ComposeModel::validate("not json").is_err());
    }

    #[test]
    fn finds_include_keys() {
        for text in [
            "include:\n  - other.yml\n",
            "  include:\n",
            "\"include\": [\"x\"]\n",
            "{\"include\": [\"oci://x\"], \"services\": {}}",
            "include :\n",
        ] {
            assert!(has_include(text), "{text:?}");
        }
        for text in [
            "services:\n  db:\n    image: postgres\n",
            "# include: commented? still a line, but starts with #\n",
            "x-included: true\n",
        ] {
            assert!(!has_include(text), "{text:?}");
        }
    }

    #[test]
    fn changes_show_added_removed_and_changed_services() {
        let before = ComposeModel::validate(&good().to_string()).unwrap();
        let mut changed = good();
        changed["services"]["db"]["image"] = json!("postgres:17");
        changed["services"]["db"]["environment"]["PGUSER"] = json!("admin");
        changed["services"].as_object_mut().unwrap().remove("cache");
        changed["services"]["mail"] = json!({ "image": "mailhog/mailhog" });
        let after = ComposeModel::validate(&changed.to_string()).unwrap();

        insta::assert_snapshot!(after.changes_since(&before));
    }

    #[test]
    fn hidden_changes_are_still_reported() {
        let before = ComposeModel::validate(&good().to_string()).unwrap();
        let mut changed = good();
        changed["services"]["db"]["environment"]["POSTGRES_PASSWORD"] = json!("other");
        let after = ComposeModel::validate(&changed.to_string()).unwrap();

        let changes = after.changes_since(&before);

        assert!(changes.contains("not shown changed"));
        assert!(!changes.contains("other"));
    }

    #[test]
    fn project_name_is_chosen_by_casulo() {
        let project = ProjectName::parse("app").unwrap();

        assert_eq!(compose_project_name(&project), "casulo-app");
    }
}
