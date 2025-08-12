use std::{
    env,
    fs,
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, ExitCode, ExitStatus, Stdio},
};


fn is_on_unc_path() -> bool {
    let Ok(current_dir) = env::current_dir() else {
        // Some unexpected directory, fall back to true
        return true;
    };

    // In WSL, resolve symlinks to get the actual path
    let canonical_dir = match fs::read_link(&current_dir) {
        Ok(link_target) => link_target,
        Err(_) => current_dir.clone(),
    };

    // Check if the path starts with /mnt/{drive_letter}
    // If not, it's likely a UNC path when accessed from Windows
    let path_str = canonical_dir.to_string_lossy();
    
    // UNC path pattern: not starting with /mnt/ followed by a single letter
    !path_str.starts_with("/mnt/") || 
    !path_str.chars().nth(5).map_or(false, |c| c.is_ascii_alphabetic()) ||
    !path_str.chars().nth(6).map_or(false, |c| c == '/')
}

struct Configuration {
    path: PathBuf,
    pipe: bool,
    needs_cmd_wrapper: bool,
}

fn find_configuration(command: &str) -> Result<Configuration, String> {
    if Path::new(command).extension().is_some() {
        let path = which::which(command).map_err(|_| format!("Command '{}' not found", command))?;
        let needs_cmd_wrapper = command.ends_with(".bat") || command.ends_with(".cmd");
        return Ok(Configuration { path, pipe: needs_cmd_wrapper, needs_cmd_wrapper });
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
            "Command '{}' found but cannot be executed from UNC path (network drive). Use .exe files or run from a local drive.",
            exe.path.display()
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

    let exe = match find_configuration(&args[1]) {
        Ok(path) => path,
        Err(msg) => {
            eprintln!("Error: {}", msg);
            return ExitCode::FAILURE;
        }
    };

    let status: ExitStatus = if exe.pipe {
        let mut command = if exe.needs_cmd_wrapper {
            let mut cmd = Command::new("cmd.exe");
            cmd.arg("/c");
            
            // Convert WSL path to Windows path using wslpath
            let windows_path = Command::new("wslpath")
                .arg("-w")
                .arg(&exe.path)
                .output()
                .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
                .unwrap_or_else(|_| exe.path.display().to_string());
                
            cmd.arg(windows_path);
            cmd.args(&args[2..]);
            cmd
        } else {
            let mut cmd = Command::new(&exe.path);
            cmd.args(&args[2..]);
            cmd
        };
        
        let mut child = match command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                eprintln!("Error starting '{}': {}", exe.path.display(), e);
                return ExitCode::FAILURE;
            }
        };

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let stdout_handle = std::thread::spawn(move || {
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

        let stderr_handle = std::thread::spawn(move || {
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

        let status = match child.wait() {
            Ok(status) => status,
            Err(e) => {
                eprintln!("Error waiting for '{}': {}", exe.path.display(), e);
                return ExitCode::FAILURE;
            }
        };

        stdout_handle.join().unwrap();
        stderr_handle.join().unwrap();

        status
    } else {
        let mut command = if exe.needs_cmd_wrapper {
            let mut cmd = Command::new("cmd.exe");
            cmd.arg("/c");
            
            // Convert WSL path to Windows path using wslpath
            let windows_path = Command::new("wslpath")
                .arg("-w")
                .arg(&exe.path)
                .output()
                .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
                .unwrap_or_else(|_| exe.path.display().to_string());
                
            cmd.arg(windows_path);
            cmd.args(&args[2..]);
            cmd
        } else {
            let mut cmd = Command::new(&exe.path);
            cmd.args(&args[2..]);
            cmd
        };
        
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(e) => {
                eprintln!("Error starting '{}': {}", exe.path.display(), e);
                return ExitCode::FAILURE;
            }
        };

        match child.wait() {
            Ok(status) => status,
            Err(e) => {
                eprintln!("Error waiting for '{}': {}", exe.path.display(), e);
                return ExitCode::FAILURE;
            }
        }
    };

    ExitCode::from(status.code().unwrap_or(1) as u8)
}
