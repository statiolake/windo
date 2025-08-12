use std::{
    env, fs,
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    thread,
};

use regex::Regex;

fn is_on_unc_path() -> bool {
    let Ok(current_dir) = env::current_dir() else {
        // Some unexpected directory. Considering it as a UNC path for safety.
        return true;
    };

    // Resolve symlinks to get the actual path
    let canonical_dir = match fs::read_link(&current_dir) {
        Ok(link_target) => link_target,
        Err(_) => current_dir,
    };

    // Check if the path starts with /mnt/{drive_letter}
    // If not, it's likely a UNC path when accessed from Windows
    // TODO: you can check UNC paths more robustly to use wslpath and UNC path pattern, but in my
    // simple use case, this is enough.
    let mnt_dir_pattern = Regex::new(r"^/mnt/[a-zA-Z]/").unwrap();
    let path_str = canonical_dir.to_string_lossy();

    !mnt_dir_pattern.is_match(&path_str)
}

struct Configuration {
    path: PathBuf,
    pipe: bool,
    needs_cmd_wrapper: bool,
}

fn find_configuration(command: &str) -> Result<Configuration, String> {
    if Path::new(command).extension().is_some() {
        let path = which::which(command).map_err(|_| format!("Command '{}' not found", command))?;
        return Ok(Configuration {
            path,
            pipe: false,
            needs_cmd_wrapper: false,
        });
    }

    struct SupportedExecutable {
        suffix: &'static str,
        needs_cmd_wrapper: bool,
        pipe: bool,
    }

    let supported = [
        SupportedExecutable {
            suffix: ".exe",
            needs_cmd_wrapper: false,
            pipe: false,
        },
        SupportedExecutable {
            suffix: ".bat",
            needs_cmd_wrapper: true,
            pipe: true,
        },
        SupportedExecutable {
            suffix: ".cmd",
            needs_cmd_wrapper: true,
            pipe: true,
        },
    ];

    let is_unc = is_on_unc_path();

    let mut found_unsupported = None;
    for ext in &supported {
        let candidate = format!("{}{}", command, ext.suffix);
        if let Ok(path) = which::which(&candidate) {
            if ext.needs_cmd_wrapper && is_unc {
                found_unsupported = Some(Configuration {
                    path,
                    pipe: ext.pipe,
                    needs_cmd_wrapper: ext.needs_cmd_wrapper,
                });
            } else {
                return Ok(Configuration {
                    path,
                    pipe: ext.pipe,
                    needs_cmd_wrapper: ext.needs_cmd_wrapper,
                });
            }
        }
    }

    if let Some(exe) = found_unsupported {
        return Err(format!(
            "Note: Command '{}' found but cannot be executed from UNC path (network drive). Use .exe files or run from a local drive.",
            exe.path.display()
        ));
    }

    Err(format!("Command '{}' not found", command))
}

fn execute() -> Result<ExitCode, String> {
    let args = env::args().collect::<Vec<String>>();
    if args.len() < 2 {
        eprintln!("Usage: {} <command> [args...]", args[0]);
        return Err("No command provided".to_string());
    }

    let config = find_configuration(&args[1])?;

    let mut cmd = if config.needs_cmd_wrapper {
        let mut cmd = Command::new("cmd.exe");
        cmd.arg("/c");
        let windows_binary_path = Command::new("wslpath")
            .arg("-w")
            .arg(&config.path)
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .map_err(|e| format!("Failed to convert path: {}", e))?;

        cmd.arg(windows_binary_path);
        cmd
    } else {
        Command::new(&config.path)
    };

    cmd.args(&args[2..]);

    if config.pipe {
        // If the command is piped, we need to set up the command to capture stdout and stderr
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Error starting '{}': {}", config.path.display(), e))?;

    let mut join_handles = vec![];
    if config.pipe {
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let stdout_handle = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            while let Ok(n) = reader.read_line(&mut line) {
                if n == 0 {
                    break;
                }
                print!("{}", line);
                io::stdout().flush().unwrap();
                line.clear();
            }
        });

        let stderr_handle = thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            while let Ok(n) = reader.read_line(&mut line) {
                if n == 0 {
                    break;
                }
                eprint!("{}", line);
                io::stderr().flush().unwrap();
                line.clear();
            }
        });

        join_handles.push(stdout_handle);
        join_handles.push(stderr_handle);
    }

    let status = child
        .wait()
        .map_err(|e| format!("Error waiting for '{}': {}", config.path.display(), e))?;

    join_handles.into_iter().for_each(|h| h.join().unwrap());

    Ok(ExitCode::from(status.code().unwrap_or(1) as u8))
}

fn main() -> ExitCode {
    match execute() {
        Ok(exit_code) => exit_code,
        Err(msg) => {
            eprintln!("Error: {}", msg);
            ExitCode::FAILURE
        }
    }
}
