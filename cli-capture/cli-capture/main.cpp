#include <obs.h>
#include <callback/signal.h>
#include <callback/calldata.h>
#include <util/base.h>
#include <util/platform.h>
#include <util/dstr.h>
#include <util/config-file.h>
#include <windows.h>
#include <iostream>
#include <vector>
#include <string>
#include <thread>
#include <atomic>
#include <mutex>
#include <csignal>
#include <iomanip>

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
std::mutex stop_mutex;
std::string output_stop_error;

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
    keep_running = false;
}

struct Args {
    bool scan = false;
    int monitor_idx = 0;
    std::string audio_desktop_id;
    std::string audio_mic_id;
    std::string output_file;
    std::string rtmp_url;
    std::string rtmp_key;
    std::string encoder = "obs_x264";
    int bitrate = 2500;
    int width = 0; // 0 means auto-detect
    int height = 0;
    int fps = 30;
    int rotation = 0;
};

struct MonitorInfo {
    int index = 0;
    RECT rect = {};
    std::string device;
    uint32_t width = 0;
    uint32_t height = 0;
    int rotation = 0;
    bool primary = false;
};

struct MonitorEnumContext {
    std::vector<MonitorInfo> monitors;
    int index = 0;
};

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
    std::vector<std::pair<std::string, std::string>> monitor_list;
    for (const auto& m : monitors) {
        std::string name = "Display " + std::to_string(m.index + 1) + ": " +
                           std::to_string(m.width) + "x" + std::to_string(m.height) +
                           " @ " + std::to_string(m.rect.left) + "," + std::to_string(m.rect.top);
        if (m.primary) {
            name += " (Primary Monitor)";
        }
        if (m.rotation != 0) {
            name += " rot=" + std::to_string(m.rotation);
        }
        monitor_list.push_back({std::to_string(m.index), name});
    }
    print_json_array("screens", monitor_list);
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
        } else if (arg == "--monitor" && i + 1 < argc) {
            args.monitor_idx = std::stoi(argv[++i]);
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
        }
    }
}

int main(int argc, char *argv[]) {
    // Set console output to UTF-8 to fix mojibake
    SetConsoleOutputCP(CP_UTF8);
    SetConsoleCP(CP_UTF8);
    SetConsoleCtrlHandler(console_handler, TRUE);
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

    // Load modules
    if (!quiet_scan) {
        std::cerr << "Loading modules..." << std::endl;
    }
    obs_load_all_modules();
    if (!quiet_scan) {
        std::cerr << "Post-loading modules..." << std::endl;
    }
    obs_post_load_modules();

    // Need to initialize video context even for scanning some properties properly
    // Use a dummy resolution
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
    

    // Attempt to reset video early for better enumeration support if needed
    if (!quiet_scan) {
        std::cerr << "Resetting video..." << std::endl;
    }
    int ret = obs_reset_video(&ovi);
    if (ret != OBS_VIDEO_SUCCESS) {
        if (!quiet_scan) {
            std::cerr << "Failed to reset video, error code: " << ret << std::endl;
        }
    } else {
        if (!quiet_scan) {
            std::cerr << "Video reset successful." << std::endl;
        }
    }

    if (args.scan) {
        print_json_start();
        list_screens();
        list_audio_devices("wasapi_output_capture", "desktop_audio");
        list_audio_devices("wasapi_input_capture", "microphone");
        list_encoders();
        print_json_end();
        obs_shutdown();
        return 0;
    }

    // Capture Mode
    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);
#ifdef SIGBREAK
    signal(SIGBREAK, signal_handler);
#endif

    // Auto-detect resolution if not provided
    if (args.width == 0 || args.height == 0) {
        auto monitors = get_monitors();
        if (args.monitor_idx < (int)monitors.size()) {
            const auto& m = monitors[args.monitor_idx];
            args.width = (int)m.width;
            args.height = (int)m.height;
            args.rotation = m.rotation;
            std::cout << "Auto-detected resolution: " << args.width << "x" << args.height << std::endl;
        } else {
            args.width = 1920;
            args.height = 1080;
            args.rotation = 0;
            std::cerr << "Monitor index out of range, using default 1920x1080" << std::endl;
        }
    }

    // Reset Video with correct resolution
    args.width &= 0xFFFFFFFC;
    args.height &= 0xFFFFFFFE;
    ovi.base_width = args.width;
    ovi.base_height = args.height;
    ovi.output_width = args.width;
    ovi.output_height = args.height;
    ovi.fps_num = args.fps;
    ovi.output_format = VIDEO_FORMAT_NV12;
    ovi.colorspace = VIDEO_CS_709;
    ovi.range = VIDEO_RANGE_PARTIAL;
    ovi.gpu_conversion = true;

    if (obs_reset_video(&ovi) != OBS_VIDEO_SUCCESS) {
        std::cerr << "Failed to reset video" << std::endl;
        obs_shutdown();
        return -1;
    }

    // Reset Audio
    obs_audio_info oai;
    memset(&oai, 0, sizeof(obs_audio_info));
    oai.samples_per_sec = 48000;
    oai.speakers = SPEAKERS_STEREO;
    if (!obs_reset_audio(&oai)) {
        std::cerr << "Failed to reset audio" << std::endl;
        obs_shutdown();
        return -1;
    }

    // Create Scene
    obs_scene_t* scene = obs_scene_create("Main Scene");
    
    // Create Monitor Source
    obs_data_t* monitor_settings = obs_data_create();
    obs_data_set_int(monitor_settings, "monitor", args.monitor_idx);
    obs_source_t* monitor_source = obs_source_create("monitor_capture", "Screen Capture", monitor_settings, NULL);
    obs_data_release(monitor_settings);

    uint32_t canvas_width = args.width;
    uint32_t canvas_height = args.height;
    obs_video_info active_ovi;
    if (obs_get_video_info(&active_ovi)) {
        canvas_width = active_ovi.base_width;
        canvas_height = active_ovi.base_height;
    }

    if (monitor_source) {
        obs_sceneitem_t* item = obs_scene_add(scene, monitor_source);
        
        // Ensure it fills the screen
        obs_transform_info transform;
        memset(&transform, 0, sizeof(transform));
        obs_sceneitem_get_info2(item, &transform);
        transform.bounds_type = OBS_BOUNDS_SCALE_INNER;
        transform.bounds.x = (float)canvas_width;
        transform.bounds.y = (float)canvas_height;
        transform.alignment = OBS_ALIGN_CENTER;
        transform.bounds_alignment = OBS_ALIGN_CENTER;
        transform.pos.x = (float)canvas_width * 0.5f;
        transform.pos.y = (float)canvas_height * 0.5f;
        transform.rot = (float)args.rotation;
        obs_sceneitem_set_info2(item, &transform);
        
        obs_source_release(monitor_source);
    } else {
        std::cerr << "Failed to create monitor source" << std::endl;
    }

    obs_set_output_source(0, obs_scene_get_source(scene));
    
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
    obs_data_set_int(v_settings, "bitrate", args.bitrate);
    obs_encoder_update(v_encoder, v_settings);
    obs_data_release(v_settings);

    obs_data_t* a_settings = obs_data_create();
    obs_data_set_int(a_settings, "bitrate", 192);
    obs_encoder_update(a_encoder, a_settings);
    obs_data_release(a_settings);

    obs_encoder_set_video(v_encoder, obs_get_video());
    obs_encoder_set_audio(a_encoder, obs_get_audio());
    obs_encoder_set_preferred_video_format(v_encoder, VIDEO_FORMAT_NV12);
    obs_encoder_set_preferred_color_space(v_encoder, VIDEO_CS_709);
    obs_encoder_set_preferred_range(v_encoder, VIDEO_RANGE_PARTIAL);

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
        obs_scene_release(scene);
        if (service) {
            obs_service_release(service);
        }
        return -1;
    }

    std::cout << "Capture started. Output: " << (args.rtmp_url.empty() ? args.output_file : args.rtmp_url) << std::endl;
    std::cout << "Press Ctrl+C to stop." << std::endl;

    while (keep_running) {
        std::this_thread::sleep_for(std::chrono::milliseconds(100));
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
    obs_output_stop(output);
    obs_output_release(output);
    obs_encoder_release(v_encoder);
    obs_encoder_release(a_encoder);
    obs_scene_release(scene);
    if (service) {
        obs_service_release(service);
    }
    
    obs_shutdown();
    return 0;
}
