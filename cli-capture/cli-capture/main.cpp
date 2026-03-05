#include <obs.h>
#include <callback/signal.h>
#include <callback/calldata.h>
#include <util/base.h>
#include <util/platform.h>
#include <util/dstr.h>
#include <util/config-file.h>
#include <windows.h>
#include <mmsystem.h>
#include <psapi.h>
#pragma comment(lib, "psapi.lib")
#pragma comment(lib, "winmm.lib")
#include <iostream>
#include <vector>
#include <string>
#include <thread>
#include <atomic>
#include <mutex>
#include <condition_variable>
#include <csignal>
#include <iomanip>
#include <algorithm>
#include <cctype>
#include <queue>

// UI task queue for libobs main thread callbacks
struct UiTask {
    obs_task_t task;
    void *param;
    bool wait;
    HANDLE event; // signaled when wait==true and task is done
};

static std::mutex ui_task_mutex;
static std::queue<UiTask> ui_task_queue;

static void cli_ui_task_handler(obs_task_t task, void *param, bool wait)
{
    if (wait) {
        HANDLE ev = CreateEventA(NULL, TRUE, FALSE, NULL);
        {
            std::lock_guard<std::mutex> lock(ui_task_mutex);
            ui_task_queue.push({task, param, true, ev});
        }
        WaitForSingleObject(ev, INFINITE);
        CloseHandle(ev);
    } else {
        std::lock_guard<std::mutex> lock(ui_task_mutex);
        ui_task_queue.push({task, param, false, NULL});
    }
}

static void process_ui_tasks()
{
    // Drain tasks without holding the lock during execution,
    // to avoid deadlock if a task re-enters cli_ui_task_handler.
    for (;;) {
        UiTask t;
        {
            std::lock_guard<std::mutex> lock(ui_task_mutex);
            if (ui_task_queue.empty())
                break;
            t = ui_task_queue.front();
            ui_task_queue.pop();
        }
        t.task(t.param);
        if (t.wait && t.event) {
            SetEvent(t.event);
        }
    }
}

// Global flag for interrupt
enum StopReason {
    StopReasonNone = 0,
    StopReasonSigInt = 1,
    StopReasonSigTerm = 2,
    StopReasonSigBreak = 3,
    StopReasonConsoleClose = 4,
    StopReasonOutputStopped = 5
};

std::atomic<bool> keep_running(true);
std::atomic<int> stop_reason(StopReasonNone);
std::atomic<long long> output_stop_code(0);
std::atomic<bool> output_stop_received(false);
std::mutex stop_mutex;
std::condition_variable stop_cv;
std::string output_stop_error;
bool video_reset_done = false;

constexpr int kMonitorMethodAuto = 0;
constexpr int kMonitorMethodDxgi = 1;
constexpr int kMonitorMethodWgc = 2;

struct TimePeriodGuard {
    explicit TimePeriodGuard(UINT period) : period(period) {
        timeBeginPeriod(period);
    }
    ~TimePeriodGuard() {
        timeEndPeriod(period);
    }
    UINT period;
};

const char* monitor_method_name(int method) {
    switch (method) {
    case kMonitorMethodDxgi:
        return "DXGI";
    case kMonitorMethodWgc:
        return "WGC";
    case kMonitorMethodAuto:
        return "AUTO";
    default:
        return "UNKNOWN";
    }
}

std::string infer_copy_path(int method, const std::string& encoder_id) {
    const bool uses_tex = encoder_id.find("_tex") != std::string::npos;
    const char* enc_path = uses_tex ? "gpu-texture" : "cpu-copy";
    if (method == kMonitorMethodDxgi) {
        return std::string("dxgi-dup -> gpu-texture -> ") + enc_path;
    }
    if (method == kMonitorMethodWgc) {
        return std::string("wgc -> gpu-texture -> ") + enc_path;
    }
    if (method == kMonitorMethodAuto) {
        return std::string("auto(dxgi|wgc) -> gpu-texture -> ") + enc_path;
    }
    return "unknown";
}

bool reset_video_once(obs_video_info* ovi, bool quiet) {
    if (video_reset_done) {
        return true;
    }
    if (!quiet) {
        std::cerr << "Resetting video..." << std::endl;
    }
    int ret = obs_reset_video(ovi);
    if (ret != OBS_VIDEO_SUCCESS) {
        if (!quiet) {
            std::cerr << "Failed to reset video, error code: " << ret << std::endl;
        }
        return false;
    }
    if (!quiet) {
        std::cerr << "Video reset successful." << std::endl;
    }
    video_reset_done = true;
    return true;
}

bool reset_video(obs_video_info* ovi, bool quiet) {
    if (!quiet) {
        std::cerr << "Resetting video..." << std::endl;
    }
    int ret = obs_reset_video(ovi);
    if (ret != OBS_VIDEO_SUCCESS) {
        if (!quiet) {
            std::cerr << "Failed to reset video, error code: " << ret << std::endl;
        }
        return false;
    }
    if (!quiet) {
        std::cerr << "Video reset successful." << std::endl;
    }
    return true;
}

void clear_output_sources() {
    obs_set_output_source(0, NULL);
    obs_set_output_source(1, NULL);
    obs_set_output_source(2, NULL);
}

void set_stop_reason(int reason) {
    int expected = StopReasonNone;
    stop_reason.compare_exchange_strong(expected, reason);
}

void signal_handler(int signal) {
    if (signal == SIGINT) {
        set_stop_reason(StopReasonSigInt);
    } else if (signal == SIGTERM) {
        set_stop_reason(StopReasonSigTerm);
    }
#ifdef SIGBREAK
    else if (signal == SIGBREAK) {
        set_stop_reason(StopReasonSigBreak);
    }
#endif
    keep_running = false;
}

void silent_log_handler(int level, const char *msg, va_list args, void *param) {
    UNUSED_PARAMETER(level);
    UNUSED_PARAMETER(msg);
    UNUSED_PARAMETER(args);
    UNUSED_PARAMETER(param);
}

BOOL WINAPI console_handler(DWORD ctrl_type) {
    if (ctrl_type == CTRL_CLOSE_EVENT || ctrl_type == CTRL_SHUTDOWN_EVENT || ctrl_type == CTRL_LOGOFF_EVENT) {
        set_stop_reason(StopReasonConsoleClose);
        keep_running = false;
        return TRUE;
    }
    return FALSE;
}

void output_stop_cb(void *param, calldata_t *data) {
    UNUSED_PARAMETER(param);
    const char *err = calldata_string(data, "last_error");
    long long code = calldata_int(data, "code");
    if (err) {
        std::lock_guard<std::mutex> lock(stop_mutex);
        output_stop_error = err;
    }
    output_stop_code.store(code);
    set_stop_reason(StopReasonOutputStopped);
    output_stop_received.store(true);
    keep_running = false;
    stop_cv.notify_all();
}

struct Args {
    bool scan = false;
    bool scan_windows = false;
    bool test = false;
    int monitor_idx = 0;
    std::string monitor_id; // device_id string, takes priority over monitor_idx
    int method = kMonitorMethodAuto;
    std::string audio_desktop_id;
    std::string audio_mic_id;
    std::string output_file;
    std::string rtmp_url;
    std::string rtmp_key;
    std::string window_id;
    std::string encoder = "obs_x264";
    int bitrate = 2500;
    int width = 0; // 0 means auto-detect
    int height = 0;
    int fps = 30;
    int rotation = 0;
};

struct WindowInfo {
    std::string title;
    std::string exe_name;
    std::string class_name;
    std::string game_capture_id;
};

struct MonitorInfo {
    int index = 0;
    RECT rect = {};
    std::string device;      // szDevice e.g. "\\\\.\\DISPLAY1"
    std::string device_id;   // DeviceID from EnumDisplayDevices (interface path)
    uint32_t width = 0;
    uint32_t height = 0;
    int rotation = 0;
    bool primary = false;
};

struct MonitorEnumContext {
    std::vector<MonitorInfo> monitors;
    int index = 0;
};

BOOL CALLBACK EnumWindowsProc(HWND hwnd, LPARAM lParam) {
    auto* list = reinterpret_cast<std::vector<WindowInfo>*>(lParam);

    if (!IsWindowVisible(hwnd))
        return TRUE;
    if (IsIconic(hwnd))
        return TRUE;

    char title[512] = {};
    if (GetWindowTextA(hwnd, title, sizeof(title)) == 0)
        return TRUE;
    if (strlen(title) == 0)
        return TRUE;

    char class_buf[256] = {};
    GetClassNameA(hwnd, class_buf, sizeof(class_buf));
    if (strcmp(class_buf, "Shell_TrayWnd") == 0)
        return TRUE;
    if (strcmp(class_buf, "Progman") == 0)
        return TRUE;

    DWORD pid = 0;
    GetWindowThreadProcessId(hwnd, &pid);
    if (pid == 0)
        return TRUE;

    HANDLE hProc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
    if (!hProc)
        return TRUE;

    char exe_path[MAX_PATH] = {};
    DWORD size = MAX_PATH;
    if (!QueryFullProcessImageNameA(hProc, 0, exe_path, &size)) {
        CloseHandle(hProc);
        return TRUE;
    }
    CloseHandle(hProc);

    std::string exe_full(exe_path);
    std::string exe_name = exe_full;
    size_t slash = exe_full.find_last_of("\\/");
    if (slash != std::string::npos) {
        exe_name = exe_full.substr(slash + 1);
    }

    WindowInfo info;
    info.title = title;
    info.exe_name = exe_name;
    info.class_name = class_buf;
    info.game_capture_id = info.title + ":" + info.exe_name + ":" + info.class_name;
    list->push_back(info);
    return TRUE;
}

std::vector<WindowInfo> get_capturable_windows() {
    std::vector<WindowInfo> list;
    EnumWindows(EnumWindowsProc, (LPARAM)&list);
    return list;
}

// Monitor enumeration callback
BOOL CALLBACK MonitorEnumProc(HMONITOR hMonitor, HDC hdcMonitor, LPRECT lprcMonitor, LPARAM dwData) {
    auto* ctx = reinterpret_cast<MonitorEnumContext*>(dwData);
    MONITORINFOEXA mi;
    memset(&mi, 0, sizeof(mi));
    mi.cbSize = sizeof(mi);
    GetMonitorInfoA(hMonitor, &mi);

    MonitorInfo info;
    info.index = ctx->index++;
    info.rect = *lprcMonitor;
    info.device = mi.szDevice;
    info.primary = (mi.dwFlags & MONITORINFOF_PRIMARY) != 0;

    // Get device interface ID (needed by duplicator-monitor-capture)
    DISPLAY_DEVICEA dd;
    memset(&dd, 0, sizeof(dd));
    dd.cb = sizeof(dd);
    if (EnumDisplayDevicesA(mi.szDevice, 0, &dd, EDD_GET_DEVICE_INTERFACE_NAME)) {
        info.device_id = dd.DeviceID;
    } else {
        // Fallback to szDevice (enum_monitor_fallback in duplicator will match this)
        info.device_id = mi.szDevice;
    }

    DEVMODEA dm;
    memset(&dm, 0, sizeof(dm));
    dm.dmSize = sizeof(dm);
    if (EnumDisplaySettingsExA(mi.szDevice, ENUM_CURRENT_SETTINGS, &dm, 0)) {
        info.width = dm.dmPelsWidth;
        info.height = dm.dmPelsHeight;
        switch (dm.dmDisplayOrientation) {
        case DMDO_90:
            info.rotation = 90;
            break;
        case DMDO_180:
            info.rotation = 180;
            break;
        case DMDO_270:
            info.rotation = 270;
            break;
        default:
            info.rotation = 0;
            break;
        }
    } else {
        info.width = (uint32_t)(lprcMonitor->right - lprcMonitor->left);
        info.height = (uint32_t)(lprcMonitor->bottom - lprcMonitor->top);
    }

    ctx->monitors.push_back(info);
    return TRUE;
}

std::vector<MonitorInfo> get_monitors() {
    MonitorEnumContext ctx;
    EnumDisplayMonitors(NULL, NULL, MonitorEnumProc, (LPARAM)&ctx);
    return ctx.monitors;
}

// Helper to print JSON-like output
void print_json_start() {
    std::cout << "{\n";
}

void print_json_end() {
    std::cout << "\n}\n";
}

void print_json_array(const std::string& key, const std::vector<std::pair<std::string, std::string>>& items, bool last = false) {
    std::cout << "  \"" << key << "\": [\n";
    for (size_t i = 0; i < items.size(); ++i) {
        std::cout << "    { \"id\": \"" << items[i].first << "\", \"name\": \"" << items[i].second << "\" }";
        if (i < items.size() - 1) std::cout << ",";
        std::cout << "\n";
    }
    std::cout << "  ]" << (last ? "" : ",") << "\n";
}

// Function to list screens
void list_screens() {
    auto monitors = get_monitors();
    auto escape_json = [](const std::string& s) {
        std::string out;
        for (char c : s) {
            if (c == '"')
                out += "\\\"";
            else if (c == '\\')
                out += "\\\\";
            else
                out += c;
        }
        return out;
    };
    std::cout << "  \"screens\": [\n";
    for (size_t i = 0; i < monitors.size(); ++i) {
        const auto& m = monitors[i];
        std::string name = "Display " + std::to_string(m.index + 1) + ": " +
                           std::to_string(m.width) + "x" + std::to_string(m.height) +
                           " @ " + std::to_string(m.rect.left) + "," + std::to_string(m.rect.top);
        if (m.primary) {
            name += " (Primary Monitor)";
        }
        if (m.rotation != 0) {
            name += " rot=" + std::to_string(m.rotation);
        }
        std::cout << "    { \"id\": \"" << escape_json(m.device_id)
                  << "\", \"index\": " << m.index
                  << ", \"name\": \"" << escape_json(name) << "\" }";
        if (i < monitors.size() - 1) std::cout << ",";
        std::cout << "\n";
    }
    std::cout << "  ],\n";
}

void list_windows() {
    auto windows = get_capturable_windows();
    auto escape_json = [](const std::string& s) {
        std::string out;
        for (char c : s) {
            if (c == '"')
                out += "\\\"";
            else if (c == '\\')
                out += "\\\\";
            else
                out += c;
        }
        return out;
    };
    std::cout << "  \"windows\": [\n";
    for (size_t i = 0; i < windows.size(); ++i) {
        std::cout << "    { "
                  << "\"title\": \"" << escape_json(windows[i].title) << "\", "
                  << "\"exe\": \"" << escape_json(windows[i].exe_name) << "\", "
                  << "\"id\": \"" << escape_json(windows[i].game_capture_id) << "\""
                  << " }";
        if (i < windows.size() - 1)
            std::cout << ",";
        std::cout << "\n";
    }
    std::cout << "  ]\n";
}

// Function to list audio devices
void list_audio_devices(const char* source_id, const std::string& json_key) {
    obs_source_t* source = obs_source_create(source_id, "temp_audio", NULL, NULL);
    if (!source) {
        std::cout << "  \"" << json_key << "\": [],\n";
        return;
    }

    obs_properties_t* props = obs_source_properties(source);
    obs_property_t* device_prop = obs_properties_get(props, "device_id");

    size_t count = obs_property_list_item_count(device_prop);
    std::vector<std::pair<std::string, std::string>> devices;
    for (size_t i = 0; i < count; ++i) {
        const char* name = obs_property_list_item_name(device_prop, i);
        const char* val = obs_property_list_item_string(device_prop, i);
        if (val && name) {
            devices.push_back({val, name});
        }
    }

    print_json_array(json_key, devices);

    obs_properties_destroy(props);
    obs_source_release(source);
}

// Function to list encoders
void list_encoders() {
    std::vector<std::pair<std::string, std::string>> encoders;
    const char* id = NULL;
    size_t i = 0;
    while (obs_enum_encoder_types(i++, &id)) {
        // Filter for common hardware encoders and x264
        if (strstr(id, "nvenc") || 
            strstr(id, "amf") || 
            strstr(id, "qsv") ||
            strcmp(id, "obs_x264") == 0) {
            const char* name = obs_encoder_get_display_name(id);
            encoders.push_back({id, name ? name : id});
        }
    }
    print_json_array("encoders", encoders, true);
}

void parse_args(int argc, char* argv[], Args& args) {
    for (int i = 1; i < argc; ++i) {
        std::string arg = argv[i];
        if (arg == "--scan") {
            args.scan = true;
        } else if (arg == "--test") {
            args.test = true;
        } else if (arg == "--scan-windows") {
            args.scan_windows = true;
        } else if (arg == "--monitor" && i + 1 < argc) {
            std::string val = argv[++i];
            // If it's a pure integer, treat as index; otherwise treat as device_id
            bool is_int = !val.empty() && std::all_of(val.begin(), val.end(), ::isdigit);
            if (is_int) {
                args.monitor_idx = std::stoi(val);
            } else {
                args.monitor_id = val;
            }
        } else if (arg == "--desktop-audio" && i + 1 < argc) {
            args.audio_desktop_id = argv[++i];
        } else if (arg == "--mic-audio" && i + 1 < argc) {
            args.audio_mic_id = argv[++i];
        } else if (arg == "--output" && i + 1 < argc) {
            args.output_file = argv[++i];
        } else if (arg == "--rtmp" && i + 1 < argc) {
            args.rtmp_url = argv[++i];
        } else if (arg == "--key" && i + 1 < argc) {
            args.rtmp_key = argv[++i];
        } else if (arg == "--window" && i + 1 < argc) {
            args.window_id = argv[++i];
        } else if (arg == "--encoder" && i + 1 < argc) {
            args.encoder = argv[++i];
        } else if (arg == "--bitrate" && i + 1 < argc) {
            args.bitrate = std::stoi(argv[++i]);
        } else if (arg == "--width" && i + 1 < argc) {
            args.width = std::stoi(argv[++i]);
        } else if (arg == "--height" && i + 1 < argc) {
            args.height = std::stoi(argv[++i]);
        } else if (arg == "--fps" && i + 1 < argc) {
            args.fps = std::stoi(argv[++i]);
        } else if (arg == "--method" && i + 1 < argc) {
            std::string value = argv[++i];
            std::transform(value.begin(), value.end(), value.begin(), [](unsigned char c) {
                return (char)std::tolower(c);
            });
            if (value == "dxgi") {
                args.method = kMonitorMethodDxgi;
            } else if (value == "wgc") {
                args.method = kMonitorMethodWgc;
            } else if (value == "auto") {
                args.method = kMonitorMethodAuto;
            } else {
                try {
                    int method = std::stoi(value);
                    if (method == kMonitorMethodDxgi || method == kMonitorMethodWgc || method == kMonitorMethodAuto) {
                        args.method = method;
                    }
                } catch (...) {
                }
            }
        }
    }
}

int main(int argc, char *argv[]) {
    TimePeriodGuard time_period_guard(1);
    // Set console output to UTF-8 to fix mojibake
    SetConsoleOutputCP(CP_UTF8);
    SetConsoleCP(CP_UTF8);
    SetConsoleCtrlHandler(console_handler, TRUE);

    // Enable SE_INC_BASE_PRIORITY_NAME privilege for GPU priority
    {
        const DWORD flags = TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY;
        TOKEN_PRIVILEGES tp;
        HANDLE token;
        LUID val;
        if (OpenProcessToken(GetCurrentProcess(), flags, &token)) {
            if (LookupPrivilegeValue(NULL, SE_INC_BASE_PRIORITY_NAME, &val)) {
                tp.PrivilegeCount = 1;
                tp.Privileges[0].Luid = val;
                tp.Privileges[0].Attributes = SE_PRIVILEGE_ENABLED;
                AdjustTokenPrivileges(token, false, &tp, sizeof(tp), NULL, NULL);
            }
            CloseHandle(token);
        }
    }

    // Initialize Realtime Work Queue for MMCSS thread scheduling
    HMODULE hRtwq = LoadLibraryW(L"RTWorkQ.dll");
    if (hRtwq) {
        typedef HRESULT(STDAPICALLTYPE * PFN_RtwqStartup)();
        auto func = (PFN_RtwqStartup)GetProcAddress(hRtwq, "RtwqStartup");
        if (func) func();
    }

    HMODULE user32 = GetModuleHandleA("user32.dll");
    if (user32) {
        using SetProcessDpiAwarenessContext_t = BOOL(WINAPI *)(HANDLE);
        auto set_dpi_ctx = (SetProcessDpiAwarenessContext_t)GetProcAddress(user32, "SetProcessDpiAwarenessContext");
        if (set_dpi_ctx) {
            set_dpi_ctx((HANDLE)-4);
        } else {
            SetProcessDPIAware();
        }
    }

    Args args;
    parse_args(argc, argv, args);

    if (args.scan_windows) {
        base_set_log_handler(silent_log_handler, nullptr);
        print_json_start();
        list_windows();
        print_json_end();
        return 0;
    }

    const bool quiet_scan = args.scan;
    if (quiet_scan) {
        base_set_log_handler(silent_log_handler, nullptr);
    }

    if (!obs_startup("en-US", NULL, NULL)) {
        if (!quiet_scan) {
            std::cerr << "Failed to startup OBS" << std::endl;
        }
        return -1;
    }

    obs_set_ui_task_handler(cli_ui_task_handler);
    
    // Set current directory to executable directory to find data files
    char exe_path[MAX_PATH];
    GetModuleFileNameA(NULL, exe_path, MAX_PATH);
    std::string path(exe_path);
    size_t pos = path.find_last_of("\\/");
    std::string dir = path.substr(0, pos);
    SetCurrentDirectoryA(dir.c_str());
    
    // Manually add data paths relative to executable location in rundir structure
    // rundir/RelWithDebInfo/bin/64bit/cli-capture.exe
    // rundir/RelWithDebInfo/data/libobs/
    obs_add_data_path("../../data/libobs");
    obs_add_data_path("../../data/obs-plugins/%module%");

    if (!quiet_scan) {
        obs_log_loaded_modules();
    }

    // ---- Scan mode: load modules first (needs video for some properties) ----
    if (args.scan) {
        std::cerr << "Loading modules..." << std::endl;
        obs_load_all_modules();
        obs_post_load_modules();

        obs_video_info ovi;
        memset(&ovi, 0, sizeof(obs_video_info));
        ovi.adapter = 0;
        ovi.base_width = 1920;
        ovi.base_height = 1080;
        ovi.output_width = 1920;
        ovi.output_height = 1080;
        ovi.fps_num = 30;
        ovi.fps_den = 1;
        ovi.graphics_module = "libobs-d3d11";
        ovi.output_format = VIDEO_FORMAT_NV12;
        ovi.colorspace = VIDEO_CS_709;
        ovi.range = VIDEO_RANGE_PARTIAL;
        ovi.gpu_conversion = true;
        ovi.scale_type = OBS_SCALE_BICUBIC;

        if (!reset_video_once(&ovi, quiet_scan)) {
            obs_shutdown();
            return -1;
        }
        print_json_start();
        list_screens();
        list_audio_devices("wasapi_output_capture", "desktop_audio");
        list_audio_devices("wasapi_input_capture", "microphone");
        list_encoders();
        print_json_end();
        obs_shutdown();
        return 0;
    }

    // ===== Capture Mode =====
    // OBS Studio init order: obs_startup -> audio -> video -> load_modules
    const int user_set_width = args.width;
    const int user_set_height = args.height;

    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);
#ifdef SIGBREAK
    signal(SIGBREAK, signal_handler);
#endif

    // 1. Reset Audio first (matches OBS Studio order)
    {
        obs_audio_info oai;
        memset(&oai, 0, sizeof(obs_audio_info));
        oai.samples_per_sec = 48000;
        oai.speakers = SPEAKERS_STEREO;
        if (!obs_reset_audio(&oai)) {
            std::cerr << "Failed to reset audio" << std::endl;
            obs_shutdown();
            return -1;
        }
    }

    // 2. Reset Video (before loading modules - matches OBS Studio order)
    // Find the target monitor (by device_id string or index)
    auto monitors = get_monitors();
    const MonitorInfo* target_monitor = nullptr;
    if (!args.monitor_id.empty()) {
        for (const auto& m : monitors) {
            if (m.device_id == args.monitor_id) {
                target_monitor = &m;
                break;
            }
        }
        if (!target_monitor) {
            std::cerr << "Warning: monitor_id not found, falling back to index " << args.monitor_idx << std::endl;
        }
    }
    if (!target_monitor && args.monitor_idx >= 0 && args.monitor_idx < (int)monitors.size()) {
        target_monitor = &monitors[args.monitor_idx];
    }

    // Determine base (canvas) resolution from monitor info
    int base_width = 1920;
    int base_height = 1080;
    if (!args.test && args.window_id.empty() && target_monitor) {
        if (target_monitor->width > 0 && target_monitor->height > 0) {
            base_width = (int)target_monitor->width;
            base_height = (int)target_monitor->height;
            std::cout << "Auto-detected monitor resolution: " << base_width << "x" << base_height << std::endl;
        }
    }
    base_width &= 0xFFFFFFFC;
    base_height &= 0xFFFFFFFE;

    // output = user-requested encode resolution, or base if not specified
    int out_width = user_set_width ? (user_set_width & 0xFFFFFFFC) : base_width;
    int out_height = user_set_height ? (user_set_height & 0xFFFFFFFE) : base_height;

    obs_video_info ovi;
    memset(&ovi, 0, sizeof(obs_video_info));
    ovi.adapter = 0;
    ovi.graphics_module = "libobs-d3d11";
    ovi.fps_den = 1;
    ovi.base_width = base_width;
    ovi.base_height = base_height;
    ovi.output_width = out_width;
    ovi.output_height = out_height;
    ovi.fps_num = args.fps;
    ovi.output_format = VIDEO_FORMAT_NV12;
    ovi.colorspace = VIDEO_CS_709;
    ovi.range = VIDEO_RANGE_PARTIAL;
    ovi.gpu_conversion = true;
    ovi.scale_type = OBS_SCALE_BILINEAR;

    std::cout << "Canvas: " << ovi.base_width << "x" << ovi.base_height
              << ", Output: " << ovi.output_width << "x" << ovi.output_height << std::endl;

    if (!reset_video(&ovi, quiet_scan)) {
        std::cerr << "Failed to reset video" << std::endl;
        obs_shutdown();
        return -1;
    }

    // 3. Load modules AFTER video init (matches OBS Studio order)
    // This is critical: modules like win-capture need the graphics context
    // to be fully initialized before they load.
    std::cerr << "Loading modules..." << std::endl;
    obs_load_all_modules();
    std::cerr << "Post-loading modules..." << std::endl;
    obs_post_load_modules();

    // Create Monitor Source
    obs_source_t* monitor_source = nullptr;
    if (!args.test && !args.window_id.empty()) {
        obs_data_t* s = obs_data_create();
        obs_data_set_string(s, "window", args.window_id.c_str());
        obs_data_set_int(s, "capture_mode", 1);
        obs_data_set_bool(s, "allow_transparency", false);
        obs_data_set_bool(s, "force_scaling", false);
        monitor_source = obs_source_create("game_capture", "Game Capture", s, NULL);
        obs_data_release(s);
    } else {
        // Use the device_id from the target monitor found earlier
        std::string monitor_device_id;
        if (target_monitor) {
            monitor_device_id = target_monitor->device_id;
        }
        obs_data_t* monitor_settings = obs_data_create();
        obs_data_set_string(monitor_settings, "monitor_id", monitor_device_id.c_str());
        obs_data_set_int(monitor_settings, "method", args.method);
        monitor_source = obs_source_create("monitor_capture", "Screen Capture", monitor_settings, NULL);
        obs_data_release(monitor_settings);
    }

    if (monitor_source) {
        obs_data_t* source_settings = obs_source_get_settings(monitor_source);
        if (source_settings) {
            int method = (int)obs_data_get_int(source_settings, "method");
            std::cout << "Monitor capture method: " << monitor_method_name(method) << std::endl;
            std::cout << "Monitor capture copy path: " << infer_copy_path(method, args.encoder) << std::endl;
            obs_data_release(source_settings);
        }
    } else {
        std::cerr << "Failed to create monitor source" << std::endl;
    }

    // Create Scene
    obs_scene_t* scene = obs_scene_create("Main Scene");

    if (monitor_source) {
        obs_scene_add(scene, monitor_source);
        obs_source_release(monitor_source);
    }

    if (args.test) {
        std::cout << "Monitor source created. Test mode, output not started." << std::endl;
        clear_output_sources();
        obs_scene_release(scene);
        obs_shutdown();
        return 0;
    }

    obs_source_t* scene_source = obs_scene_get_source(scene);
    obs_set_output_source(0, scene_source);
    
    // Audio Sources
    if (!args.audio_desktop_id.empty()) {
        obs_data_t* settings = obs_data_create();
        obs_data_set_string(settings, "device_id", args.audio_desktop_id.c_str());
        obs_source_t* audio = obs_source_create("wasapi_output_capture", "Desktop Audio", settings, NULL);
        obs_data_release(settings);
        if (audio) {
            obs_set_output_source(1, audio);
            obs_source_release(audio);
        }
    }
    
    if (!args.audio_mic_id.empty()) {
        obs_data_t* settings = obs_data_create();
        obs_data_set_string(settings, "device_id", args.audio_mic_id.c_str());
        obs_source_t* audio = obs_source_create("wasapi_input_capture", "Mic Audio", settings, NULL);
        obs_data_release(settings);
        if (audio) {
            obs_set_output_source(2, audio);
            obs_source_release(audio);
        }
    }

    obs_output_t* output = NULL;
    obs_service_t* service = NULL;
    if (!args.rtmp_url.empty()) {
        output = obs_output_create("rtmp_output", "RTMP Stream", NULL, NULL);
        service = obs_service_create("rtmp_custom", "RTMP Service", NULL, NULL);
        obs_data_t* settings = obs_data_create();
        obs_data_set_string(settings, "server", args.rtmp_url.c_str());
        obs_data_set_string(settings, "key", args.rtmp_key.c_str());
        if (service) {
            obs_service_update(service, settings);
        }
        obs_data_release(settings);
        if (output && service) {
            obs_output_set_service(output, service);
        }
    } else if (!args.output_file.empty()) {
        output = obs_output_create("ffmpeg_muxer", "File Output", NULL, NULL);
        obs_data_t* settings = obs_data_create();
        obs_data_set_string(settings, "path", args.output_file.c_str());
        obs_output_update(output, settings);
        obs_data_release(settings);
    } else {
        std::cerr << "No output specified. Use --output <file> or --rtmp <url>" << std::endl;
        clear_output_sources();
        obs_scene_release(scene);
        obs_shutdown();
        return -1;
    }

    if (!output || (!args.rtmp_url.empty() && !service)) {
        std::cerr << "Failed to create output or service" << std::endl;
        if (output) {
            obs_output_release(output);
        }
        if (service) {
            obs_service_release(service);
        }
        clear_output_sources();
        obs_scene_release(scene);
        obs_shutdown();
        return -1;
    }

    // Encoders
    obs_encoder_t* v_encoder = obs_video_encoder_create(args.encoder.c_str(), "Video Encoder", NULL, NULL);
    obs_encoder_t* a_encoder = obs_audio_encoder_create("ffmpeg_aac", "Audio Encoder", NULL, 0, NULL);

    if (!v_encoder) {
         std::cerr << "Failed to create video encoder: " << args.encoder << ", falling back to obs_x264" << std::endl;
         v_encoder = obs_video_encoder_create("obs_x264", "Video Encoder", NULL, NULL);
    }

    // Configure Video Encoder
    obs_data_t* v_settings = obs_data_create();
    const char* encoder_id = obs_encoder_get_id(v_encoder);
    if (encoder_id && strstr(encoder_id, "nvenc")) {
        obs_data_set_bool(v_settings, "texture", true);
        obs_data_set_string(v_settings, "preset", "p3");
        obs_data_set_string(v_settings, "preset2", "p3");
        obs_data_set_string(v_settings, "multipass", "disabled");
        obs_data_set_bool(v_settings, "lookahead", false);
        obs_data_set_bool(v_settings, "adaptive_quantization", false);
        obs_data_set_bool(v_settings, "psycho_aq", false);
        obs_data_set_int(v_settings, "bf", 0);
    } else if (encoder_id && strstr(encoder_id, "amf")) {
        obs_data_set_string(v_settings, "preset", "speed");
        obs_data_set_int(v_settings, "bf", 0);
        obs_data_set_bool(v_settings, "pre_analysis", false);
    } else if (encoder_id && strstr(encoder_id, "qsv")) {
        obs_data_set_string(v_settings, "target_usage", "TU7");
        obs_data_set_string(v_settings, "latency", "ultra-low");
        obs_data_set_int(v_settings, "bframes", 0);
    } else if (encoder_id && strcmp(encoder_id, "obs_x264") == 0) {
        obs_data_set_string(v_settings, "preset", "superfast");
        obs_data_set_string(v_settings, "tune", "zerolatency");
    }
    obs_data_set_int(v_settings, "bitrate", args.bitrate);
    obs_encoder_update(v_encoder, v_settings);
    obs_data_release(v_settings);

    obs_data_t* a_settings = obs_data_create();
    obs_data_set_int(a_settings, "bitrate", 192);
    obs_encoder_update(a_encoder, a_settings);
    obs_data_release(a_settings);

    obs_encoder_set_preferred_video_format(v_encoder, VIDEO_FORMAT_NV12);
    obs_encoder_set_preferred_color_space(v_encoder, VIDEO_CS_709);
    obs_encoder_set_preferred_range(v_encoder, VIDEO_RANGE_PARTIAL);
    obs_encoder_set_video(v_encoder, obs_get_video());
    obs_encoder_set_audio(a_encoder, obs_get_audio());

    obs_output_set_video_encoder(output, v_encoder);
    obs_output_set_audio_encoder(output, a_encoder, 0);

    signal_handler_t *output_signals = obs_output_get_signal_handler(output);
    if (output_signals) {
        signal_handler_connect_ref(output_signals, "stop", output_stop_cb, NULL);
    }

    // Start
    if (!obs_output_start(output)) {
        std::cerr << "Failed to start output: " << obs_output_get_last_error(output) << std::endl;
        obs_output_release(output);
        obs_encoder_release(v_encoder);
        obs_encoder_release(a_encoder);
        clear_output_sources();
        obs_scene_release(scene);
        if (service) {
            obs_service_release(service);
        }
        return -1;
    }

    std::cout << "Capture started. Output: " << (args.rtmp_url.empty() ? args.output_file : args.rtmp_url) << std::endl;
    std::cout << "Press Ctrl+C to stop." << std::endl;

    while (keep_running) {
        process_ui_tasks();
        std::this_thread::sleep_for(std::chrono::milliseconds(1));
    }

    std::cout << "Stopping..." << std::endl;
    int reason = stop_reason.load();
    if (reason == StopReasonOutputStopped) {
        long long code = output_stop_code.load();
        std::string err;
        {
            std::lock_guard<std::mutex> lock(stop_mutex);
            err = output_stop_error;
        }
        if (!err.empty()) {
            std::cerr << "Output stopped. code=" << code << ", error=" << err << std::endl;
        } else {
            std::cerr << "Output stopped. code=" << code << std::endl;
        }
    } else if (reason == StopReasonSigInt) {
        std::cerr << "Stopped by SIGINT" << std::endl;
    } else if (reason == StopReasonSigTerm) {
        std::cerr << "Stopped by SIGTERM" << std::endl;
    } else if (reason == StopReasonSigBreak) {
        std::cerr << "Stopped by SIGBREAK" << std::endl;
    } else if (reason == StopReasonConsoleClose) {
        std::cerr << "Stopped by console close" << std::endl;
    }
    if (reason != StopReasonOutputStopped) {
        obs_output_stop(output);
        std::unique_lock<std::mutex> lock(stop_mutex);
        stop_cv.wait_for(lock, std::chrono::seconds(5), [] {
            return output_stop_received.load();
        });
    }
    obs_output_release(output);
    obs_encoder_release(v_encoder);
    obs_encoder_release(a_encoder);
    clear_output_sources();
    obs_scene_release(scene);
    if (service) {
        obs_service_release(service);
    }
    
    obs_shutdown();

    // Shutdown Realtime Work Queue
    if (hRtwq) {
        typedef HRESULT(STDAPICALLTYPE * PFN_RtwqShutdown)();
        auto func = (PFN_RtwqShutdown)GetProcAddress(hRtwq, "RtwqShutdown");
        if (func) func();
        FreeLibrary(hRtwq);
    }

    return 0;
}
