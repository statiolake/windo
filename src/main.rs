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

fn find_executable(command: &str) -> Option<PathBuf> {
    if Path::new(command).extension().is_some() {
        return which::which(command).ok();
    }

    let extensions = if is_on_unc_path() {
        [".exe"].as_slice()
    } else {
        [".exe", ".bat", ".cmd"].as_slice()
    };

    for &ext in extensions {
        let candidate = format!("{}{}", command, ext);
        if let Ok(path) = which::which(&candidate) {
            return Some(path);
        }
    }

    None
}

fn main() -> ExitCode {
    let args = env::args().collect::<Vec<String>>();
    if args.len() < 2 {
        eprintln!("Usage: {} <command> [args...]", args[0]);
        return ExitCode::FAILURE;
    }

    let Some(exe) = find_executable(&args[1]) else {
        eprintln!("Error: Command '{}' not found", args[1]);
        return ExitCode::FAILURE;
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
