use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use time_tracker::{
    ProjectChanges, add_task, check_reports, create_project, default_data_path, edit_task,
    export_report, install_report_timer, remove_report_timer, remove_task, reset_active_project,
    reset_task, retry_reports, select_project, start_task, status, stop_task, sync_state,
    update_drive, update_project,
};

#[derive(Parser)]
#[command(
    name = "time-tracker",
    version,
    about = "Project time tracking backend for Omarchy"
)]
struct Cli {
    /// JSON ledger path. Defaults to ~/.config/omarchy/time-tracker.json.
    #[arg(long, global = true, env = "TIME_TRACKER_DATA_PATH")]
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
    Update(ProjectUpdate),
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
    export_weekly: Option<bool>,
    #[arg(long)]
    export_monthly: Option<bool>,
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
        #[arg(long, default_value = "TimeTracker")]
        folder: String,
        #[arg(long, default_value_t = true)]
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

fn main() -> Result<()> {
    let cli = Cli::parse();
    let data_path = cli.data_path.unwrap_or(default_data_path()?);
    match cli.command {
        Commands::Status { json } => {
            let result = status(&data_path)?;
            if json {
                println!("{}", serde_json::to_string(&result)?);
            } else {
                println!("{}", result.active_project_seconds);
            }
        }
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
                    export_weekly: update.export_weekly,
                    export_monthly: update.export_monthly,
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
        Commands::Sync => sync_state(&data_path)?,
    }
    Ok(())
}
