//! Terminal interface: `privavault lock|open|list|check …`. Runs instead of the window when the
//! first argument is one of these commands. Unix only (the Windows build is a GUI-subsystem app
//! with no console to print to).

use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use privavault_core::{create_vault, Job, Progress, Strength, Vault};
use zeroize::Zeroizing;

const USAGE: &str = "\
Usage:
  privavault lock <file or folder>... [-o OUT.pvault] [--strength standard|high|maximum] [--force]
  privavault open <vault.pvault> [-d DIR]     extract everything into a new folder in DIR (default: .)
  privavault list <vault.pvault>              show the files inside
  privavault check <vault.pvault>             decrypt and verify every byte, write nothing
  privavault                                  start the app

The password is asked on the terminal. When stdin is not a terminal, the first line of stdin is
used instead (for scripts: printf '%s\\n' \"$PW\" | privavault open v.pvault).";

/// `Some(exit code)` when the arguments name a command, `None` to start the app.
pub fn main() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first()?.as_str();
    if !matches!(cmd, "lock" | "open" | "list" | "check" | "help" | "--help" | "-h") {
        return None;
    }
    let result = match cmd {
        "lock" => lock(&args[1..]),
        "open" => open(&args[1..]),
        "list" => list(&args[1..]),
        "check" => check(&args[1..]),
        _ => {
            println!("{USAGE}");
            Ok(())
        }
    };
    Some(match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("privavault: {e}");
            1
        }
    })
}

type CliResult = Result<(), String>;
type Parsed<'a> = (Vec<&'a str>, Vec<(&'a str, &'a str)>);

/// Split `args` into positionals and `--flag value` / `--switch` options.
fn parse<'a>(args: &'a [String], valued: &[&str], switches: &[&str]) -> Result<Parsed<'a>, String> {
    let (mut pos, mut opts) = (Vec::new(), Vec::new());
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if valued.contains(&a.as_str()) {
            let v = it.next().ok_or(format!("{a} needs a value"))?;
            opts.push((a.as_str(), v.as_str()));
        } else if switches.contains(&a.as_str()) {
            opts.push((a.as_str(), ""));
        } else if a.starts_with('-') && a.len() > 1 {
            return Err(format!("unknown option {a}\n\n{USAGE}"));
        } else {
            pos.push(a.as_str());
        }
    }
    Ok((pos, opts))
}

fn opt<'a>(opts: &[(&str, &'a str)], names: &[&str]) -> Option<&'a str> {
    opts.iter().rev().find(|(k, _)| names.contains(k)).map(|(_, v)| *v)
}

fn one_vault<'a>(pos: &[&'a str]) -> Result<&'a Path, String> {
    match pos {
        [v] => Ok(Path::new(*v)),
        _ => Err(format!("expected exactly one vault file\n\n{USAGE}")),
    }
}

fn lock(args: &[String]) -> CliResult {
    let (pos, opts) = parse(args, &["-o", "--output", "--strength"], &["--force", "-f"])?;
    if pos.is_empty() {
        return Err(format!("nothing to lock\n\n{USAGE}"));
    }
    let inputs: Vec<PathBuf> = pos.iter().map(PathBuf::from).collect();
    let output = match opt(&opts, &["-o", "--output"]) {
        Some(o) => PathBuf::from(o),
        None => default_output(&inputs[0])?,
    };
    let strength = match opt(&opts, &["--strength"]).unwrap_or("standard") {
        "standard" => Strength::Standard,
        "high" => Strength::High,
        "maximum" => Strength::Maximum,
        s => return Err(format!("unknown strength {s:?} (standard, high or maximum)")),
    };
    // The app gets overwrite consent from its save dialog; here it takes --force.
    if output.exists() && opt(&opts, &["--force", "-f"]).is_none() {
        return Err(format!("{} already exists (use --force to replace it)", output.display()));
    }
    let password = read_password("Password: ")?;
    if password.chars().count() < 8 {
        return Err("the password must be at least 8 characters".into());
    }
    if io::stdin().is_terminal() && *read_password("Repeat password: ")? != *password {
        return Err("the passwords do not match".into());
    }
    let rep = run_job(|job| create_vault(&inputs, &output, &password, strength.params(), job))?;
    println!("Locked {} files, {} folders ({}) into {}", rep.files, rep.folders, size(rep.bytes), rep.output.display());
    for s in &rep.skipped {
        eprintln!("skipped (not a regular file or folder): {}", s.display());
    }
    Ok(())
}

/// `notes/` → `./notes.pvault`, `a.txt` → `./a.txt.pvault`.
fn default_output(first: &Path) -> Result<PathBuf, String> {
    let name = std::fs::canonicalize(first)
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_owned()))
        .ok_or(format!("cannot read {}", first.display()))?;
    let mut name = name.into_string().map_err(|_| "file name is not valid UTF-8; pass -o".to_string())?;
    name.push_str(".pvault");
    Ok(PathBuf::from(name))
}

fn open_vault(path: &Path) -> Result<Vault, String> {
    let password = read_password("Password: ")?;
    run_job(|job| Vault::open(path, &password, job))
}

fn open(args: &[String]) -> CliResult {
    let (pos, opts) = parse(args, &["-d", "--dir"], &[])?;
    let path = one_vault(&pos)?;
    let parent = PathBuf::from(opt(&opts, &["-d", "--dir"]).unwrap_or("."));
    let mut vault = open_vault(path)?;
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "vault".into());
    let rep = run_job(|job| vault.extract_all(&parent, &stem, job))?;
    println!(
        "Extracted {} files, {} folders ({}) into {}",
        rep.files,
        rep.folders,
        size(rep.bytes),
        rep.root.display()
    );
    for s in &rep.skipped_unsafe {
        eprintln!("skipped unsafe name: {s:?}");
    }
    Ok(())
}

fn list(args: &[String]) -> CliResult {
    let (pos, _) = parse(args, &[], &[])?;
    let vault = open_vault(one_vault(&pos)?)?;
    for e in vault.entries() {
        // Names come from the vault: print them escaped so control characters can't drive the terminal.
        let name = format!("{:?}", e.path);
        let name = &name[1..name.len() - 1];
        if e.is_dir {
            println!("{:>10}  {name}/", "");
        } else {
            println!("{:>10}  {name}", size(e.size));
        }
    }
    Ok(())
}

fn check(args: &[String]) -> CliResult {
    let (pos, _) = parse(args, &[], &[])?;
    let mut vault = open_vault(one_vault(&pos)?)?;
    let bytes = run_job(|job| vault.verify(job))?;
    println!(
        "OK: {} files, {} verified ({})",
        vault.entries().iter().filter(|e| !e.is_dir).count(),
        size(bytes),
        vault.kdf_description()
    );
    Ok(())
}

/// Run a core operation with a progress line on stderr (only when stderr is a terminal).
fn run_job<T>(f: impl FnOnce(&Job) -> privavault_core::Result<T>) -> Result<T, String> {
    let cancel = AtomicBool::new(false);
    let tty = io::stderr().is_terminal();
    let progress = move |p: Progress| {
        if tty && p.total > 0 {
            eprint!("\r{:?} {:>3}%", p.phase, p.done * 100 / p.total);
        }
    };
    let result = f(&Job::new(&cancel, &progress));
    if tty {
        eprint!("\r\x1b[K");
    }
    result.map_err(|e| e.to_string())
}

fn read_password(prompt: &str) -> Result<Zeroizing<String>, String> {
    let stdin = io::stdin();
    let mut line = Zeroizing::new(String::new());
    if stdin.is_terminal() {
        eprint!("{prompt}");
        io::stderr().flush().ok();
        let _echo = EchoOff::new();
        stdin.lock().read_line(&mut line).map_err(|e| e.to_string())?;
        eprintln!();
    } else {
        stdin.lock().read_line(&mut line).map_err(|e| e.to_string())?;
    }
    let trimmed = line.trim_end_matches(['\n', '\r']).len();
    line.truncate(trimmed);
    if line.is_empty() {
        return Err("no password given".into());
    }
    Ok(line)
}

/// Turns terminal echo off until dropped.
struct EchoOff(Option<libc::termios>);

impl EchoOff {
    fn new() -> Self {
        // SAFETY: plain termios calls on stdin with a zero-initialised struct the kernel fills in.
        unsafe {
            let mut t: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(libc::STDIN_FILENO, &mut t) != 0 {
                return EchoOff(None);
            }
            let saved = t;
            t.c_lflag &= !libc::ECHO;
            t.c_lflag |= libc::ECHONL;
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &t);
            EchoOff(Some(saved))
        }
    }
}

impl Drop for EchoOff {
    fn drop(&mut self) {
        if let Some(t) = self.0 {
            // SAFETY: restores the attributes read in `new`.
            unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &t) };
        }
    }
}

fn size(b: u64) -> String {
    const U: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", U[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn parses_positionals_and_options() {
        let a = s(&["a", "-o", "x.pvault", "b", "--force"]);
        let (pos, opts) = parse(&a, &["-o"], &["--force"]).unwrap();
        assert_eq!(pos, ["a", "b"]);
        assert_eq!(opt(&opts, &["-o"]), Some("x.pvault"));
        assert!(opt(&opts, &["--force"]).is_some());
        assert!(parse(&s(&["-o"]), &["-o"], &[]).is_err());
        assert!(parse(&s(&["--nope"]), &[], &[]).is_err());
    }

    #[test]
    fn sizes() {
        assert_eq!(size(512), "512 B");
        assert_eq!(size(1536), "1.5 KiB");
    }
}
