//! 会话感知采集拉起（参考 RustDesk 的 winlogon/explorer token + CreateProcessAsUser 方案）。
//!
//! 由系统服务（LocalSystem, Session 0）监视活动控制台会话，在用户登录后的默认桌面
//! （winsta0\default）中以该用户身份拉起 cli-capture，替代 ONLOGON 计划任务自启。
//!
//! 适用范围：仅处理“用户已登录后”的画面采集（含锁屏，锁屏时游戏画面静止）。
//! 不处理登录界面 / UAC 安全桌面等特权桌面。

use anyhow::{anyhow, Result};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{info, warn};
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::TOKEN_ALL_ACCESS;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
use windows::Win32::System::RemoteDesktop::{
    WTSGetActiveConsoleSessionId, WTSQuerySessionInformationW, WTSQueryUserToken, WTSFreeMemory,
    WTS_CONNECTSTATE_CLASS, WTS_CURRENT_SERVER_HANDLE, WTSActive, WTSConnectState,
    ProcessIdToSessionId,
};
use windows::Win32::System::Threading::{
    CreateProcessAsUserW, GetExitCodeProcess, OpenProcess, OpenProcessToken,
    CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, PROCESS_INFORMATION,
    PROCESS_QUERY_LIMITED_INFORMATION, STARTUPINFOW, WaitForSingleObject, STARTF_USESTDHANDLES,
};
use windows::Win32::Foundation::{SetHandleInformation, HANDLE_FLAG_INHERIT, HANDLE_FLAGS, WAIT_TIMEOUT};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::Security::SECURITY_ATTRIBUTES;

/// GetExitCodeProcess 的“仍在运行”返回值。
const STILL_ACTIVE: u32 = 259;

const POLL_INTERVAL: Duration = Duration::from_millis(500);
const SPAWN_ALIVE_CHECK_DELAY: Duration = Duration::from_millis(500);
const SPAWN_MAX_ATTEMPTS: usize = 3;

/// 会话内 `--stop-capture` helper 的等待上限（它自己只等 1.5 秒投递事件）。
const HELPER_TIMEOUT: Duration = Duration::from_secs(30);

struct LaunchState {
    session_id: Option<u32>,
    generation: u64,
}

/// 会话拉起器：后台监视活动会话，spawn_capture 以当前活动会话的用户身份拉起进程。
pub struct SessionLauncher {
    state: Arc<Mutex<LaunchState>>,
}

impl SessionLauncher {
    /// 启动后台会话监视任务。需在 tokio runtime 内调用。
    pub fn start() -> Self {
        // 同步探测一次作为初值：tokio::spawn 出的监视任务要等运行时调度才会执行第一次 tick，
        // 初值留空会让启动后紧接着到来的第一个请求（录制/扫描）误报“无活动会话”。
        let initial = active_capture_session();
        info!("session launcher started, active capture session: {:?}", initial);
        let state = Arc::new(Mutex::new(LaunchState {
            session_id: initial,
            generation: 0,
        }));
        let mon_state = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(POLL_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                let new_sid = active_capture_session();
                let mut st = match mon_state.lock() {
                    Ok(g) => g,
                    Err(_) => break,
                };
                if new_sid != st.session_id {
                    info!(
                        "active capture session changed: {:?} -> {:?}",
                        st.session_id, new_sid
                    );
                    st.generation += 1;
                    st.session_id = new_sid;
                }
            }
        });
        Self { state }
    }

    /// 当前监视到的可用于采集的会话 id（调试用）。
    #[allow(dead_code)]
    pub fn current_session(&self) -> Option<u32> {
        self.state.lock().ok().and_then(|st| st.session_id)
    }

    /// 以当前活动会话的用户身份拉起 cli-capture，返回进程 PID。
    ///
    /// 若在拉起过程中发生会话切换，会回收刚创建的进程并按新会话重试。
    pub async fn spawn_capture(&self, cli_path: &str, args: &[String]) -> Result<u32> {
        let cmd_line = quote_command_line(cli_path, args);
        for attempt in 1..=SPAWN_MAX_ATTEMPTS {
            let (sid, generation) = {
                let st = self
                    .state
                    .lock()
                    .map_err(|e| anyhow!("launch state lock poisoned: {}", e))?;
                (st.session_id, st.generation)
            };
            let Some(sid) = sid else {
                return Err(anyhow!("没有可用的活动用户会话（尚未登录或会话不可用）"));
            };

            let pid = unsafe { launch_in_session(sid, &cmd_line) }
                .map_err(|e| anyhow!("在会话 {} 中拉起进程失败: {}", sid, e))?;

            // 等待进程真正跑起来（立即退出通常是参数/环境错误）
            tokio::time::sleep(SPAWN_ALIVE_CHECK_DELAY).await;
            if !is_process_alive(pid) {
                warn!(
                    "cli-capture (pid={}) 启动后立即退出，尝试 {}/{}",
                    pid, attempt, SPAWN_MAX_ATTEMPTS
                );
                continue;
            }

            // 校验拉起期间会话未发生切换
            let (cur_sid, cur_gen) = {
                let st = self
                    .state
                    .lock()
                    .map_err(|e| anyhow!("launch state lock poisoned: {}", e))?;
                (st.session_id, st.generation)
            };
            if cur_sid == Some(sid) && cur_gen == generation {
                info!("cli-capture 已在会话 {} 中以用户身份启动, pid={}", sid, pid);
                return Ok(pid);
            }
            warn!("拉起期间发生会话切换，回收 pid={} 后按新会话重试", pid);
            kill_process_tree(pid);
        }
        Err(anyhow!(
            "多次尝试（{} 次）仍无法在活动会话中拉起采集进程",
            SPAWN_MAX_ATTEMPTS
        ))
    }

    /// 优雅停止会话内的采集进程：投递 CTRL_BREAK 让它自己走完 `obs_output_stop`
    /// 并把 MP4 尾部（moov）写出来，必要时才退化为强制结束。
    ///
    /// 为什么不能直接 taskkill /F：硬杀 = TerminateProcess，OBS 没有机会收尾，
    /// 录出来的 mp4 没有 moov，播放器打不开。
    /// 为什么要在会话内投递：控制台是会话内对象，Session 0 的服务 attach 不到
    /// 会话 1 里采集进程的控制台，所以借 `server.exe --stop-capture <pid>` 当帮手。
    pub async fn stop_capture(&self, pid: u32, wait: Duration) -> Result<()> {
        let helper = std::env::current_exe()
            .map_err(|e| anyhow!("取 current_exe 失败: {}", e))?
            .to_string_lossy()
            .to_string();
        let args = vec!["--stop-capture".to_string(), pid.to_string()];
        let delivered = match self.run_in_session(&helper, &args, HELPER_TIMEOUT).await {
            Ok(out) => Ok(out),
            Err(e) => Err(e),
        };
        finish_stop(pid, delivered, wait).await
    }

    /// 以当前活动会话的用户身份运行命令并捕获其 stdout（阻塞等待完成）。
    ///
    /// 用于 `--scan` / `--scan-windows` 这类短生命周期探测命令，
    /// 以及停止采集时的会话内 helper（`--stop-capture`）。
    pub async fn run_in_session(
        &self,
        cli_path: &str,
        args: &[String],
        timeout: Duration,
    ) -> Result<String> {
        let (sid, generation) = {
            let st = self
                .state
                .lock()
                .map_err(|e| anyhow!("launch state lock poisoned: {}", e))?;
            (st.session_id, st.generation)
        };
        let Some(sid) = sid else {
            return Err(anyhow!("没有可用的活动用户会话（尚未登录或会话不可用）"));
        };

        let cmd_line = quote_command_line(cli_path, args);
        let started = std::time::Instant::now();
        let out = tokio::task::spawn_blocking(move || unsafe {
            run_in_session_blocking(sid, &cmd_line, timeout)
        })
        .await
        .map_err(|e| anyhow!("scan task join failed: {}", e))??;
        // 这台机器上 --scan 实测 2~91 秒不等（同一套参数连续跑都会差几十倍），
        // 耗时记下来，便于区分“拉起坏了”和“cli-capture 本身就慢”。
        info!(
            "会话 {} 内命令完成: 耗时 {:.1}s, {} 字节",
            sid,
            started.elapsed().as_secs_f64(),
            out.len()
        );

        // 校验执行期间会话未切换；切换后以新会话重试一次（循环而非递归，避免 async 递归装箱）
        let (cur_sid, cur_gen) = {
            let st = self
                .state
                .lock()
                .map_err(|e| anyhow!("launch state lock poisoned: {}", e))?;
            (st.session_id, st.generation)
        };
        if cur_sid == Some(sid) && cur_gen == generation {
            Ok(out)
        } else {
            warn!("扫描期间发生会话切换，按新会话重试");
            Box::pin(self.run_in_session(cli_path, args, timeout)).await
        }
    }
}

/// direct 模式（本进程就在用户会话里，采集进程是自己的子进程）的优雅停止：
/// 直接以本进程身份拉起同一份 exe 的 `--stop-capture` helper。
pub async fn stop_capture_local(pid: u32, wait: Duration) -> Result<()> {
    let exe = std::env::current_exe().map_err(|e| anyhow!("取 current_exe 失败: {}", e))?;
    let delivered = match tokio::process::Command::new(exe)
        .args(["--stop-capture".to_string(), pid.to_string()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
    {
        Ok(st) => Ok(format!("helper 退出码 {}", st.code().unwrap_or(-1))),
        Err(e) => Err(anyhow!("拉起停止 helper 失败: {}", e)),
    };
    finish_stop(pid, delivered, wait).await
}

/// 等采集进程自己收尾退出（写 moov + obs_shutdown）；超时才强制结束。
async fn finish_stop(pid: u32, delivered: Result<String>, wait: Duration) -> Result<()> {
    match delivered {
        Ok(info) => info!("已向 pid={} 投递 CTRL_BREAK（{}）", pid, info.trim()),
        Err(e) => warn!("向 pid={} 投递 CTRL_BREAK 失败: {}", pid, e),
    }

    let deadline = std::time::Instant::now() + wait;
    while std::time::Instant::now() < deadline {
        if process_exited(pid) {
            info!("采集进程 pid={} 已优雅退出", pid);
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    warn!(
        "采集进程 pid={} 在 {:?} 内没有退出，强制结束（该文件可能缺少 MP4 尾部）",
        pid, wait
    );
    kill_process_tree(pid);
    Ok(())
}

/// 进程内共享的会话拉起器（避免每次探测都新建后台监视任务）。
pub fn global_launcher() -> &'static SessionLauncher {
    static GLOBAL: std::sync::OnceLock<SessionLauncher> = std::sync::OnceLock::new();
    GLOBAL.get_or_init(SessionLauncher::start)
}

/// 当前可用于采集的控制台会话：
/// - 排除 Session 0（服务会话）与 0xFFFFFFFF（无活动会话）
/// - 要求会话状态为 WTSActive（已登录且连接；锁屏状态仍视为可采集，画面静止）
pub fn active_capture_session() -> Option<u32> {
    let sid = unsafe { WTSGetActiveConsoleSessionId() };
    if sid == 0 || sid == u32::MAX {
        return None;
    }
    match unsafe { session_connect_state(sid) } {
        Some(state) if state == WTSActive => Some(sid),
        _ => None,
    }
}

unsafe fn session_connect_state(session_id: u32) -> Option<WTS_CONNECTSTATE_CLASS> {
    let mut buf = PWSTR::null();
    let mut bytes: u32 = 0;
    let ok = WTSQuerySessionInformationW(
        WTS_CURRENT_SERVER_HANDLE,
        session_id,
        WTSConnectState,
        &mut buf,
        &mut bytes,
    )
    .is_ok();
    if !ok || buf.0.is_null() {
        return None;
    }
    let state = if bytes >= 4 {
        Some(WTS_CONNECTSTATE_CLASS(*(buf.0 as *const i32)))
    } else {
        None
    };
    WTSFreeMemory(buf.0 as *mut core::ffi::c_void);
    state
}

/// 获取目标会话的用户 token。
///
/// 顺序刻意与 `LaunchProcessWin(as_user=TRUE)` 一致——**优先取会话内 explorer.exe 的 token**：
/// 那就是用户自己双击运行时用的 token（UAC 下为过滤后的中等完整性）。
/// `WTSQueryUserToken` 对管理员账户返回的是**未过滤**的高完整性 token，
/// 以它拉起的 cli-capture 与用户在桌面手工启动的不是同一个执行环境。
/// 仅在会话内没有 shell 时才退到 WTS，最后才是该会话的 SYSTEM（winlogon.exe）。
unsafe fn get_session_user_token(session_id: u32, out: &mut HANDLE) -> bool {
    if token_of_process_in_session("explorer.exe", session_id, out) {
        info!("会话 {} 使用 explorer.exe 的用户 token", session_id);
        return true;
    }
    if WTSQueryUserToken(session_id, out as *mut HANDLE).is_ok() {
        warn!(
            "会话 {} 未找到 explorer.exe，改用 WTSQueryUserToken（管理员账户下为未过滤 token）",
            session_id
        );
        return true;
    }
    if token_of_process_in_session("winlogon.exe", session_id, out) {
        warn!("会话 {} 只能取到 winlogon.exe 的 SYSTEM token", session_id);
        return true;
    }
    false
}

unsafe fn token_of_process_in_session(name: &str, session_id: u32, out: &mut HANDLE) -> bool {
    let Some(pid) = find_process_in_session(name, session_id) else {
        return false;
    };
    let Ok(hproc) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
        warn!("OpenProcess({}, pid={}) 失败", name, pid);
        return false;
    };
    let ok = OpenProcessToken(hproc, TOKEN_ALL_ACCESS, out as *mut HANDLE).is_ok();
    let _ = CloseHandle(hproc);
    if !ok {
        warn!("OpenProcessToken({}, pid={}) 失败", name, pid);
    }
    ok
}

unsafe fn find_process_in_session(name: &str, session_id: u32) -> Option<u32> {
    let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut found = None;
    let mut has_next = Process32FirstW(snapshot, &mut entry).is_ok();
    while has_next {
        let exe = String::from_utf16_lossy(&entry.szExeFile);
        let exe = exe.trim_end_matches('\0');
        if exe.eq_ignore_ascii_case(name) {
            let mut sid = 0u32;
            if ProcessIdToSessionId(entry.th32ProcessID, &mut sid).is_ok() && sid == session_id {
                found = Some(entry.th32ProcessID);
                break;
            }
        }
        has_next = Process32NextW(snapshot, &mut entry).is_ok();
    }
    let _ = CloseHandle(snapshot);
    found
}

/// 在指定会话中以该用户身份创建无窗口进程，返回 PID。
///
/// 进程显式落在 winsta0\default（用户默认桌面），
/// 使用 CREATE_UNICODE_ENVIRONMENT + CreateEnvironmentBlock 构造用户环境块。
unsafe fn launch_in_session(session_id: u32, cmd_line: &str) -> Result<u32> {
    let mut token = HANDLE::default();
    if !get_session_user_token(session_id, &mut token) {
        return Err(anyhow!("无法获取会话 {} 的用户 token", session_id));
    }

    let mut env: *mut core::ffi::c_void = std::ptr::null_mut();
    let has_env = CreateEnvironmentBlock(&mut env, token, true).is_ok();
    if has_env {
        info!("已为会话 {} 构建用户环境块", session_id);
    } else {
        warn!("CreateEnvironmentBlock 失败，使用默认环境变量");
    }

    let mut cmd: Vec<u16> = OsStr::new(cmd_line)
        .encode_wide()
        .chain(Some(0))
        .collect();
    let desktop: Vec<u16> = OsStr::new("winsta0\\default")
        .encode_wide()
        .chain(Some(0))
        .collect();

    let si = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        lpDesktop: PWSTR(desktop.as_ptr() as *mut u16),
        ..Default::default()
    };
    let mut pi = PROCESS_INFORMATION::default();

    // CREATE_NO_WINDOW：给控制台子系统程序一个隐藏控制台。
    // cli-capture 是控制台程序（靠 stdout/stderr 输出），不能用 DETACHED_PROCESS
    // （那会让它完全没有控制台，std 句柄全是无效值）。
    // CREATE_NEW_PROCESS_GROUP：让它自成进程组，停止时才能用 CTRL_BREAK 精确投递给它。
    let mut flags = CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP;
    if has_env {
        flags |= CREATE_UNICODE_ENVIRONMENT;
    }

    let create_result = CreateProcessAsUserW(
        token,
        PWSTR::null(),
        PWSTR(cmd.as_mut_ptr()),
        None,
        None,
        false,
        flags,
        if has_env { Some(env as *const core::ffi::c_void) } else { None },
        PWSTR::null(),
        &si,
        &mut pi,
    );

    if has_env {
        let _ = DestroyEnvironmentBlock(env);
    }
    let _ = CloseHandle(token);

    create_result.map_err(|e| anyhow!("CreateProcessAsUserW 失败: {}", e))?;
    let _ = CloseHandle(pi.hThread);
    let pid = pi.dwProcessId;
    let _ = CloseHandle(pi.hProcess);
    Ok(pid)
}

/// 在用户会话中同步运行命令并通过匿名管道捕获其 stdout。
///
/// 管道句柄是内核对象，跨会话继承没有问题；不经过 cmd 中转，规避其引号解析坑。
unsafe fn run_in_session_blocking(session_id: u32, cmd_line: &str, timeout: Duration) -> Result<String> {
    let mut token = HANDLE::default();
    if !get_session_user_token(session_id, &mut token) {
        return Err(anyhow!("无法获取会话 {} 的用户 token", session_id));
    }

    let mut env: *mut core::ffi::c_void = std::ptr::null_mut();
    let has_env = CreateEnvironmentBlock(&mut env, token, true).is_ok();
    if !has_env {
        warn!("CreateEnvironmentBlock 失败，使用默认环境变量");
    }

    // 匿名管道：两端都先设为可继承，随后把读端改回不可继承，
    // 确保子进程不会持有读端（否则父进程读不到 EOF）。
    let sa = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: true.into(),
    };
    let mut pipe_read = HANDLE::default();
    let mut pipe_write = HANDLE::default();
    CreatePipe(&mut pipe_read, &mut pipe_write, Some(&sa), 0)
        .map_err(|e| anyhow!("创建 stdout 管道失败: {}", e))?;
    let _ = SetHandleInformation(pipe_read, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0));

    // stderr 丢弃到 NUL，避免污染 stdout 的 JSON。
    // 注意：STARTF_USESTDHANDLES 要求句柄**可继承**，否则子进程里的 0/1/2 号句柄是无效值，
    // 而该值又会被子进程后续 CreateFile 之类重新占用 —— 于是 stderr 就悄悄指到了别的对象上。
    let nul_name: Vec<u16> = OsStr::new("NUL").encode_wide().chain(Some(0)).collect();
    let nul = windows::Win32::Storage::FileSystem::CreateFileW(
        windows::core::PCWSTR(nul_name.as_ptr()),
        windows::Win32::Storage::FileSystem::FILE_GENERIC_WRITE.0,
        windows::Win32::Storage::FileSystem::FILE_SHARE_WRITE
            | windows::Win32::Storage::FileSystem::FILE_SHARE_READ,
        Some(&sa),
        windows::Win32::Storage::FileSystem::OPEN_EXISTING,
        windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL,
        HANDLE::default(),
    )
    .unwrap_or(HANDLE::default());

    let mut cmd: Vec<u16> = OsStr::new(cmd_line)
        .encode_wide()
        .chain(Some(0))
        .collect();
    let desktop: Vec<u16> = OsStr::new("winsta0\\default")
        .encode_wide()
        .chain(Some(0))
        .collect();

    let si = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        lpDesktop: PWSTR(desktop.as_ptr() as *mut u16),
        dwFlags: STARTF_USESTDHANDLES,
        hStdInput: nul,
        hStdOutput: pipe_write,
        hStdError: nul,
        ..Default::default()
    };
    let mut pi = PROCESS_INFORMATION::default();

    let mut flags = CREATE_NO_WINDOW;
    if has_env {
        flags |= CREATE_UNICODE_ENVIRONMENT;
    }

    let create_result = CreateProcessAsUserW(
        token,
        PWSTR::null(),
        PWSTR(cmd.as_mut_ptr()),
        None,
        None,
        true, // bInheritHandles：传递可继承的管道写端
        flags,
        if has_env { Some(env as *const core::ffi::c_void) } else { None },
        PWSTR::null(),
        &si,
        &mut pi,
    );

    if has_env {
        let _ = DestroyEnvironmentBlock(env);
    }
    let _ = CloseHandle(token);
    let _ = CloseHandle(nul);

    if let Err(e) = create_result {
        let _ = CloseHandle(pipe_read);
        let _ = CloseHandle(pipe_write);
        return Err(anyhow!("CreateProcessAsUserW 失败: {}", e));
    }
    let _ = CloseHandle(pi.hThread);
    // 父进程立即关闭自己的写端副本，子进程退出后读端才能收到 EOF
    let _ = CloseHandle(pipe_write);

    // 后台线程持续读管道，防止子进程输出超过管道缓冲区（64KB）而卡死
    // HANDLE(*mut c_void) 不满足 Send，以 usize 形式移交所有权
    // 同时把已读到的字节共享出来：超时时才能回报“子进程到底输出到哪一步”
    let pipe_read_raw = pipe_read.0 as usize;
    let collected = Arc::new(Mutex::new(Vec::<u8>::new()));
    let sink = collected.clone();
    let reader = std::thread::spawn(move || {
        use std::io::Read;
        use std::os::windows::io::FromRawHandle;
        let mut file = std::fs::File::from_raw_handle(pipe_read_raw as _);
        let mut chunk = [0u8; 4096];
        loop {
            match file.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if let Ok(mut buf) = sink.lock() {
                        buf.extend_from_slice(&chunk[..n]);
                    }
                }
            }
        }
    });

    let timeout_ms = timeout.as_millis().min(u32::MAX as u128) as u32;
    let wait = WaitForSingleObject(pi.hProcess, timeout_ms);
    if wait == WAIT_TIMEOUT {
        let pid = pi.dwProcessId;
        let _ = CloseHandle(pi.hProcess);
        kill_process_tree(pid);
        let _ = reader.join();
        let captured = collected.lock().map(|b| b.len()).unwrap_or(0);
        return Err(anyhow!(
            "扫描超时（{} 秒），已终止进程；期间 stdout 收到 {} 字节{}",
            timeout.as_secs(),
            captured,
            if captured == 0 {
                "（子进程未在退出前刷新输出）"
            } else {
                ""
            }
        ));
    }

    let mut exit_code = 0u32;
    let _ = GetExitCodeProcess(pi.hProcess, &mut exit_code);
    let _ = CloseHandle(pi.hProcess);

    reader
        .join()
        .map_err(|_| anyhow!("stdout 读取线程 panic"))?;
    let bytes = collected.lock().map_err(|_| anyhow!("stdout 缓冲锁中毒"))?.clone();

    if exit_code != 0 {
        return Err(anyhow!(
            "扫描进程退出码 {}，输出: {}",
            exit_code,
            String::from_utf8_lossy(&bytes)
        ));
    }
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

fn is_process_alive(pid: u32) -> bool {
    unsafe {
        let Ok(hproc) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            // 无法打开句柄时保守视为存活，避免误报
            return true;
        };
        let mut code = 0u32;
        let alive = GetExitCodeProcess(hproc, &mut code).is_ok() && code == STILL_ACTIVE;
        let _ = CloseHandle(hproc);
        alive
    }
}

/// 进程是否已经退出（与 `is_process_alive` 相反：打不开句柄即视为已消失）。
fn process_exited(pid: u32) -> bool {
    unsafe {
        let Ok(hproc) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return true;
        };
        let mut code = 0u32;
        let exited = GetExitCodeProcess(hproc, &mut code).is_err() || code != STILL_ACTIVE;
        let _ = CloseHandle(hproc);
        exited
    }
}

/// `server.exe --stop-capture <pid>`：在**用户会话内**给采集进程投递 CTRL_BREAK。
///
/// 由 `SessionLauncher::stop_capture` 拉起，不要在服务进程里直接调用
/// （Session 0 的进程 attach 不到会话 1 的控制台）。
/// cli-capture 收到后走 `signal_handler` → 停止 OBS 输出 → 写完 MP4 尾部再退出。
pub fn send_ctrl_break(pid: u32) -> Result<()> {
    use windows::Win32::System::Console::{
        AttachConsole, FreeConsole, GenerateConsoleCtrlEvent, SetConsoleCtrlHandler,
        CTRL_BREAK_EVENT,
    };

    unsafe extern "system" fn ignore_ctrl_break(_ctrl_type: u32) -> windows::Win32::Foundation::BOOL {
        // 事件会广播给同一控制台上的所有进程，这里把自己摘出去
        true.into()
    }

    unsafe {
        let _ = FreeConsole();
        AttachConsole(pid).map_err(|e| anyhow!("AttachConsole({}) 失败: {}", pid, e))?;
        // NULL handler：忽略 CTRL_C；自定义 handler（返回 TRUE）：忽略 CTRL_BREAK
        let _ = SetConsoleCtrlHandler(None, true);
        let _ = SetConsoleCtrlHandler(Some(ignore_ctrl_break), true);
        GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid)
            .map_err(|e| anyhow!("GenerateConsoleCtrlEvent(pid={}) 失败: {}", pid, e))?;
        // 事件是异步投递的，等它落地再退出，避免 helper 先消失
        std::thread::sleep(Duration::from_millis(1500));
    }
    println!("ctrl-break -> pid {}", pid);
    Ok(())
}

/// 结束进程及其子进程树。
pub fn kill_process_tree(pid: u32) {
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .output();
}

/// 构造带引号的命令行（CreateProcessAsUser 需要完整命令行字符串）。
fn quote_command_line(cli_path: &str, args: &[String]) -> String {
    let mut parts = vec![quote_arg(cli_path)];
    parts.extend(args.iter().map(|a| quote_arg(a)));
    parts.join(" ")
}

fn quote_arg(arg: &str) -> String {
    if arg.is_empty() {
        "\"\"".to_string()
    } else if arg.contains(' ') || arg.contains('"') {
        format!("\"{}\"", arg.replace('"', "\\\""))
    } else {
        arg.to_string()
    }
}

#[cfg(test)]
mod tests {
    /// 会话初值必须同步就绪：后台监视任务的第一次 tick 依赖运行时调度，
    /// 若初值留空，启动后紧接着的第一次录制/扫描会误报“无活动用户会话”。
    #[test]
    fn session_known_before_first_monitor_tick() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        // current_thread runtime 下 spawn 出的监视任务在本作用域内不会被调度，
        // 因此这里读到的一定是 start() 的同步初值。
        let launcher = rt.block_on(async { super::SessionLauncher::start() });
        assert_eq!(launcher.current_session(), super::active_capture_session());
    }

    #[tokio::test]
    async fn run_in_session_echo_roundtrip() {
        let launcher = super::global_launcher();
        // 等待后台监视任务识别到活动会话（最多 3 秒）
        let mut session = None;
        for _ in 0..30 {
            session = launcher.current_session();
            if session.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        let Some(sid) = session else {
            eprintln!("no active capture session on this machine; skipping");
            return;
        };
        eprintln!("active session: {}", sid);
        let out = launcher
            .run_in_session("cmd.exe", &["/c".to_string(), "echo hello-scan".to_string()], std::time::Duration::from_secs(30))
            .await
            .expect("run_in_session failed");
        assert!(out.contains("hello-scan"), "unexpected output: {:?}", out);
    }
}
