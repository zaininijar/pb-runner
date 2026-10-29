use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const OWNER: &str = "pb-runner-owned-prefix-v1\n";
const DEFAULT_LAUNCHER: &str = r"C:\Zepetto\Z-Launcher(ID)\ZLauncher.exe";
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug)]
struct Options {
    state: PathBuf,
    wine: Option<PathBuf>,
    timeout: u64,
    debug: String,
    command: String,
    args: Vec<OsString>,
}

fn help() {
    println!("PB Runner 0.1.0 — independent Rust prototype\n\
Usage: pb-runner [OPTIONS] COMMAND [ARGS...]\n\n\
Options (before COMMAND):\n\
  --state DIR      Dedicated state directory (default: ./state)\n\
  --wine FILE      Independent Wine executable; saved by init\n\
  --timeout SEC    Stop this owned prefix after timeout; 0 = no limit\n\
  --debug TRACE    WINEDEBUG value (default: -all,+seh)\n\n\
Commands:\n\
  doctor           Read-only host/runtime checks\n\
  init             Create a NEW isolated prefix and select Windows 10\n\
  install FILE     Run an official Windows .exe or .msi installer\n\
  dotnet SCRIPT    Install .NET 4.8 through a verified Winetricks script\n\
  run [EXE ARGS]   Run ZLauncher by default, or a specified executable\n\
  stop             Stop processes in this runner's owned prefix only\n\
  status           Show client/launcher presence and last run\n\n\
Each invocation writes separate stdout/stderr files and a result.json.\n\
Exit 0 means the invoked process exited successfully, NOT that PB is playable.\n\
PB login, patching and anti-cheat remain handled by the official software.");
}

fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Options> {
    let mut iter = args.into_iter();
    let mut state = env::current_dir()?.join("state");
    let mut wine = None;
    let mut timeout = 0;
    let mut debug = "-all,+seh".to_string();
    let command = loop {
        let Some(arg) = iter.next() else { return Err("missing command; use --help".into()); };
        match arg.to_str() {
            Some("--state") => state = PathBuf::from(iter.next().ok_or("--state needs a directory")?),
            Some("--wine") => wine = Some(PathBuf::from(iter.next().ok_or("--wine needs a file")?)),
            Some("--timeout") => timeout = iter.next().ok_or("--timeout needs seconds")?.to_str()
                .ok_or("invalid timeout")?.parse()?,
            Some("--debug") => debug = iter.next().ok_or("--debug needs a value")?.into_string()
                .map_err(|_| "debug value must be UTF-8")?,
            Some("--help" | "-h" | "help") => { help(); return Err("__help__".into()); }
            Some(s) if s.starts_with('-') => return Err(format!("unknown option: {s}").into()),
            Some(s) => break s.to_string(),
            None => return Err("command must be UTF-8".into()),
        }
    };
    if !state.is_absolute() { state = env::current_dir()?.join(state); }
    Ok(Options { state, wine, timeout, debug, command, args: iter.collect() })
}

fn json(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""), '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"), '\r' => out.push_str("\\r"), '\t' => out.push_str("\\t"),
            c if c < '\u{20}' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"'); out
}

fn stamp() -> u128 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() }

fn failure_hint(stderr: &str, code: i32) -> &'static str {
    if stderr.contains("Can't check in server_mach_port") || stderr.contains("cannot find the server port") {
        "host_ipc_blocked"
    } else if code != 0 {
        "process_failed"
    } else {
        "process_exited_only"
    }
}

fn resolve_wine(opt: &Options) -> Result<PathBuf> {
    let p = match &opt.wine {
        Some(p) => p.clone(),
        None => PathBuf::from(fs::read_to_string(opt.state.join("engine.path"))?.trim()),
    };
    let p = fs::canonicalize(&p).map_err(|e| format!("runtime {}: {e}", p.display()))?;
    if !p.is_file() { return Err("Wine executable is not a file".into()); }
    Ok(p)
}

fn prefix(opt: &Options) -> PathBuf { opt.state.join("prefix") }

fn assert_owned(opt: &Options) -> Result<()> {
    let marker = opt.state.join("prefix.owner");
    if fs::read_to_string(marker).unwrap_or_default() != OWNER {
        return Err("prefix is not owned by PB Runner; run init with a dedicated --state directory".into());
    }
    let root = fs::canonicalize(&opt.state)?;
    let p = fs::canonicalize(prefix(opt))?;
    if p != root.join("prefix") || fs::symlink_metadata(prefix(opt))?.file_type().is_symlink() {
        return Err("refusing a prefix symlink or a prefix outside the dedicated state directory".into());
    }
    Ok(())
}

fn runtime_command(opt: &Options, wine: &Path) -> Command {
    let mut cmd = Command::new(wine);
    // Inherited wrapper overrides can silently substitute the wrong DLL/runtime.
    for key in ["WINEDLLOVERRIDES", "WINESERVER", "WINELOADER", "WINEARCH", "WINEDEBUG",
        "WINEPREFIX", "WINEESYNC", "WINEFSYNC", "WINEMSYNC", "WINE_DISABLE_FAST_PATH",
        "DYLD_LIBRARY_PATH", "DYLD_FALLBACK_LIBRARY_PATH", "LD_LIBRARY_PATH",
        "VK_DRIVER_FILES", "VK_ICD_FILENAMES",
        "STEAM_COMPAT_DATA_PATH", "STEAM_COMPAT_CLIENT_INSTALL_PATH", "SteamAppId"] {
        cmd.env_remove(key);
    }
    cmd.env("WINEPREFIX", prefix(opt));
    cmd.env("WINEDEBUG", &opt.debug);
    // Baseline uses Wine server synchronization rather than wrapper-specific modes.
    cmd.env("WINEESYNC", "0").env("WINEFSYNC", "0").env("WINEMSYNC", "0");
    if let Some(bin) = wine.parent() {
        let lib = bin.join("../lib");
        if lib.is_dir() {
            cmd.env("DYLD_FALLBACK_LIBRARY_PATH", &lib);
            let icd = lib.join("wine/x86_64-unix/vulkan/icd.d/MoltenVK_icd.json");
            if icd.is_file() { cmd.env("VK_DRIVER_FILES", &icd).env("VK_ICD_FILENAMES", &icd); }
        }
        let mut paths = vec![bin.to_path_buf()];
        if let Some(old) = env::var_os("PATH") { paths.extend(env::split_paths(&old)); }
        if let Ok(paths) = env::join_paths(paths) { cmd.env("PATH", paths); }
    }
    cmd
}

fn tee<R: Read>(mut source: R, path: PathBuf, stderr: bool) -> io::Result<()> {
    let mut file = File::create(path)?;
    let mut buf = [0u8; 8192];
    loop {
        let size = source.read(&mut buf)?;
        if size == 0 { break; }
        file.write_all(&buf[..size])?;
        file.flush()?;
        if stderr { let _ = io::stderr().write_all(&buf[..size]); }
        else { let _ = io::stdout().write_all(&buf[..size]); }
    }
    Ok(())
}

fn stop_owned(opt: &Options, wine: &Path) -> Result<()> {
    assert_owned(opt)?;
    let server = wine.parent().ok_or("Wine has no parent directory")?.join("wineserver");
    if !server.is_file() { return Err("matching wineserver executable is missing".into()); }
    let status = runtime_command(opt, &server).arg("-k").status()?;
    if !status.success() { return Err(format!("wineserver stop failed: {status}").into()); }
    Ok(())
}

fn run_logged(opt: &Options, wine: &Path, args: &[OsString], cwd: Option<&Path>) -> Result<i32> {
    run_program_logged(opt, wine, wine, args, cwd, &[])
}

fn run_program_logged(opt: &Options, wine: &Path, executable: &Path, args: &[OsString], cwd: Option<&Path>, extra_env: &[(OsString, OsString)]) -> Result<i32> {
    assert_owned(opt)?;
    let id = format!("{}-{}", stamp(), std::process::id());
    let session = opt.state.join("logs").join(&id);
    fs::create_dir_all(&session)?;
    let mut cmd = runtime_command(opt, executable);
    cmd.envs(extra_env.iter().cloned());
    cmd.args(args).stdin(Stdio::inherit()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(cwd) = cwd { cmd.current_dir(cwd); }
    let args_json = args.iter().map(|a| json(&a.to_string_lossy())).collect::<Vec<_>>().join(", ");
    let manifest = format!("{{\n  \"runner_version\": \"{}\",\n  \"runtime\": {},\n  \"program\": {},\n  \"prefix\": {},\n  \"args\": [{}],\n  \"cwd\": {},\n  \"debug\": {},\n  \"timeout_seconds\": {}\n}}\n",
        env!("CARGO_PKG_VERSION"),json(&wine.to_string_lossy()),json(&executable.to_string_lossy()),json(&prefix(opt).to_string_lossy()),
        args_json,json(&cwd.map(|p|p.display().to_string()).unwrap_or_default()),json(&opt.debug),opt.timeout);
    fs::write(session.join("invocation.json"), manifest)?;
    fs::write(opt.state.join("last-session"), &id)?;
    eprintln!("Session logs: {}", session.display());
    let start = Instant::now();
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(error) => {
            fs::write(session.join("result.json"), format!("{{\"spawn_error\":{},\"playability_verified\":false}}\n",json(&error.to_string())))?;
            return Err(error.into());
        }
    };
    fs::write(session.join("process.pid"),child.id().to_string())?;
    let stdout = child.stdout.take().ok_or("stdout unavailable")?;
    let stderr = child.stderr.take().ok_or("stderr unavailable")?;
    let out_path = session.join("stdout.log"); let err_path = session.join("stderr.log");
    let out_thread = thread::spawn(move || tee(stdout, out_path, false));
    let err_thread = thread::spawn(move || tee(stderr, err_path, true));
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? { break status; }
        if opt.timeout != 0 && start.elapsed() >= Duration::from_secs(opt.timeout) {
            timed_out = true;
            eprintln!("Timeout: stopping this owned prefix.");
            let _ = stop_owned(opt, wine);
            let _ = child.kill();
            break child.wait()?;
        }
        thread::sleep(Duration::from_millis(100));
    };
    // ZSelfUpdater restarts ZLauncher. Keep the readers alive after the first
    // loader exits: closing the pipes can make Console.WriteLine in the new
    // managed process throw IOException and break its download workers.
    let follow_launcher = opt.command == "run" && (opt.args.is_empty()
        || opt.args[0].to_string_lossy().to_ascii_lowercase().ends_with("zlauncher.exe"));
    if follow_launcher && !timed_out && !(out_thread.is_finished() && err_thread.is_finished()) {
        eprintln!("Loader exited; continuing to capture the launcher descendants. Use stop to end this prefix.");
        while !(out_thread.is_finished() && err_thread.is_finished()) {
            if opt.timeout != 0 && start.elapsed() >= Duration::from_secs(opt.timeout) {
                timed_out = true;
                let _ = stop_owned(opt, wine);
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
    // Boot/configuration commands can leave service processes with inherited
    // handles; they retain the bounded drain used for non-launcher commands.
    let drain = Instant::now();
    while !(out_thread.is_finished() && err_thread.is_finished()) && drain.elapsed() < Duration::from_secs(2) {
        thread::sleep(Duration::from_millis(50));
    }
    let streams_complete = out_thread.is_finished() && err_thread.is_finished();
    if streams_complete {
        out_thread.join().map_err(|_| "stdout capture thread panicked")??;
        err_thread.join().map_err(|_| "stderr capture thread panicked")??;
    }
    let code = if timed_out { 124 } else { status.code().unwrap_or(1) };
    let stderr_text = fs::read_to_string(session.join("stderr.log")).unwrap_or_default();
    let hint = failure_hint(&stderr_text, code);
    fs::write(session.join("result.json"),format!("{{\n  \"exit_code\": {code},\n  \"timed_out\": {timed_out},\n  \"elapsed_ms\": {},\n  \"streams_complete\": {streams_complete},\n  \"observation\": {},\n  \"playability_verified\": false\n}}\n",start.elapsed().as_millis(),json(hint)))?;
    eprintln!("Process exit: {code}. PB playability has NOT been verified.");
    if hint == "host_ipc_blocked" { eprintln!("Host blocked Wine's macOS IPC. This is not evidence of a Point Blank crash."); }
    if !streams_complete { eprintln!("Descendants retained output pipes; this session log may be incomplete."); }
    Ok(code)
}

fn windows_path(opt: &Options, input: &OsStr) -> Result<PathBuf> {
    let text = input.to_string_lossy();
    if text.len() >= 3 && text.as_bytes().get(..2).is_some_and(|b| b.eq_ignore_ascii_case(b"c:"))
        && (text.as_bytes()[2] == b'\\' || text.as_bytes()[2] == b'/') {
        let relative = PathBuf::from(text[3..].replace('\\', "/"));
        if relative.components().any(|c| !matches!(c, Component::Normal(_))) {
            return Err("Windows path must stay inside the prefix's C drive".into());
        }
        Ok(prefix(opt).join("drive_c").join(relative))
    } else {
        Ok(fs::canonicalize(input)?)
    }
}

fn doctor(opt: &Options, wine: &Path) -> Result<i32> {
    println!("Runtime: {}\nState: {}\nPrefix: {}",wine.display(),opt.state.display(),prefix(opt).display());
    let result = runtime_command(opt,wine).arg("--version").output()?;
    print!("{}",String::from_utf8_lossy(&result.stdout));
    eprint!("{}",String::from_utf8_lossy(&result.stderr));
    println!("Owned prefix: {}",assert_owned(opt).is_ok());
    if cfg!(target_os="macos") {
        let rosetta = Command::new("/usr/bin/arch").args(["-x86_64","/usr/bin/true"]).status()?;
        println!("x86_64 execution: {}",rosetta.success());
    }
    Ok(result.status.code().unwrap_or(1))
}

fn init(opt: &Options, wine: &Path) -> Result<i32> {
    fs::create_dir_all(&opt.state)?;
    if prefix(opt).exists() {
        assert_owned(opt)?;
        if opt.state.join("prefix.ready").is_file() {
            println!("This runner's prefix is already initialized.");
            return Ok(0);
        }
        println!("Continuing initialization of an incomplete, runner-owned prefix.");
    } else {
        fs::create_dir(prefix(opt))?;
        let mut marker = OpenOptions::new().write(true).create_new(true).open(opt.state.join("prefix.owner"))?;
        marker.write_all(OWNER.as_bytes())?;
    }
    fs::write(opt.state.join("engine.path"),wine.to_string_lossy().as_bytes())?;
    let code = run_logged(opt,wine,&["wineboot.exe".into(),"--init".into()],None)?;
    if code != 0 { return Ok(code); }
    let code = run_logged(opt,wine,&["winecfg.exe".into(),"-v".into(),"win10".into()],None)?;
    if code == 0 { fs::write(opt.state.join("prefix.ready"),"wineboot completed; Windows 10 selected\n")?; }
    Ok(code)
}

fn status(opt: &Options) -> Result<i32> {
    assert_owned(opt)?;
    let launcher = windows_path(opt,OsStr::new(DEFAULT_LAUNCHER))?;
    let client = prefix(opt).join("drive_c/Zepetto/PointBlank (ID)/PointBlank.exe");
    println!("Prefix initialized: {}\nOfficial launcher present: {}\nClient at default path present: {}",opt.state.join("prefix.ready").is_file(),launcher.is_file(),client.is_file());
    if let Ok(last) = fs::read_to_string(opt.state.join("last-session")) {
        println!("Last session: {}",last.trim());
        let dir = opt.state.join("logs").join(last.trim());
        match fs::read_to_string(dir.join("result.json")) {
            Ok(s) => print!("{s}"), Err(_) => println!("No completed result; run may be active or interrupted."),
        }
    }
    println!("Online match verified: no (requires an observed successful match).");
    Ok(0)
}

fn dotnet(opt: &Options, wine: &Path) -> Result<i32> {
    assert_owned(opt)?;
    let script = fs::canonicalize(opt.args.first().ok_or("dotnet needs the verified Winetricks script path")?)?;
    let tools = script.parent().ok_or("script needs a parent directory")?;
    let bin = wine.parent().ok_or("runtime needs a bin directory")?;
    let mut paths = vec![tools.to_path_buf(),bin.to_path_buf()];
    if let Some(old)=env::var_os("PATH") {paths.extend(env::split_paths(&old));}
    let extra = vec![
        (OsString::from("WINE"),wine.as_os_str().to_os_string()),
        (OsString::from("WINE64"),wine.as_os_str().to_os_string()),
        (OsString::from("WINESERVER"),bin.join("wineserver").into_os_string()),
        (OsString::from("WINEARCH"),OsString::from("win64")),
        (OsString::from("W_CACHE"),opt.state.join("cache").into_os_string()),
        (OsString::from("XDG_CACHE_HOME"),opt.state.join("cache").into_os_string()),
        (OsString::from("WINETRICKS_GUI"),OsString::from("none")),
        (OsString::from("PATH"),env::join_paths(paths)?),
    ];
    let code=run_program_logged(opt,wine,Path::new("/bin/sh"),&[script.as_os_str().to_os_string(),"dotnet48".into()],Some(&opt.state),&extra)?;
    if code!=0 {return Ok(code);}
    run_logged(opt,wine,&["winecfg.exe".into(),"-v".into(),"win10".into()],None)
}

fn process_snapshot(opt: &Options, wine: &Path) -> Result<i32> {
    assert_owned(opt)?;
    let output = runtime_command(opt, wine).env("WINEDEBUG", "-all,err+all")
        .args(["tasklist.exe", "/FO", "CSV", "/NH"]).output()?;
    fs::write(opt.state.join("process-snapshot.csv"), &output.stdout)?;
    fs::write(opt.state.join("process-snapshot.stderr.log"), &output.stderr)?;
    let code = output.status.code().unwrap_or(1);
    fs::write(opt.state.join("process-snapshot.json"), format!("{{\"timestamp_ms\":{},\"exit_code\":{}}}\n", stamp(), code))?;
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    Ok(code)
}

fn execute(mut opt: Options) -> Result<i32> {
    // An owned-state switch makes startup traces reproducible across GUI restarts.
    if opt.command == "run" && opt.state.join("startup-trace.enabled").is_file() {
        assert_owned(&opt)?;
        opt.debug = "-all,+timestamp,+pid,+tid,+seh,+loaddll,+process".into();
    }
    if opt.command == "status" { return status(&opt); }
    let wine = resolve_wine(&opt)?;
    match opt.command.as_str() {
        "doctor" => doctor(&opt,&wine),
        "processes" => process_snapshot(&opt,&wine),
        "init" => init(&opt,&wine),
        "dotnet" => dotnet(&opt,&wine),
        "stop" => { stop_owned(&opt,&wine)?; println!("Stopped this runner's prefix."); Ok(0) }
        "install" | "run" => {
            assert_owned(&opt)?;
            let input = opt.args.first().cloned().unwrap_or_else(||DEFAULT_LAUNCHER.into());
            if opt.command == "install" && opt.args.is_empty() { return Err("install needs an installer file".into()); }
            let host_path = windows_path(&opt,&input)?;
            if !host_path.is_file() { return Err(format!("executable not found: {}",host_path.display()).into()); }
            let mut args = if host_path.extension().is_some_and(|s|s.eq_ignore_ascii_case("msi")) {
                vec!["msiexec.exe".into(),"/i".into(),host_path.as_os_str().to_os_string()]
            } else { vec![host_path.as_os_str().to_os_string()] };
            args.extend(opt.args.iter().skip(1).cloned());
            run_logged(&opt,&wine,&args,host_path.parent())
        }
        other => Err(format!("unknown command: {other}; use --help").into()),
    }
}

fn main() -> ExitCode {
    let result = parse(env::args_os().skip(1)).and_then(execute);
    match result {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(e) if e.to_string()=="__help__" => ExitCode::SUCCESS,
        Err(e) => { eprintln!("PB Runner: {e}"); ExitCode::FAILURE }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn options(root: &Path) -> Options { Options {state:root.into(),wine:None,timeout:0,debug:"-all".into(),command:"run".into(),args:vec![]} }
    #[test]
    fn windows_paths_cannot_escape_prefix() {
        let root=env::temp_dir().join("pb-test-paths"); let opt=options(&root);
        assert!(windows_path(&opt,OsStr::new(r"C:\..\outside.exe")).is_err());
        assert_eq!(windows_path(&opt,OsStr::new(DEFAULT_LAUNCHER)).unwrap(),root.join("prefix/drive_c/Zepetto/Z-Launcher(ID)/ZLauncher.exe"));
    }
    #[test]
    fn refuses_unowned_and_symlinked_prefixes() {
        let root=env::temp_dir().join(format!("pb-ownership-{}",stamp()));
        fs::create_dir_all(root.join("prefix")).unwrap(); let opt=options(&root);
        assert!(assert_owned(&opt).is_err());
        fs::write(root.join("prefix.owner"),OWNER).unwrap(); assert!(assert_owned(&opt).is_ok());
        #[cfg(unix)] {
            fs::remove_dir(root.join("prefix")).unwrap();
            std::os::unix::fs::symlink(env::temp_dir(),root.join("prefix")).unwrap();
            assert!(assert_owned(&opt).is_err());
            fs::remove_file(root.join("prefix")).unwrap();
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn arguments_after_command_are_not_reinterpreted() {
        let opt=parse(["run","C:\\PB\\ZLauncher.exe","--timeout","10"].map(OsString::from)).unwrap();
        assert_eq!(opt.timeout,0); assert_eq!(opt.args.len(),3);
    }
    #[test]
    fn manifest_strings_escape_control_characters() { assert_eq!(json("a\n\"\\"),"\"a\\n\\\"\\\\\""); }
    #[test]
    fn host_ipc_failure_is_not_classified_as_game_failure() {
        assert_eq!(failure_hint("wineserver: Can't check in server_mach_port",1),"host_ipc_blocked");
        assert_eq!(failure_hint("",0),"process_exited_only");
    }
    #[test]
    #[cfg(unix)]
    fn records_real_exit_and_both_streams_without_shell_interpolation() {
        use std::os::unix::fs::PermissionsExt;
        let root=env::temp_dir().join(format!("pb-process-{}",stamp()));
        fs::create_dir_all(root.join("prefix")).unwrap(); fs::write(root.join("prefix.owner"),OWNER).unwrap();
        let fake=root.join("mock wine");
        fs::write(&fake,"#!/bin/sh\nprintf '%s\\n' \"$WINEPREFIX\"\nprintf '%s\\n' \"$1\" >&2\nexit 7\n").unwrap();
        fs::set_permissions(&fake,fs::Permissions::from_mode(0o755)).unwrap();
        let opt=options(&root); let literal=OsString::from("$(touch DO_NOT_CREATE)");
        assert_eq!(run_logged(&opt,&fake,&[literal],None).unwrap(),7);
        let id=fs::read_to_string(root.join("last-session")).unwrap(); let log=root.join("logs").join(id);
        assert!(fs::read_to_string(log.join("stderr.log")).unwrap().contains("$(touch DO_NOT_CREATE)"));
        assert!(fs::read_to_string(log.join("stdout.log")).unwrap().contains(&root.display().to_string()));
        assert!(fs::read_to_string(log.join("result.json")).unwrap().contains("\"exit_code\": 7"));
        assert!(!root.join("DO_NOT_CREATE").exists()); fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[cfg(unix)]
    fn launcher_descendants_can_write_after_loader_exit() {
        use std::os::unix::fs::PermissionsExt;
        let root=env::temp_dir().join(format!("pb-descendants-{}",stamp()));
        fs::create_dir_all(root.join("prefix")).unwrap();
        fs::write(root.join("prefix.owner"),OWNER).unwrap();
        let fake=root.join("mock-wine");
        fs::write(&fake,"#!/bin/sh\n(sleep 3; printf 'late launcher output\\n'; printf 'late stderr\\n' >&2) &\nexit 0\n").unwrap();
        fs::set_permissions(&fake,fs::Permissions::from_mode(0o755)).unwrap();
        let opt=options(&root);
        assert_eq!(run_logged(&opt,&fake,&[],None).unwrap(),0);
        let id=fs::read_to_string(root.join("last-session")).unwrap();
        let log=root.join("logs").join(id);
        assert!(fs::read_to_string(log.join("stdout.log")).unwrap().contains("late launcher output"));
        assert!(fs::read_to_string(log.join("stderr.log")).unwrap().contains("late stderr"));
        assert!(fs::read_to_string(log.join("result.json")).unwrap().contains("\"streams_complete\": true"));
        fs::remove_dir_all(root).unwrap();
    }
}
