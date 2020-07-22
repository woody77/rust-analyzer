//! Driver for rust-analyzer.
//!
//! Based on cli flags, either spawns an LSP server, or runs a batch analysis
mod args;

use std::convert::TryFrom;

use lsp_server::Connection;
use ra_project_model::ProjectManifest;
use rust_analyzer::{
    cli,
    config::{Config, LinkedProject},
    from_json, Result,
};
use vfs::AbsPathBuf;

use crate::args::HelpPrinted;

fn main() -> Result<()> {
    setup_logging()?;
    let args = match args::Args::parse()? {
        Ok(it) => it,
        Err(HelpPrinted) => return Ok(()),
    };
    match args.command {
        args::Command::RunServer => run_server()?,
        args::Command::ProcMacro => ra_proc_macro_srv::cli::run()?,

        args::Command::Parse { no_dump } => cli::parse(no_dump)?,
        args::Command::Symbols => cli::symbols()?,
        args::Command::Highlight { rainbow } => cli::highlight(rainbow)?,
        args::Command::Stats {
            randomize,
            parallel,
            memory_usage,
            only,
            with_deps,
            path,
            load_output_dirs,
            with_proc_macro,
        } => cli::analysis_stats(
            args.verbosity,
            memory_usage,
            path.as_ref(),
            only.as_ref().map(String::as_ref),
            with_deps,
            randomize,
            parallel,
            load_output_dirs,
            with_proc_macro,
        )?,
        args::Command::Bench { memory_usage, path, what, load_output_dirs, with_proc_macro } => {
            cli::analysis_bench(
                args.verbosity,
                path.as_ref(),
                what,
                memory_usage,
                load_output_dirs,
                with_proc_macro,
            )?
        }
        args::Command::Diagnostics { path, load_output_dirs, with_proc_macro, all } => {
            cli::diagnostics(path.as_ref(), load_output_dirs, with_proc_macro, all)?
        }
        args::Command::Ssr { rules } => {
            cli::apply_ssr_rules(rules)?;
        }
        args::Command::StructuredSearch { patterns, debug_snippet } => {
            cli::search_for_patterns(patterns, debug_snippet)?;
        }
        args::Command::Version => println!("rust-analyzer {}", env!("REV")),
    }
    Ok(())
}

fn setup_logging() -> Result<()> {
    std::env::set_var("RUST_BACKTRACE", "short");
    env_logger::try_init_from_env("RA_LOG")?;
    ra_prof::init();
    Ok(())
}

fn run_server() -> Result<()> {
    log::info!("lifecycle: server started");

    let (connection, io_threads) = Connection::stdio();

    let (initialize_id, initialize_params) = connection.initialize_start()?;
    let initialize_params =
        from_json::<lsp_types::InitializeParams>("InitializeParams", initialize_params)?;

    let server_capabilities = rust_analyzer::server_capabilities(&initialize_params.capabilities);

    let initialize_result = lsp_types::InitializeResult {
        capabilities: server_capabilities,
        server_info: Some(lsp_types::ServerInfo {
            name: String::from("rust-analyzer"),
            version: Some(String::from(env!("REV"))),
        }),
    };

    let initialize_result = serde_json::to_value(initialize_result).unwrap();

    connection.initialize_finish(initialize_id, initialize_result)?;

    if let Some(client_info) = initialize_params.client_info {
        log::info!("Client '{}' {}", client_info.name, client_info.version.unwrap_or_default());
    }

    let config = {
        let root_path = match initialize_params
            .root_uri
            .and_then(|it| it.to_file_path().ok())
            .and_then(|it| AbsPathBuf::try_from(it).ok())
        {
            Some(it) => it,
            None => {
                let cwd = std::env::current_dir()?;
                AbsPathBuf::assert(cwd)
            }
        };

        let mut config = Config::new(root_path);
        if let Some(json) = initialize_params.initialization_options {
            config.update(json);
        }
        config.update_caps(&initialize_params.capabilities);

        if config.linked_projects.is_empty() {
            let workspace_roots = initialize_params
                .workspace_folders
                .map(|workspaces| {
                    workspaces
                        .into_iter()
                        .filter_map(|it| it.uri.to_file_path().ok())
                        .filter_map(|it| AbsPathBuf::try_from(it).ok())
                        .collect::<Vec<_>>()
                })
                .filter(|workspaces| !workspaces.is_empty())
                .unwrap_or_else(|| vec![config.root_path.clone()]);

            config.linked_projects = ProjectManifest::discover_all(&workspace_roots)
                .into_iter()
                .map(LinkedProject::from)
                .collect();
        }

        config
    };

    rust_analyzer::main_loop(config, connection)?;

    log::info!("shutting down IO...");
    io_threads.join()?;
    log::info!("... IO is down");
    Ok(())
}
