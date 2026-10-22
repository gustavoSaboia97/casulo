//! `casulo run` and `casulo shell`: start the agent, or a shell, in the
//! hardened container.

use std::io::IsTerminal;
use std::process::ExitCode;

use anyhow::{Context, Result};

use super::compose::require_compose_approval;
use super::project_memory::{build_memory_image, memory_image_source};
use super::{home_and_config, runtime, warn_if_others_can_read};
use crate::application::project::domain::config::choose_harness;
use crate::application::project::domain::push;
use crate::application::sandbox::domain::RunSpec;
use crate::application::sandbox::ports::{Containers, Images};
use crate::application::sandbox::services::inject::AgentSetup;
use crate::application::sandbox::services::memory_container as memory_service;
use crate::application::sandbox::services::push as push_service;
use crate::application::sandbox::services::run_plan::{self, RunPlan, RunRequest};
use crate::application::sandbox::services::{build, compose, inject, mask, run};
use crate::application::shared::domain::{CasuloHome, ProjectName};
use crate::cli::args::RunArgs;
use crate::cli::environment;
use crate::infrastructure::agent_files;
use crate::infrastructure::config_file;
use crate::infrastructure::docker::Docker;
use crate::infrastructure::harness_files;

/// Runs the agent, or `command` instead of it (for `casulo shell`), and
/// exits with the container's exit code.
pub fn start_container(arguments: RunArgs, command: Option<Vec<String>>) -> Result<ExitCode> {
    let (home, global) = home_and_config()?;
    let project = environment::resolve_project(&home, arguments.project.project)?;
    let project_config_file = home.project_config_file(&project);
    let project_config = config_file::load_project_config(&project_config_file)?;
    warn_if_others_can_read(&project_config_file, project_config.git.token.is_some());
    let harness_name = choose_harness(arguments.project.harness, &project_config, &global);
    let harness = harness_files::find_harness(&home.harnesses_dir(), &harness_name)?;
    let workspace = project_config.workspace_dir(&home, &project);
    let workspace_owner = environment::workspace_owner(&workspace)?;
    let setup = AgentSetup {
        subagents: agent_files::load_subagents(&home.agents_dir())?,
        mcp_servers: agent_files::load_mcp_servers(&home.mcp_dir())?,
        // Only casulo gives hooks: the run plan adds memory's.
        session_hook: None,
    };
    let runtime = runtime(&global);
    // The compose services are checked even in --dry-run, so a rejected file
    // shows up before the real run; they are only started for real.
    let compose_services = if project_config.compose.enabled {
        Some(compose::check(
            &project,
            &workspace,
            &project_config.compose,
            &runtime,
        )?)
    } else {
        None
    };
    let image = project_config.image.clone();
    let request = RunRequest {
        home: home.clone(),
        project: project.clone(),
        project_config,
        global,
        harness: harness.clone(),
        setup,
        workspace_owner,
        command,
        compose_network: compose_services
            .as_ref()
            .and_then(|checked| checked.model.network.clone()),
        // A terminal is only attached when there is one, so piping input works.
        terminal: std::io::stdin().is_terminal(),
        session_id: std::process::id(),
        memory_image: memory_service::image_tag(&memory_image_source()),
    };
    let RunPlan {
        spec,
        memory_spec,
        setup,
        push_policy,
        secret_names,
        warnings,
    } = run_plan::plan(request, &|name| std::env::var_os(name).is_some())?;
    for warning in warnings {
        eprintln!("warning: {warning}");
    }
    if let Some(policy) = &push_policy
        && push_service::take_reminder(&home, &project)?
    {
        eprintln!("warning: {}", push::push_reminder(policy));
    }

    if arguments.dry_run {
        let mut secrets_set = environment::names_set_in_environment(&secret_names);
        secrets_set.extend(spec.secret_values.iter().map(|(name, _)| name.clone()));
        if let Some(memory_spec) = &memory_spec {
            for line in memory_service::start_command_lines(&runtime, memory_spec) {
                println!("{}", line.display(&[]));
            }
        }
        for line in runtime.run_command_lines(&spec) {
            println!("{}", line.display(&secrets_set));
        }
        return Ok(ExitCode::SUCCESS);
    }
    if let Some(tag) = build::build_image_if_missing(&home, &project, &harness, &image, &runtime)? {
        eprintln!("built {tag}, which did not exist yet");
    }
    if let Some(checked) = &compose_services {
        require_compose_approval(&home, &project, checked, true)?;
        compose::start(&home, &project, checked, &runtime)?;
    }
    mask::prepare_mask_sources(&home, &project)?;
    inject::write_generated_files(&home, &project, &harness, &setup)?;
    push_service::write_push_files(&home, &project, push_policy.as_ref())?;
    // After the files above, so it can tell a mounted directory from a file.
    run::prepare_agent_config(&home, &project, &harness, &spec.mounts)?;
    if let Some(memory_spec) = &memory_spec {
        start_memory(&home, &project, &runtime, memory_spec)?;
    }
    let result = runtime.run(&spec);
    if memory_spec.is_some() {
        // A failure here must not hide the agent's exit code.
        if let Err(error) = memory_service::stop_if_last(&runtime, &project, spec.name.as_deref()) {
            eprintln!("warning: cannot stop the memory server: {error:#}");
        }
    }
    let code = result?;
    // Exit codes outside 0-255 cannot be returned, so they become a failure.
    Ok(u8::try_from(code).map_or(ExitCode::FAILURE, ExitCode::from))
}

/// Starts the project's memory container, or joins the one another session
/// started.
fn start_memory(
    home: &CasuloHome,
    project: &ProjectName,
    runtime: &Docker,
    memory_spec: &RunSpec,
) -> Result<()> {
    if !runtime.image_exists(&memory_spec.image)? {
        let tag = build_memory_image(home, runtime)?;
        eprintln!("built {tag}, which did not exist yet");
    }
    let memory_dir = home.memory_dir(project);
    std::fs::create_dir_all(&memory_dir)
        .with_context(|| format!("cannot create {}", memory_dir.display()))?;
    match memory_service::start(runtime, memory_spec)? {
        memory_service::MemoryStart::Started => eprintln!("memory: started"),
        memory_service::MemoryStart::AlreadyRunning => {
            eprintln!("memory: shared with a running session")
        }
    }
    Ok(())
}
