use std::{env, io, process::Command};

fn main() -> io::Result<()> {
    let args = env::args().collect::<Vec<String>>();
    if args.len() < 2 {
        eprintln!("Usage: {} <command> [args...]", args[0]);
        return Ok(());
    }

    Command::new(&args[1]).args(&args[2..]).spawn()?.wait()?;
    Ok(())
}
