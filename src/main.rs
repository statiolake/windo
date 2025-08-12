use std::{
    env,
    io::{self, BufRead, BufReader, Write},
    path::{Component, Path, PathBuf, Prefix},
    process::{Command, ExitCode, ExitStatus, Stdio},
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

struct Configuration {
    path: PathBuf,
    pipe: bool,
}

fn find_configuration(command: &str) -> Result<Configuration, String> {
    if Path::new(command).extension().is_some() {
        let path = which::which(command).map_err(|_| format!("Command '{}' not found", command))?;
        return Ok(Configuration { path, pipe: false });
    }

    struct SupportedExecutable {
        suffix: &'static str,
        support_unc: bool,
        pipe: bool,
    }
    let supported = [
        SupportedExecutable {
            suffix: ".exe",
            support_unc: true,
            pipe: false,
        },
        SupportedExecutable {
            suffix: ".bat",
            support_unc: false,
            pipe: true,
        },
        SupportedExecutable {
            suffix: ".cmd",
            support_unc: false,
            pipe: true,
        },
    ];

    let is_unc = is_on_unc_path();

    let mut found_unsupported = None;

    for ext in &supported {
        let candidate = format!("{}{}", command, ext.suffix);
        if let Ok(path) = which::which(&candidate) {
            if !ext.support_unc && is_unc {
                found_unsupported = Some(Configuration {
                    path,
                    pipe: ext.pipe,
                });
            } else {
                return Ok(Configuration {
                    path,
                    pipe: ext.pipe,
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
        let mut child = match Command::new(&exe.path)
            .args(&args[2..])
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
        let mut child = match Command::new(&exe.path).args(&args[2..]).spawn() {
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
