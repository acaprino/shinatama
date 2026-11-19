//! P14 probe: Windows process containment (register P14, plan step C09). Not part of the product.
//!
//! `child` is a hostile synthetic child that spawns a grandchild at once. Terminals are opened
//! with several strategies and then stopped or orphaned (kill of the app) while membership of
//! the child and grandchild in the session job and in the global job is checked directly.
//! Every spawned process sleeps at most MAX_SLEEP seconds, so a leak cleans itself up.
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::any::Any;
use std::ffi::c_void;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::windows::io::{FromRawHandle, RawHandle};
use std::path::{Path, PathBuf};
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use std::{mem, ptr, thread};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::Console::*;
use windows_sys::Win32::System::JobObjects::*;
use windows_sys::Win32::System::Pipes::*;
use windows_sys::Win32::System::Threading::*;

type Res<T> = Result<T, Box<dyn std::error::Error>>;
const MAX_SLEEP: u64 = 120;
// SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_QUOTA | PROCESS_TERMINATE
const ACCESS: u32 = 0x0010_0000 | 0x1000 | 0x0100 | 0x0001;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Strat {
    /// portable-pty spawns, then the process is assigned to the session job (as crates/terminal does).
    Portable,
    /// Direct ConPTY, CREATE_SUSPENDED, assign to the session job, then resume.
    Suspended,
    /// Direct ConPTY with PROC_THREAD_ATTRIBUTE_JOB_LIST next to the pseudoconsole attribute.
    JobList,
}
#[derive(Clone, Copy, Debug)]
enum Race {
    /// Attach immediately after the spawn call returns, as production does.
    Natural,
    /// Wait until the grandchild exists, then attach.
    Forced,
    /// Sleep before attaching; the reader answering the console may start first or after.
    Delay { ms: u64, reader_first: bool },
}

struct Term {
    job: HANDLE,
    hchild: HANDLE,
    hgc: HANDLE,
    child_pid: u32,
    gc_pid: u32,
    attach_us: u128,
    ran_before: bool,
    hpc: Option<HPCON>,
    _keep: Vec<Box<dyn Any>>,
}

fn last() -> std::io::Error {
    std::io::Error::last_os_error()
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn make_job() -> Res<HANDLE> {
    unsafe {
        let job = CreateJobObjectW(ptr::null(), ptr::null());
        if job.is_null() {
            return Err(format!("CreateJobObjectW: {}", last()).into());
        }
        let mut lim: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = mem::zeroed();
        lim.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &lim as *const _ as *const c_void,
            mem::size_of_val(&lim) as u32,
        ) == 0
        {
            return Err(format!("SetInformationJobObject: {}", last()).into());
        }
        Ok(job)
    }
}
fn join_global_job() -> Res<HANDLE> {
    let job = make_job()?;
    unsafe {
        if AssignProcessToJobObject(job, GetCurrentProcess()) == 0 {
            return Err(format!("AssignProcessToJobObject(self): {}", last()).into());
        }
    }
    Ok(job)
}
fn open_proc(pid: u32) -> Res<HANDLE> {
    let h = unsafe { OpenProcess(ACCESS, 0, pid) };
    if h.is_null() {
        return Err(format!("OpenProcess({pid}): {}", last()).into());
    }
    Ok(h)
}
fn alive(h: HANDLE) -> bool {
    unsafe { WaitForSingleObject(h, 0) == WAIT_TIMEOUT }
}
fn in_job(process: HANDLE, job: HANDLE) -> bool {
    let mut r = 0;
    unsafe { IsProcessInJob(process, job, &mut r) };
    r != 0
}
fn kill(h: HANDLE) {
    unsafe { TerminateProcess(h, 9) };
}
fn exe_path() -> Res<String> {
    Ok(std::env::current_exe()?.to_string_lossy().into_owned())
}
fn wait_file(path: &Path, secs: u64) -> Res<String> {
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        if let Ok(s) = fs::read_to_string(path) {
            if s.ends_with('\n') {
                return Ok(s);
            }
        }
        thread::sleep(Duration::from_millis(5));
    }
    Err(format!("timeout waiting for {}", path.display()).into())
}
fn write_atomic(path: &Path, text: &str) -> Res<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)?;
    Ok(())
}
fn pids(s: &str) -> Vec<u32> {
    s.split_whitespace().filter_map(|p| p.parse().ok()).collect()
}
fn image_name(h: HANDLE) -> String {
    let mut buf = [0u16; 512];
    let mut len = buf.len() as u32;
    if unsafe { QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len) } == 0 {
        return "?".into();
    }
    let full = String::from_utf16_lossy(&buf[..len as usize]);
    full.rsplit('\\').next().unwrap_or("?").to_string()
}

/// Answers the console's cursor-position request, which ConPTY issues before the client runs.
fn spawn_reader(mut reader: impl Read + Send + 'static, mut writer: impl Write + Send + 'static) {
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            if std::env::var("P14_NO_DSR").is_err() && buf[..n].windows(4).any(|w| w == b"\x1b[6n") {
                let _ = writer.write_all(b"\x1b[1;1R");
                let _ = writer.flush();
            }
        }
    });
}

fn child_mode(pidfile: &str) -> Res<()> {
    let mut cmd = Command::new(exe_path()?);
    cmd.arg("idle");
    if std::env::var("P14_DETACHED").as_deref() == Ok("1") {
        // A daemon-style grandchild: no console, so closing the pseudoconsole cannot reach it.
        cmd.creation_flags(0x0000_0008).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    }
    let gc = cmd.spawn()?;
    write_atomic(Path::new(pidfile), &format!("{} {}\n", std::process::id(), gc.id()))?;
    thread::sleep(Duration::from_secs(MAX_SLEEP));
    Ok(())
}

unsafe fn open_conpty() -> Res<(HPCON, File, File)> {
    let (mut in_r, mut in_w, mut out_r, mut out_w): (HANDLE, HANDLE, HANDLE, HANDLE) =
        (ptr::null_mut(), ptr::null_mut(), ptr::null_mut(), ptr::null_mut());
    if CreatePipe(&mut in_r, &mut in_w, ptr::null(), 0) == 0 || CreatePipe(&mut out_r, &mut out_w, ptr::null(), 0) == 0 {
        return Err(format!("CreatePipe: {}", last()).into());
    }
    let mut hpc: HPCON = mem::zeroed();
    let hr = CreatePseudoConsole(COORD { X: 120, Y: 30 }, in_r, out_w, 0, &mut hpc);
    CloseHandle(in_r);
    CloseHandle(out_w);
    if hr != 0 {
        return Err(format!("CreatePseudoConsole: HRESULT {hr:#x}").into());
    }
    Ok((hpc, File::from_raw_handle(out_r as RawHandle), File::from_raw_handle(in_w as RawHandle)))
}

unsafe fn spawn_direct(hpc: HPCON, job: HANDLE, joblist: bool, suspended: bool, cmdline: &str) -> Res<PROCESS_INFORMATION> {
    let count = if joblist { 2 } else { 1 };
    let mut size = 0usize;
    InitializeProcThreadAttributeList(ptr::null_mut(), count, 0, &mut size);
    let mut buf = vec![0u64; size.div_ceil(8)];
    let list = buf.as_mut_ptr() as *mut c_void;
    if InitializeProcThreadAttributeList(list, count, 0, &mut size) == 0 {
        return Err(format!("InitializeProcThreadAttributeList: {}", last()).into());
    }
    if UpdateProcThreadAttribute(list, 0, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize, hpc as *const c_void, mem::size_of::<HPCON>(), ptr::null_mut(), ptr::null()) == 0 {
        return Err(format!("UpdateProcThreadAttribute(pseudoconsole): {}", last()).into());
    }
    let jobs = [job];
    if joblist
        && UpdateProcThreadAttribute(list, 0, PROC_THREAD_ATTRIBUTE_JOB_LIST as usize, jobs.as_ptr() as *const c_void, mem::size_of::<HANDLE>(), ptr::null_mut(), ptr::null()) == 0
    {
        return Err(format!("UpdateProcThreadAttribute(job list): {}", last()).into());
    }
    let mut si: STARTUPINFOEXW = mem::zeroed();
    si.StartupInfo.cb = mem::size_of::<STARTUPINFOEXW>() as u32;
    si.lpAttributeList = list;
    let mut pi: PROCESS_INFORMATION = mem::zeroed();
    let mut cl = wide(cmdline);
    let flags = EXTENDED_STARTUPINFO_PRESENT | if suspended { CREATE_SUSPENDED } else { 0 };
    let ok = CreateProcessW(ptr::null(), cl.as_mut_ptr(), ptr::null(), ptr::null(), 0, flags, ptr::null(), ptr::null(), &si.StartupInfo, &mut pi);
    let err = last();
    DeleteProcThreadAttributeList(list);
    if ok == 0 {
        return Err(format!("CreateProcessW: {err}").into());
    }
    Ok(pi)
}

fn open_term(strat: Strat, race: Race, name: &str, dir: &Path) -> Res<Term> {
    fs::create_dir_all(dir)?;
    let pidfile = dir.join(format!("{name}.pid"));
    let _ = fs::remove_file(&pidfile);
    let job = make_job()?;
    let exe = exe_path()?;
    let mut keep: Vec<Box<dyn Any>> = Vec::new();
    let mut hpc = None;
    let (hchild, attach_us, ran_before);
    match strat {
        Strat::Portable => {
            let pair = native_pty_system().openpty(PtySize { rows: 30, cols: 120, pixel_width: 0, pixel_height: 0 })?;
            let mut cmd = CommandBuilder::new(&exe);
            cmd.arg("child");
            cmd.arg(pidfile.to_string_lossy().as_ref());
            let child = pair.slave.spawn_command(cmd)?;
            let t0 = Instant::now();
            drop(pair.slave);
            let pid = child.process_id().ok_or("no process id")?;
            let reader_first = matches!(race, Race::Forced | Race::Delay { reader_first: true, .. });
            let start_reader = |m: &Box<dyn portable_pty::MasterPty + Send>| -> Res<()> {
                spawn_reader(m.try_clone_reader()?, m.take_writer()?);
                Ok(())
            };
            if reader_first {
                start_reader(&pair.master)?;
            }
            let mut t_wait = Duration::ZERO;
            match race {
                Race::Forced => {
                    wait_file(&pidfile, 15)?;
                    t_wait = t0.elapsed();
                }
                Race::Delay { ms, .. } => {
                    thread::sleep(Duration::from_millis(ms));
                    t_wait = t0.elapsed();
                }
                Race::Natural => {}
            }
            ran_before = pidfile.exists();
            let t1 = Instant::now();
            unsafe {
                let h = OpenProcess(0x0100 | 0x0001, 0, pid);
                if h.is_null() {
                    return Err(format!("OpenProcess(assign): {}", last()).into());
                }
                let assigned = AssignProcessToJobObject(job, h);
                let err = last();
                CloseHandle(h);
                if assigned == 0 {
                    return Err(format!("AssignProcessToJobObject: {err}").into());
                }
            }
            attach_us = t1.elapsed().as_micros() + if matches!(race, Race::Natural) { 0 } else { 0 * t_wait.as_micros() };
            if !reader_first {
                start_reader(&pair.master)?;
            }
            hchild = open_proc(pid)?;
            keep.push(Box::new(child));
            keep.push(Box::new(pair.master));
        }
        Strat::Suspended | Strat::JobList => unsafe {
            let (h, out, inp) = open_conpty()?;
            spawn_reader(out, inp);
            hpc = Some(h);
            let cmdline = format!("\"{exe}\" child \"{}\"", pidfile.display());
            let t0 = Instant::now();
            let pi = if strat == Strat::JobList {
                spawn_direct(h, job, true, false, &cmdline)?
            } else {
                spawn_direct(h, job, false, true, &cmdline)?
            };
            if strat == Strat::Suspended {
                if AssignProcessToJobObject(job, pi.hProcess) == 0 {
                    return Err(format!("AssignProcessToJobObject: {}", last()).into());
                }
                if ResumeThread(pi.hThread) == u32::MAX {
                    return Err(format!("ResumeThread: {}", last()).into());
                }
            }
            attach_us = t0.elapsed().as_micros();
            ran_before = false;
            CloseHandle(pi.hThread);
            hchild = pi.hProcess;
        },
    }
    let ids = pids(&wait_file(&pidfile, 15)?);
    let (child_pid, gc_pid) = (ids[0], ids[1]);
    let hgc = open_proc(gc_pid)?;
    Ok(Term { job, hchild, hgc, child_pid, gc_pid, attach_us, ran_before, hpc, _keep: keep })
}

fn close_term(t: Term) {
    unsafe {
        TerminateJobObject(t.job, 1);
        kill(t.hchild);
        kill(t.hgc);
        if let Some(h) = t.hpc {
            ClosePseudoConsole(h);
        }
        CloseHandle(t.hchild);
        CloseHandle(t.hgc);
        CloseHandle(t.job);
    }
}

fn close_pty(t: &mut Term) {
    if let Some(h) = t.hpc.take() {
        unsafe { ClosePseudoConsole(h) };
    }
    t._keep.clear();
}

fn parse_strat(s: &str) -> Res<Strat> {
    Ok(match s {
        "portable" => Strat::Portable,
        "suspended" => Strat::Suspended,
        "joblist" => Strat::JobList,
        _ => return Err(format!("strategy {s}").into()),
    })
}

/// app <strategy> <global 0|1> <stop-a|kill-app> <dir>
fn app(a: &[String]) -> Res<()> {
    thread::spawn(|| {
        thread::sleep(Duration::from_secs(MAX_SLEEP + 30));
        std::process::exit(3);
    });
    let strat = parse_strat(&a[0])?;
    let global = if a[1] == "1" { Some(join_global_job()?) } else { None };
    let dir = PathBuf::from(&a[3]);
    let race = Race::Forced;
    println!(
        "RESULT strategy={:?} global_job={} race={:?} action={} grandchild_detached={}",
        strat, global.is_some(), race, a[2], std::env::var("P14_DETACHED").as_deref() == Ok("1")
    );
    let mut ta = open_term(strat, race, "A", &dir)?;
    let tb = open_term(strat, race, "B", &dir)?;
    for (n, t) in [("A", &ta), ("B", &tb)] {
        let g = |h| global.map(|gj| in_job(h, gj)).map(|b| b.to_string()).unwrap_or("-".into());
        println!(
            "RESULT {n}: child_in_session_job={} grandchild_in_session_job={} child_in_global={} grandchild_in_global={} child_ran_before_attach={}",
            in_job(t.hchild, t.job), in_job(t.hgc, t.job), g(t.hchild), g(t.hgc), t.ran_before
        );
    }
    if a[2] == "stop-a" {
        unsafe { TerminateJobObject(ta.job, 1) };
        thread::sleep(Duration::from_secs(2));
        println!(
            "RESULT after job termination of A: A.child_alive={} A.grandchild_alive={} B.child_alive={} B.grandchild_alive={}",
            alive(ta.hchild), alive(ta.hgc), alive(tb.hchild), alive(tb.hgc)
        );
        close_pty(&mut ta);
        thread::sleep(Duration::from_secs(3));
        println!(
            "RESULT after job termination and pseudoconsole close of A (production stop): A.child_alive={} A.grandchild_alive={} B.child_alive={} B.grandchild_alive={}",
            alive(ta.hchild), alive(ta.hgc), alive(tb.hchild), alive(tb.hgc)
        );
        close_term(ta);
        close_term(tb);
    } else {
        write_atomic(&dir.join("ready"), &format!("{} {} {} {}\n", ta.child_pid, ta.gc_pid, tb.child_pid, tb.gc_pid))?;
        thread::sleep(Duration::from_secs(MAX_SLEEP));
    }
    Ok(())
}

fn out_dir(name: &str) -> PathBuf {
    let d = PathBuf::from(std::env::var("P14_OUT").unwrap_or_else(|_| "out".into())).join(name);
    let _ = fs::remove_dir_all(&d);
    d
}

fn poll_survivors(label: &str, handles: &[(String, HANDLE)]) {
    let start = Instant::now();
    for at in [1u64, 3, 10] {
        thread::sleep((start + Duration::from_secs(at)).saturating_duration_since(Instant::now()));
        let list: Vec<String> = handles.iter().filter(|(_, h)| alive(*h)).map(|(n, _)| n.clone()).collect();
        println!("RESULT {label}: after {at}s survivors={} {:?}", list.len(), list);
    }
    for (_, h) in handles {
        if alive(*h) {
            kill(*h);
        }
        unsafe { CloseHandle(*h) };
    }
}

fn kill_scenario(strat: &str, global: &str, detached: &str) -> Res<()> {
    let dir = out_dir(&format!("kill-{strat}-{global}-d{detached}"));
    fs::create_dir_all(&dir)?;
    let mut app = Command::new(exe_path()?)
        .args(["app", strat, global, "kill-app"])
        .arg(&dir)
        .env("P14_DETACHED", detached)
        .stdout(Stdio::piped())
        .spawn()?;
    let ready = wait_file(&dir.join("ready"), 40)?;
    let ids = pids(&ready);
    let names = ["A.child", "A.grandchild", "B.child", "B.grandchild"];
    let handles: Vec<(String, HANDLE)> = ids.iter().zip(names).map(|(p, n)| Ok((n.to_string(), open_proc(*p)?))).collect::<Res<_>>()?;
    app.kill()?;
    app.wait()?;
    let mut text = String::new();
    app.stdout.take().unwrap().read_to_string(&mut text)?;
    print!("{text}");
    println!("RESULT app killed (TerminateProcess)");
    poll_survivors(&format!("{strat}/global={global}/detached={detached}/kill-app"), &handles);
    Ok(())
}

fn stop_scenario(strat: &str, global: &str, detached: &str) -> Res<()> {
    let dir = out_dir(&format!("stop-{strat}-{global}-d{detached}"));
    fs::create_dir_all(&dir)?;
    let out = Command::new(exe_path()?).args(["app", strat, global, "stop-a"]).arg(&dir).env("P14_DETACHED", detached).output()?;
    print!("{}", String::from_utf8_lossy(&out.stdout));
    if !out.status.success() {
        println!("RESULT app exit {:?}: {}", out.status.code(), String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
}

fn trial_stats(label: &str, strat: Strat, race: Race, n: usize) -> Res<()> {
    let dir = out_dir(&format!("trial-{label}"));
    let (mut esc, mut ran, mut child_out) = (0, 0, 0);
    let mut us = Vec::new();
    for k in 0..n {
        let t = open_term(strat, race, &format!("t{k}"), &dir)?;
        if !in_job(t.hgc, t.job) {
            esc += 1;
        }
        if !in_job(t.hchild, t.job) {
            child_out += 1;
        }
        if t.ran_before {
            ran += 1;
        }
        us.push(t.attach_us);
        close_term(t);
    }
    us.sort();
    println!(
        "SWEEP {label}: grandchild_escaped={esc}/{n} child_not_in_job={child_out}/{n} child_ran_before_attach={ran}/{n} attach_us min={} median={} max={}",
        us[0], us[us.len() / 2], us[us.len() - 1]
    );
    Ok(())
}

fn sweep() -> Res<()> {
    println!("SWEEP delay before attach, portable-pty strategy, hostile child spawns a grandchild at start");
    for (first, tag) in [(true, "reader-first"), (false, "reader-late")] {
        for ms in [0u64, 1, 2, 3, 5, 8, 12, 20, 50, 200] {
            trial_stats(&format!("portable-delay{ms}ms-{tag}"), Strat::Portable, Race::Delay { ms, reader_first: first }, 10)?;
        }
    }
    trial_stats("portable-natural", Strat::Portable, Race::Natural, 60)?;
    trial_stats("suspended-natural", Strat::Suspended, Race::Natural, 30)?;
    trial_stats("joblist-natural", Strat::JobList, Race::Natural, 30)?;
    Ok(())
}

/// wrap-run <cmd...>: joins a kill-on-close job, then runs the command inside it.
fn wrap_run(a: &[String]) -> Res<()> {
    let gj = join_global_job()?;
    println!("RESULT wrapper in global job: {}", in_job(unsafe { GetCurrentProcess() }, gj));
    let status = Command::new(&a[0]).args(&a[1..]).status()?;
    println!("RESULT command exit code {:?}", status.code());
    std::process::exit(status.code().unwrap_or(1));
}

/// wrap-hold <readyfile> <exe>: joins a job, starts the exe, lists the job's processes, then waits.
fn wrap_hold(a: &[String]) -> Res<()> {
    let gj = join_global_job()?;
    let _child = Command::new(&a[1]).spawn()?;
    thread::sleep(Duration::from_secs(14));
    let mut buf = vec![0usize; 1024];
    let mut ret = 0u32;
    let ok = unsafe { QueryInformationJobObject(gj, JobObjectBasicProcessIdList, buf.as_mut_ptr() as *mut c_void, (buf.len() * 8) as u32, &mut ret) };
    if ok == 0 {
        return Err(format!("QueryInformationJobObject: {}", last()).into());
    }
    let n = (buf[0] >> 32) as usize;
    let me = std::process::id();
    let list: Vec<String> = buf[1..1 + n].iter().map(|p| *p as u32).filter(|p| *p != me).map(|p| p.to_string()).collect();
    write_atomic(Path::new(&a[0]), &format!("{}\n", list.join(" ")))?;
    thread::sleep(Duration::from_secs(MAX_SLEEP));
    Ok(())
}

fn real_app(a: &[String]) -> Res<()> {
    let dir = out_dir("real-app");
    fs::create_dir_all(&dir)?;
    let ready = dir.join("ready");
    let mut w = Command::new(exe_path()?).arg("wrap-hold").arg(&ready).arg(&a[0]).env("SHINATAMA_HOME", dir.join("home")).spawn()?;
    let ids = pids(&wait_file(&ready, 90)?);
    let mut handles = Vec::new();
    for p in &ids {
        if let Ok(h) = open_proc(*p) {
            handles.push((format!("{}#{p}", image_name(h)), h));
        }
    }
    println!("RESULT real app: {} processes in the global job: {:?}", handles.len(), handles.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>());
    w.kill()?;
    w.wait()?;
    println!("RESULT wrapper killed (TerminateProcess)");
    poll_survivors("real-app/kill-wrapper", &handles);
    Ok(())
}

fn driver() -> Res<()> {
    let det = std::env::args().nth(2).unwrap_or("0".into());
    for (strat, global) in [("portable", "0"), ("portable", "1"), ("suspended", "1"), ("joblist", "1")] {
        println!("SCENARIO stop-a {strat} global={global} detached={det}");
        stop_scenario(strat, global, &det)?;
    }
    for (strat, global) in [("portable", "0"), ("portable", "1"), ("suspended", "1"), ("joblist", "1")] {
        println!("SCENARIO kill-app {strat} global={global} detached={det}");
        kill_scenario(strat, global, &det)?;
    }
    Ok(())
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let r = match a.first().map(String::as_str) {
        Some("idle") => {
            thread::sleep(Duration::from_secs(MAX_SLEEP));
            Ok(())
        }
        Some("child") => child_mode(&a[1]),
        Some("app") => app(&a[1..]),
        Some("driver") => driver(),
        Some("sweep") => sweep(),
        Some("wrap-run") => wrap_run(&a[1..]),
        Some("wrap-hold") => wrap_hold(&a[1..]),
        Some("real-app") => real_app(&a[1..]),
        _ => Err("usage: idle|child|app|driver|sweep|wrap-run|wrap-hold|real-app".into()),
    };
    if let Err(e) = r {
        eprintln!("ERROR {e}");
        std::process::exit(2);
    }
}
