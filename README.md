# ALLS Recorder

基于 `cli-capture` 的录制/推流管理平台，提供后端服务、前端管理界面与采集 CLI 统一协作的完整方案。

## 模块组成

- `server/`：Rust 后端服务（Axum + SQLx），负责认证、设备探测、录制/推流任务调度、文件管理与系统配置；同时内置托管前端产物。
- `web-ui/`：前端管理台与用户端界面（Vite + React + TypeScript），用于初始化向导、日常操作与用户配置。
- `cli-capture/`：基于 OBS libobs 的独立采集 CLI，提供硬件扫描与录制/推流能力。
- `server/schema.sql` / `server/schema_sqlite.sql`：幂等数据库 schema（分别对应 PostgreSQL 与 SQLite），每次启动自动执行。
- `ARCHITECTURE.md`：架构与模块说明文档。

## 运行环境

- Windows 10/11 x64 —— 采集端（OBS + win-capture）仅支持 Windows，服务模式与会话内拉起也是 Windows 专用实现
- Rust（stable，含 `cargo`）
- Node.js（含 `npm`）—— 仅构建/开发前端时需要
- PostgreSQL —— **可选**；不配置时使用内置 SQLite，单机零配置即可运行
- 构建 `cli-capture` 额外需要：Visual Studio（含 C++ 工具链与 CMake）与可访问 GitHub 的网络（下载 OBS 依赖）

## 运行时形态

| 形态 | 进程身份 | 采集进程怎么起 |
| --- | --- | --- |
| 直接运行 | 你启动它的身份（前台） | 作为子进程直接拉起 |
| Windows 服务（生产推荐） | `LocalSystem`，Session 0 | 由服务用 `CreateProcessAsUser` 在**活动控制台会话**里以该用户身份拉起 |
| Agent（可选回退） | 当前登录用户，Session ≥1 | 服务通过 TCP 请求 Agent，由 Agent 以用户身份拉起 |

## 使用流程

1. 发现后端：前端发现页会列出可用后端（默认探测 `3000` 端口，也可扫描网段），未初始化的后端会标记提示。
2. 初始化：选择未初始化后端后进入初始化向导 —— 第一步选数据库（内置 SQLite 或 PostgreSQL）并设置 `JWT_SECRET`，第二步创建管理员账号；初始化会写入服务端目录下的 `.env` 并立即生效。
3. 登录：初始化完成后登录系统。
4. 管理台常用操作：
   - 设置 `cli-capture` 路径与全局录制目录。
   - 执行硬件探测并保存结果。
   - 安装/卸载 Windows 系统服务、安装可选 Agent。
   - 发布公告、管理用户与录制文件。
5. 用户侧常用操作：
   - 录制或推流。
   - 设置自己的分辨率/码率/FPS 等参数。
   - 选择采集模式（屏幕/窗口）和采集方法（auto/dxgi/wgc）。
   - 查看在线用户并发起停止请求。

## 快速开始（单机零配置）

1. 构建产物（见下方「编译指南」），或解压发行包。
2. 直接运行 `dist/server/server.exe`。首次启动会在可执行文件旁创建 `data/alls_recorder.db`（SQLite），无需任何数据库配置。
3. 浏览器打开 <http://localhost:3000> —— 服务端会自动托管 `server.exe` 旁的 `web-ui/` 目录；如果该目录不存在，则只提供 API（此时可用 `npm run dev` 起前端开发服务器，或自行部署 `dist/web-ui/`）。
4. 按向导初始化（选 SQLite → 填 `JWT_SECRET` → 建管理员）。
5. 进管理台：填 `cli-capture.exe` 路径、设置全局录制目录，然后跑一次硬件探测。

日志：服务模式写可执行文件旁 `logs/server.log.YYYY-MM-DD`（按天滚动，启动时清理 7 天前）；非服务模式设置 `ALLS_LOG_FILE=1` 也会写文件，否则走 stdout。

## 安装指引

### 方式一：Windows 系统服务（生产推荐）

#### 前置

- 已构建出的目录，例如 `dist/server/`（含 `server.exe` 与 `web-ui/`）与 `dist/cli-capture/`（含 `bin/64bit/cli-capture.exe`）。
- 安装/卸载需要管理员权限，安装完成后**服务的日常运行不需要**。
- 机器上需要有**一个活动的控制台会话**（见下方「服务模式如何采集屏幕」）。

#### 安装

在管理员 PowerShell 中执行（`--install-service` 会自动请求 UAC 提升）：

```powershell
cd dist\server
.\server.exe --install-service
```

也可以在前端管理台点「安装为系统服务」：它调用 `POST /api/service/install`，需要服务端进程本身具备管理员权限（以服务运行时天然满足）。

脚本实际做的事：`sc create AllsRecorder binPath="<exe> --service" start=delayed-auto DisplayName="Alls Recorder Service"`，并写入服务描述。服务账号为 `LocalSystem`，启动类型为**延迟自动**。

#### 安装后的目录与文件

服务启动时会把工作目录设为可执行文件所在目录，因此这些都落在 `server.exe` 旁边：

```
dist/server/
├── server.exe
├── web-ui/                 # 前端产物，服务端自动托管
├── data/alls_recorder.db   # 默认 SQLite 数据库（用 PostgreSQL 时不会有）
├── logs/server.log.*       # 按天滚动的日志，唯一的错误现场
├── .env                    # 初始化向导写入：DATABASE_URL / RUST_LOG / JWT_SECRET
└── init.lock               # 初始化完成标记
```

#### 服务模式如何采集屏幕

Windows 服务运行在 Session 0，没有桌面权限，无法直接录制。服务会：

- 用 `CreateProcessAsUser` 在**活动控制台会话**里以该用户身份拉起 `cli-capture`（token 优先取该会话 `explorer.exe` 的，即与用户自己双击运行时一致）；
- 因此需要有人在该机器上处于**活动登录状态**。锁屏属于可采集状态；**只通过 RDP 连接、物理控制台处于断开**的机器取不到会话，会报「没有可用的活动用户会话」；
- 探测不到会话时会退回 Agent（若已安装）；都没有则该次操作失败并给出提示。

#### 可选：安装 Agent（回退路径）

Agent 是一个 `ONLOGON` 计划任务（任务名 `AllsRecorderAgent`，`/RL LIMITED /IT`），以当前用户身份常驻并监听 TCP 3001，供服务在没有会话注入条件时回退使用：

```powershell
.\server.exe --install-agent
```

配置文件：`C:\ProgramData\AllsRecorder\agent.json`。端口用环境变量 `AGENT_PORT` 调整（默认 3001），服务端连接目标用 `AGENT_ADDR`（默认 `127.0.0.1:3001`）。

> 早期版本把 Agent 作为主路径、并随服务一起安装；现在会话注入是主路径，Agent 只是回退，安装服务时**不会**再自动创建这个任务。

#### 服务管理

```powershell
sc start AllsRecorder      # 启动
sc stop AllsRecorder       # 停止
sc query AllsRecorder      # 状态
```

#### 卸载

```powershell
.\server.exe --uninstall-service
```

会依次停止并删除服务、删除 `AllsRecorderAgent` 计划任务、删除 `C:\ProgramData\AllsRecorder` 配置目录。

#### 排错入口

先看 `logs/server.log.<日期>`，常见几类信息：

- `会话内扫描失败（…），回退到采集代理: 没有可用的活动用户会话` —— 没有活动控制台会话（无人登录 / 仅 RDP）。登录一次或安装 Agent。
- `扫描超时（180 秒），已终止进程；期间 stdout 收到 N 字节` —— `cli-capture --scan` 本身太慢（要完整初始化 OBS，单机实测从 2 秒到 90 秒以上都可能），N=0 说明子进程连输出都没来得及刷。
- `采集进程 pid=… 已优雅退出` / `…没有退出，强制结束` —— 前者正常（MP4 尾部已写出），后者说明停止超时被强杀，文件可能不完整。
- `会话 1 使用 explorer.exe 的用户 token` / `改用 WTSQueryUserToken` —— 拉起采集进程时实际用的 token 来源。

### 方式二：直接运行（开发/测试）

```bash
cd server
cargo run                 # 默认监听 0.0.0.0:3000
```

前端开发服务器：

```bash
cd web-ui
npm install
npm run dev               # http://localhost:5173
```

前端访问的后端地址取自 `localStorage.backend_url`（默认 `http://localhost:3000`），发现页负责探测与切换。

### 方式三：前端交给独立 Web 服务器

后端只提供 API 时，可把 `dist/web-ui/` 交给 Nginx/Caddy 等托管。后端已开启宽松 CORS，前端记得把 `backend_url` 指向后端地址与端口。

## 录制与采集行为

- **输出位置**：管理台设置的「全局录制目录」（`system_config.global_recording_path`）会与文件名拼接后传给 CLI；**未设置时传的是裸文件名**，文件会落在采集进程的工作目录（服务模式下即 `server.exe` 所在目录），建议先在管理台配置好目录。
- **停止录制**：服务端向采集进程投递 `CTRL_BREAK`（会话内由 helper `server.exe --stop-capture <pid>` 完成），让 OBS 停止输出并写出 MP4 尾部（moov）后自行退出，正常几百毫秒；若 20 秒仍未退出才强制结束，此时日志会警告、文件可能因缺 moov 而打不开。
- **硬件探测耗时**：`--scan` 需要加载全部 OBS 模块并做一次视频/音频初始化，耗时波动很大（同一台机器连续几次可能是 2s / 14s / 90s）。服务端超时设为 180 秒，超时后回退 Agent。
- **采集方法**：`auto`（在 dxgi 与 wgc 间自动选择）/`dxgi`/`wgc`，可在用户参数里指定。

## 编译指南

### 一键构建

```powershell
# 编译所有组件（server + web-ui + cli-capture）
.\build_all.ps1

# 只编译指定组件
.\build_all.ps1 server              # 只编译后端
.\build_all.ps1 web-ui              # 只编译前端
.\build_all.ps1 cli-capture         # 只编译采集 CLI
.\build_all.ps1 server web-ui       # 编译后端和前端

# 查看帮助
.\build_all.ps1 -Help
```

产物统一输出到：

- `dist/server/server.exe`，并把前端同步内置到 `dist/server/web-ui/`
- `dist/web-ui/`（静态文件）
- `dist/cli-capture/`（`cli-capture.exe` 与依赖库）

### 后端（Server）

```bash
cd server
cargo build --release
```

产物：`server/target/release/server.exe`。

### 前端（Web UI）

```bash
cd web-ui
npm install
npm run build
```

产物：`web-ui/dist/`。

### 采集 CLI（cli-capture）

```powershell
cd cli-capture
scripts\build_windows.bat
```

产物：`cli-capture/dist/`（脚本最后会把 `rundir` 整体拷到 `..\..\dist`，即 `cli-capture/dist/`）。

构建脚本的固定点，改动时要一起动：

- **Visual Studio 版本**：脚本用 `-G "Visual Studio 18 2026"` 显式指定生成器（CMake 从 vswhere 找到的 VS 里取）。只装了 VS 2022 的环境会报找不到生成器，改成本机对应的生成器名即可。
- **obs-studio 版本**：脚本内 `OBS_REF=32.1.0-rc3`，`git clone --branch` 固定到该 tag；
- **补丁** `cli-capture/patches/0001-obs-build-flags.patch`：注释掉 frontend 与一组用不到的插件（含需要子模块的 obs-browser / obs-websocket / win-dshow），并加入 `add_subdirectory(cli-capture)`。补丁是针对上面的 tag 生成的，脚本会先校验检出标签、应用失败立即报错。

升级 OBS 的流程：改 `OBS_REF` → 检出新版本 → 按同样意图改 `CMakeLists.txt` 与 `plugins/CMakeLists.txt` → `git diff > patches\0001-obs-build-flags.patch`。

### CI

`.github/workflows/build-release.yml` 在 push / tag / release 时构建全部组件并打包为 `alls-recorder-server-*.zip`、`alls-recorder-web-ui-*.zip`、`alls-recorder-cli-capture-*.zip`。

## 数据库说明

- **默认 SQLite**：未设置 `DATABASE_URL` 时使用 `<exe_dir>/data/alls_recorder.db`，随软件目录分发、免安装。
- **PostgreSQL**：设置 `DATABASE_URL=postgres://user:pass@host:port/db`，或在初始化向导或管理台里填连接信息（向导会在必要时自动 `CREATE DATABASE`）。
- **Schema**：`server/schema.sql`（PG）与 `server/schema_sqlite.sql`（SQLite）均为幂等脚本，每次启动自动执行，无需手动迁移。

## 命令行参数

### 后端（server.exe）

- `--install-service`：安装为 Windows 系统服务（需要管理员权限，自动请求 UAC）
- `--uninstall-service`：卸载服务，并清理 Agent 计划任务与配置目录
- `--install-agent`：安装可选的 Agent 自启计划任务
- `--service`：以服务模式运行（由服务管理器调用，一般不用手敲）
- `--agent`：以 Agent 模式运行（由计划任务调用）
- `--stop-capture <pid>`：内部使用，由服务在用户会话里拉起，用于给采集进程投递 `CTRL_BREAK`，不要手动调用

### 环境变量

| 变量 | 作用 |
| --- | --- |
| `DATABASE_URL` | 数据库连接串；不设则为内置 SQLite |
| `JWT_SECRET` | JWT 签名密钥（初始化向导会写入 `.env`） |
| `RUST_LOG` | 日志级别，默认 `server=debug,tower_http=debug` |
| `ALLS_LOG_FILE` | 置 `1` 时非服务模式也写 `logs/` 文件 |
| `RUN_AS_SERVICE` | 置 `1` 等价于 `--service`（供 NSSM 等托管场景） |
| `AGENT_ADDR` | 服务端连接 Agent 的地址，默认 `127.0.0.1:3001` |
| `AGENT_PORT` | Agent 监听端口，默认 `3001` |

### cli-capture

详见 `cli-capture/cli.md`（`--scan` / `--scan-windows` / `--monitor` / `--window` / `--method` / `--output` / `--rtmp` / `--key` / `--encoder` / `--bitrate` / `--width` / `--height` / `--fps` / `--desktop-audio` / `--mic-audio` / `--test`）。

## 目录结构

```
.
├── server/              后端服务
│   ├── src/             源代码
│   ├── schema.sql       PostgreSQL schema（幂等，启动时自动执行）
│   ├── schema_sqlite.sql SQLite schema（幂等，启动时自动执行）
│   ├── target/          编译产物
│   └── .env             配置文件（初始化向导写入；服务模式下位于 exe 旁）
├── web-ui/              前端界面
│   ├── src/             源代码
│   └── dist/            编译产物
├── cli-capture/         采集 CLI
│   ├── cli-capture/     源代码
│   ├── scripts/         构建脚本
│   ├── patches/         obs-studio 补丁
│   └── dist/            编译产物
├── build_all.ps1        一键构建
├── ARCHITECTURE.md      架构说明
└── README.md            本文件
```

## 常见问题

### 服务安装后无法立即访问？

服务是**延迟自动**启动，开机后约 2 分钟才就绪，日志里出现 `listening on 0.0.0.0:3000` 后再试。

### 硬件探测或开始录制报「没有可用的活动用户会话」？

服务需要机器上有一个活动的控制台会话（锁屏也可以）。无人登录或只通过 RDP 访问时取不到会话，表现为回退到采集代理；装 Agent（`--install-agent`）可覆盖这种场景。

### 扫描很慢，或者日志里出现「扫描超时」？

`cli-capture --scan` 要完整初始化 OBS，耗时本身就不稳定（实测 2s~90s+）。服务端超时是 180 秒，频繁超时说明这台机器上 OBS 初始化被拖慢（通常是音频/显示驱动等待），属于 CLI 侧问题。

### 录出来的文件播放器打不开？

多半是采集进程被强制结束（`taskkill /F`、断电），MP4 缺少尾部 `moov`。正常停止会投递 `CTRL_BREAK` 让 OBS 写完尾部；若日志出现「没有退出，强制结束」就是这个原因。已损坏的文件可用 `untrunc` 配合一个同编码器参数的正常文件重建索引。

### 如何修改服务端口？

端口目前写在代码里：`server/src/main.rs` 中 `SocketAddr::from(([0, 0, 0, 0], 3000))`，改后重新编译。前端默认连接的后端地址在 `localStorage.backend_url`（发现页可改），Agent 端口用 `AGENT_PORT`。

### 如何配置 HTTPS？

推荐用 Nginx/Caddy 反向代理处理 HTTPS，后端保持 HTTP。

### 可以在 Linux 上运行吗？

服务端核心逻辑与平台无关，但服务模式、会话内拉起、录制链路都依赖 Windows；`cli-capture` 只能在 Windows 上构建运行。本项目目标平台是 Windows x64，未在 Linux 上验证。

### 如何备份数据？

备份以下内容：

- 数据库：内置 SQLite 时是 `<exe_dir>/data/alls_recorder.db`，或 PostgreSQL 库
- 服务端目录下的 `.env` 配置文件
- 录制文件所在目录（管理台设置的全局录制目录）

## 许可证

本项目使用的第三方组件许可证见 `cli-capture/LICENSES/` 目录。
