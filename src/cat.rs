use std::fs;
use std::io;
use std::path::Path;

use anyhow::{Context, Result};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

pub fn passthrough_cat(args: &[&str]) -> Result<()> {
    if args.is_empty() {
        return copy_stdin_to_stdout();
    }

    if needs_system_cat(args) {
        exec_system_cat(args)?;
    }

    for arg in args {
        if arg.starts_with('-') {
            exec_system_cat(args)?;
        }
        if Path::new(arg).is_file() {
            builtin_cat_file(arg)?;
        } else {
            exec_system_cat(args)?;
        }
    }
    Ok(())
}

fn needs_system_cat(args: &[&str]) -> bool {
    args.len() > 1 || args.iter().any(|a| a.starts_with('-')) || !Path::new(args[0]).is_file()
}

fn copy_stdin_to_stdout() -> Result<()> {
    io::copy(&mut io::stdin(), &mut io::stdout())?;
    Ok(())
}

pub fn builtin_cat_file(path: &str) -> Result<()> {
    let mut file = fs::File::open(path).with_context(|| format!("cannot open {path}"))?;
    io::copy(&mut file, &mut io::stdout())?;
    Ok(())
}

pub fn exec_system_cat(args: &[&str]) -> Result<()> {
    #[cfg(unix)]
    {
        let cat = find_system_cat();
        let err = std::process::Command::new(&cat).args(args).exec();
        Err(anyhow::Error::from(err))
    }

    #[cfg(not(unix))]
    {
        if args.is_empty() {
            return copy_stdin_to_stdout();
        }
        for arg in args {
            if arg.starts_with('-') {
                anyhow::bail!("unsupported cat flag on this platform: {arg}");
            }
            if Path::new(arg).is_file() {
                builtin_cat_file(arg)?;
            } else {
                let mut stderr = io::stderr();
                use std::io::Write;
                writeln!(stderr, "omnicat: {arg}: No such file or directory")?;
                std::process::exit(1);
            }
        }
        Ok(())
    }
}

#[cfg(unix)]
fn find_system_cat() -> String {
    for candidate in ["/bin/cat", "/usr/bin/cat"] {
        if Path::new(candidate).exists() {
            return candidate.to_string();
        }
    }
    "cat".to_string()
}
