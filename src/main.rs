use anyhow::Result;
use clap::{ArgAction, Args, Parser, Subcommand};
use omatracker::{
    ProjectChanges, add_task, check_reports, create_project, default_data_path, diagnostics,
    edit_task, export_report, install_report_timer, presentation_status, remove_report_timer,
    remove_task, reset_active_project, reset_task, retry_reports, select_project, start_task,
    status, stop_task, sync_state, update_drive, update_project,
};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "omatracker",
    version,
    about = "Project time tracking backend for Omarchy"
)]
struct Cli {
    /// JSON ledger path. Defaults to ~/.config/omarchy/omatracker.json.
    #[arg(long, global = true, env = "OMATRACKER_DATA_PATH")]
    data_path: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Print the complete presentation state as JSON.
    Status {
        #[arg(long)]
        json: bool,
        /// Omit historical ledger data and external setup checks from JSON output.
        #[arg(long, requires = "json")]
        compact: bool,
    },
    /// Print dependency and background-scheduler diagnostics as JSON.
    Diagnostics,
    /// Claim an hourly work milestone, or save local feedback preferences.
    Feedback {
        #[command(subcommand)]
        command: FeedbackCommand,
    },
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
    Drive {
        #[command(subcommand)]
        command: DriveCommand,
    },
    Report {
        #[command(subcommand)]
        command: ReportCommand,
    },
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
    },
    /// Create, inspect, and preview user-owned PDF templates.
    Template {
        #[command(subcommand)]
        command: TemplateCommand,
    },
    /// Upload the current ledger snapshot with rclone.
    Sync,
}

#[derive(Subcommand)]
enum ProjectCommand {
    Create {
        #[arg(default_value = "New project")]
        name: String,
    },
    Select {
        id: String,
    },
    Update(Box<ProjectUpdate>),
}

#[derive(Subcommand)]
enum FeedbackCommand {
    Poll,
    Configure {
        #[arg(long, action = ArgAction::Set)]
        hourly_click: bool,
        #[arg(long, default_value_t = 25)]
        volume: u8,
        #[arg(long, action = ArgAction::Set)]
        reduced_motion: bool,
    },
}

#[derive(Args)]
struct ProjectUpdate {
    id: String,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    client_name: Option<String>,
    #[arg(long)]
    company_name: Option<String>,
    #[arg(long)]
    template_id: Option<String>,
    #[arg(long)]
    accent_color: Option<String>,
    #[arg(long, value_parser = ["a4", "letter"])]
    paper: Option<String>,
    /// Image file; pass an empty string to remove the logo.
    #[arg(long)]
    logo_path: Option<String>,
    #[arg(long)]
    export_weekly: Option<bool>,
    #[arg(long)]
    export_monthly: Option<bool>,
    /// Decimal hourly rate; uses the existing currency when omitted.
    #[arg(long, conflicts_with = "clear_rate")]
    hourly_rate: Option<String>,
    /// Supported currency code (for example USD, EUR, JPY, KWD).
    #[arg(long, requires = "hourly_rate", conflicts_with = "clear_rate")]
    currency: Option<String>,
    /// Remove the project's hourly rate.
    #[arg(long)]
    clear_rate: bool,
}

#[derive(Subcommand)]
enum TaskCommand {
    Add {
        #[arg(default_value = "Empty")]
        title: String,
    },
    Start {
        id: String,
    },
    Stop {
        id: String,
    },
    Reset {
        id: String,
    },
    ResetActiveProject,
    Remove {
        id: String,
    },
    Edit(TaskEdit),
}

#[derive(Args)]
struct TaskEdit {
    id: String,
    #[arg(long)]
    title: Option<String>,
    /// Time to add as a dated manual entry, such as 1h30m or 01:30:00.
    #[arg(long)]
    add: Option<String>,
}

#[derive(Subcommand)]
enum DriveCommand {
    Update {
        #[arg(long, default_value = "")]
        remote: String,
        #[arg(long, default_value = "OmaTracker")]
        folder: String,
        // The QML client always sends an explicit true/false value.
        #[arg(long, default_value_t = true, action = ArgAction::Set)]
        sync_on_startup: bool,
    },
}

#[derive(Subcommand)]
enum ReportCommand {
    /// Queue and export the preceding completed period for the active project.
    Export {
        #[arg(value_parser = ["weekly", "monthly"])]
        period: String,
    },
    /// Queue missing completed weekly/monthly reports, then process pending work.
    Check,
    /// Requeue failed or interrupted reports, then process them.
    Retry,
}

#[derive(Subcommand)]
enum ServiceCommand {
    /// Install and enable the 15-minute user-level report-check timer.
    Install,
    /// Disable and remove the user-level report-check timer.
    Remove,
}

#[derive(Subcommand)]
enum TemplateCommand {
    List {
        #[arg(long)]
        json: bool,
    },
    Create {
        name: String,
        #[arg(long, default_value = "detailed")]
        from: String,
    },
    Path {
        id: String,
    },
    Validate {
        id: String,
    },
    Preview {
        id: String,
        #[arg(long)]
        project: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let data_path = cli.data_path.unwrap_or(default_data_path()?);
    match cli.command {
        Commands::Status { json, compact } => {
            if compact {
                println!(
                    "{}",
                    serde_json::to_string(&presentation_status(&data_path)?)?
                );
            } else if json {
                println!("{}", serde_json::to_string(&status(&data_path)?)?);
            } else {
                println!(
                    "{}",
                    presentation_status(&data_path)?.active_project_seconds
                );
            }
        }
        Commands::Diagnostics => println!("{}", serde_json::to_string(&diagnostics(&data_path))?),
        Commands::Feedback { command } => match command {
            FeedbackCommand::Poll => println!(
                "{}",
                serde_json::to_string(&omatracker::feedback::poll(&data_path)?)?
            ),
            FeedbackCommand::Configure {
                hourly_click,
                volume,
                reduced_motion,
            } => omatracker::feedback::configure(&data_path, hourly_click, volume, reduced_motion)?,
        },
        Commands::Project { command } => match command {
            ProjectCommand::Create { name } => println!("{}", create_project(&data_path, &name)?),
            ProjectCommand::Select { id } => select_project(&data_path, &id)?,
            ProjectCommand::Update(update) => update_project(
                &data_path,
                &update.id,
                ProjectChanges {
                    name: update.name,
                    client_name: update.client_name,
                    company_name: update.company_name,
                    template_id: update.template_id,
                    accent_color: update.accent_color,
                    paper: update.paper,
                    logo_path: update.logo_path,
                    export_weekly: update.export_weekly,
                    export_monthly: update.export_monthly,
                    hourly_rate: update.hourly_rate,
                    currency: update.currency,
                    clear_rate: update.clear_rate,
                },
            )?,
        },
        Commands::Task { command } => match command {
            TaskCommand::Add { title } => println!("{}", add_task(&data_path, Some(&title))?),
            TaskCommand::Start { id } => start_task(&data_path, &id)?,
            TaskCommand::Stop { id } => stop_task(&data_path, &id)?,
            TaskCommand::Reset { id } => reset_task(&data_path, &id)?,
            TaskCommand::ResetActiveProject => reset_active_project(&data_path)?,
            TaskCommand::Remove { id } => remove_task(&data_path, &id)?,
            TaskCommand::Edit(edit) => edit_task(
                &data_path,
                &edit.id,
                edit.title.as_deref(),
                edit.add.as_deref(),
            )?,
        },
        Commands::Drive { command } => match command {
            DriveCommand::Update {
                remote,
                folder,
                sync_on_startup,
            } => update_drive(&data_path, &remote, &folder, sync_on_startup)?,
        },
        Commands::Report { command } => match command {
            ReportCommand::Export { period } => export_report(&data_path, &period)?,
            ReportCommand::Check => check_reports(&data_path)?,
            ReportCommand::Retry => retry_reports(&data_path)?,
        },
        Commands::Service { command } => match command {
            ServiceCommand::Install => install_report_timer(&data_path)?,
            ServiceCommand::Remove => remove_report_timer()?,
        },
        Commands::Template { command } => {
            use omatracker::templates;
            match command {
                TemplateCommand::List { json } => {
                    let items = templates::list()?;
                    if json {
                        println!("{}", serde_json::to_string(&items)?);
                    } else {
                        for item in items {
                            println!("{}\t{}\t{}", item.id, item.name, item.path);
                        }
                    }
                }
                TemplateCommand::Create { name, from } => println!(
                    "{}",
                    serde_json::to_string(&templates::create(&name, &from)?)?
                ),
                TemplateCommand::Path { id } => println!("{}", templates::path(&id)?.display()),
                TemplateCommand::Validate { id } => {
                    templates::validate(&id)?;
                    println!("Template is valid");
                }
                TemplateCommand::Preview { id, project } => println!(
                    "{}",
                    templates::preview(&data_path, &id, project.as_deref())?.display()
                ),
            }
        }
        Commands::Sync => sync_state(&data_path)?,
    }
    Ok(())
}
