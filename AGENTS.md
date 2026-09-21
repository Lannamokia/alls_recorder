# AGENTS.md — 项目指南（给编码 agent）

> 本项目是面向 **SEGA Amusement Linkage Live System（ALLS）** 的专用软件，目标平台即 ALLS 所基于的 **Windows 10 1809**。不考虑非 Windows 或非预期 Windows 版本的兼容性，这类 issue 与 PR 不受理。写代码时不要为"跨平台兼容""支持其它 Windows 版本"做额外工作。

## 项目是什么

浏览器管理台 → Rust 后端（`server/`，Axum + SQLx，托管前端产物）→ 在**用户会话**里拉起 OBS 内核的采集 CLI（`cli-capture/`）做录屏/推流；前端在 `web-ui/`（Vite + React）。默认零配置用内置 SQLite。

深入细节看 `ARCHITECTURE.md`；安装/环境变量/排错看 `README.md`；CLI 参数看 `cli-capture/cli.md`。

## 常用命令

```bash
# 后端
cd server && cargo build --release      # 产物 server/target/release/server.exe
cd server && cargo test                 # 仓库里只有 5 个单测，别指望它兜住
cd server && cargo run                  # 前台跑，默认 0.0.0.0:3000

# 前端
cd web-ui && npm ci && npm run build    # tsc -b + vite build（会做类型检查）
cd web-ui && npm run dev                # http://localhost:5173
cd web-ui && npm run lint

# 采集 CLI（很慢，会 clone OBS 并编译，需要 VS + CMake + 网络）
cd cli-capture && scripts\build_windows.bat

# 一键构建（产物统一到 dist/）
.\build_all.ps1                         # 全部
.\build_all.ps1 server                  # 只后端
```

部署与排障：

```powershell
sc query AllsRecorder                   # 服务状态
Stop-Service AllsRecorder               # 需要管理员；运行时 server.exe 被占用，覆盖前必须停
Copy-Item server\target\release\server.exe dist\server\server.exe -Force
Start-Service AllsRecorder
Get-Content dist\server\logs\server.log.<日期> -Tail 50   # 服务模式唯一的错误现场
```

**CI 只构建、不跑测试**（`.github/workflows/build-release.yml`：cargo build + npm build + cli 构建 + 打 zip）。所以本地的 `cargo test` 和真机演练是唯一防线。

## 代码地图

| 路径 | 作用 | 什么时候改 |
| --- | --- | --- |
| `server/src/main.rs` | 入口：参数分支（`--service` / `--install-service` / `--uninstall-service` / `--install-agent` / `--agent` / `--stop-capture`）、日志初始化、路由装配、内置托管 web-ui | 加启动参数、改服务/Agent 安装逻辑、改端口 |
| `server/src/api/` | HTTP handler：auth、setup、discovery、hardware、recorder、files、announcements、settings、user_config、users、service | 改接口行为 |
| `server/src/core/recorder.rs` | 录制任务管理（直接拉起 / 会话注入 / Agent 回退、优雅停止） | 改采集进程生命周期 |
| `server/src/core/session_launch.rs` | **会话注入**核心：token 获取、`CreateProcessAsUserW`、会话监视、`run_in_session`、停止 helper | 改拉起/停止采集的方式 |
| `server/src/core/hardware.rs` | 硬件扫描编排（会话内扫描 → Agent 回退） | 改探测逻辑 |
| `server/src/core/agent*.rs` | Agent 服务端与客户端（可选回退路径） | 改回退协议 |
| `server/src/db/mod.rs` | 连接池、方言 schema、`dbq!` 系列宏 | 加表/改查询方式 |
| `server/schema.sql`、`server/schema_sqlite.sql` | 两种方言的幂等建表脚本 | **改表结构必须同时改这两份** |
| `web-ui/src/pages/`、`components/` | 发现页/初始化/登录/用户端/管理台 | 改界面与交互 |
| `cli-capture/cli-capture/main.cpp` | 采集 CLI 自有源码（OBS 之上） | 改 CLI 行为/新增参数 |
| `cli-capture/scripts/build_windows.bat` | OBS 拉取 + 补丁 + CMake 构建 | 改构建方式 |
| `cli-capture/patches/` | obs-studio 补丁（与 `OBS_REF` 绑定） | 升级 OBS 时重新生成 |

## 约定

- **注释与日志用中文**，讲"为什么"而不是"做了什么"；服务端日志走 `tracing`。
- 内部函数用 `anyhow::Result`；HTTP handler 返回 `(StatusCode, String)` 或 `Json<...>`。
- API 鉴权在 handler 里做：解析 JWT、校验角色，越权返回 403。
- **数据库**：查询别直接 `sqlx::query`，用 `dbq!` / `dbq_as!` / `dbq_scalar!` / `dbq_tx!`（`server/src/db/mod.rs`，内部按方言双分支）；占位符两种方言都写 `$1` 风格。加表/加列要同时更新 `schema.sql` 与 `schema_sqlite.sql`。
- **前端**：组件内直连 `${baseUrl}/api/...`，`baseUrl` 取 `localStorage.backend_url`（默认 `http://localhost:3000`）。
- Windows 专用代码用 `#[cfg(windows)]` 包住（服务、会话注入、进程结束等），别把 Windows API 直接暴露给通用路径。
- Commit：conventional 前缀 + 中文（`feat(api): …` / `fix(service): …` / `docs: …`），**按功能模块拆分**，别把不相关的改动塞进一个 commit；不要提交 `dist/`、`target/`、`node_modules/`、`.env`。

## 易踩的坑（都是实际踩过的）

1. **服务模式运行在 Session 0**：没有桌面、没有控制台，不能让服务直接 spawn `cli-capture` 去采集。必须经会话注入（`CreateProcessAsUser` 到活动控制台会话）。
2. **控制台是会话内对象**：Session 0 的进程 `AttachConsole` 不到会话 1 里采集进程的控制台，所以停止动作要在用户会话里做——这就是 `server.exe --stop-capture <pid>` helper 存在的原因。
3. **停止采集不能用 `taskkill /F`**：硬杀 = OBS 来不及写 MP4 尾部，产出没有 `moov` 的坏文件（播放器打不开）。要投递 `CTRL_BREAK` 让它自己收尾，20 秒兜底才强杀。
4. **`SessionLauncher::start()` 必须同步探测一次会话初值**：异步监视任务的第一次 tick 要等运行时调度，初值留空会让服务启动后第一个请求误报「没有可用的活动用户会话」（曾出现过这个真实 bug，有回归测试守着）。
5. **`cli-capture --scan` 慢且抖动**：要完整初始化 OBS，实测同一台机器连续几次是 2s / 14s / 90s+；而且 stdout 只在进程退出时才刷新（缓冲），所以别做"读到输出就提前收工"的优化。服务端超时是 180 秒。
6. **obs-studio 版本与补丁是绑定的**：`build_windows.bat` 里 `OBS_REF` 固定 tag，`patches/0001-obs-build-flags.patch` 针对该 tag 生成。换 `OBS_REF` 必须重新生成补丁，否则补丁失配 → `check_obs_browser()` 没被注释 → CMake 报 `Required submodule 'obs-browser' not available`。
7. **`.bat` 里的两个 cmd 坑**：双层嵌套 `if` 块里的 `exit /b N` 会丢退出码（写单层）；块内 `echo` 的文本里带括号会破坏块解析。报错信息尽量用 ASCII，避免编码/引号问题。
8. **`dist/` 是构建产物目录（被 gitignore）**：`dist/server/server.exe` 是正在运行的服务镜像，服务运行时无法覆盖（`Device or resource busy`），需要管理员停服务。
9. **服务模式下 stdout 会被丢弃**：日志写在 `<exe_dir>/logs/server.log.<日期>`（按天滚动、启动清理 7 天前）；非服务模式加 `ALLS_LOG_FILE=1` 也会写文件。
10. **需要有人处于活动登录状态**：锁屏可以采集；只通过 RDP 访问、物理控制台断开时取不到会话（报「没有可用的活动用户会话」）。端口 3000 写死在 `main.rs`，Agent 端口 `AGENT_PORT`（默认 3001），服务端连接目标 `AGENT_ADDR`。

## 验证标准

- 改 Rust：`cargo build --release` + `cargo test`；**涉及会话拉起/停止的改动必须真机验证**——起服务、录一段、停止，确认日志出现「已向 pid=… 投递 CTRL_BREAK」「采集进程 pid=… 已优雅退出」，并确认 MP4 顶层盒子是 `ftyp/free/mdat/moov`（缺 `moov` 就是坏了）。
- 改前端：`npm run build`（含 `tsc -b` 类型检查）。
- 改采集 CLI：`build_windows.bat`，产物 `cli-capture/dist/`；改完至少手工验证 `--scan` 与一次录制+停止。
- 改 schema：两份方言文件都改，且分别用 SQLite（默认）与 PostgreSQL 各起一次确认幂等执行无报错。
- 交付前按用户的实际场景端到端跑一遍，不要只跑编译。
