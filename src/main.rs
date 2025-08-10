use std::{
    env,
    path::{Component, Path, PathBuf, Prefix},
    process::{Command, ExitCode, ExitStatus},
};

fn is_on_unc_path() -> bool {
    let Ok(current_dir) = env::current_dir() else {
        // Some unexpected directory, fall back to true
        return true;
    };

    let canonical_dir = current_dir.canonicalize().unwrap_or(current_dir);
    let Some(Component::Prefix(prefix)) = canonical_dir.components().next() else {
        return false;
    };

    matches!(prefix.kind(), Prefix::UNC(_, _) | Prefix::VerbatimUNC(_, _))
}

struct Extension {
    suffix: &'static str,
    support_unc: bool,
}

fn find_executable(command: &str) -> Result<PathBuf, String> {
    if Path::new(command).extension().is_some() {
        return which::which(command).map_err(|_| format!("Command '{}' not found", command));
    }

    let is_unc = is_on_unc_path();
    let extensions = [
        Extension {
            suffix: ".exe",
            support_unc: true,
        },
        Extension {
            suffix: ".bat",
            support_unc: false,
        },
        Extension {
            suffix: ".cmd",
            support_unc: false,
        },
    ];

    let mut found_unsupported = None;

    for ext in &extensions {
        let candidate = format!("{}{}", command, ext.suffix);
        if let Ok(path) = which::which(&candidate) {
            if !ext.support_unc && is_unc {
                found_unsupported = Some(path);
            } else {
                return Ok(path);
            }
        }
    }

    if let Some(path) = found_unsupported {
        return Err(format!(
            "Command '{}' found but cannot be executed from UNC path (network drive). Use .exe files or run from a local drive.",
            path.display()
        ));
    }

    Err(format!("Command '{}' not found", command))
}

fn main() -> ExitCode {
    let args = env::args().collect::<Vec<String>>();
    if args.len() < 2 {
        eprintln!("Usage: {} <command> [args...]", args[0]);
        return ExitCode::FAILURE;
    }

    let exe = match find_executable(&args[1]) {
        Ok(path) => path,
        Err(msg) => {
            eprintln!("Error: {}", msg);
            return ExitCode::FAILURE;
        }
    };

    let mut child = match Command::new(&exe).args(&args[2..]).spawn() {
        Ok(child) => child,
        Err(e) => {
            eprintln!("Error starting '{}': {}", exe.display(), e);
            return ExitCode::FAILURE;
        }
    };

    let status: ExitStatus = match child.wait() {
        Ok(status) => status,
        Err(e) => {
            eprintln!("Error waiting for '{}': {}", exe.display(), e);
            return ExitCode::FAILURE;
        }
    };

    ExitCode::from(status.code().unwrap_or(1) as u8)
}
