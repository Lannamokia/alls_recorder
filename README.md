# ALLS Recorder

基于 `cli-capture` 的录制/推流管理平台，提供后端服务、前端管理界面与采集 CLI 统一协作的完整方案。

## 模块组成

- `server/`：Rust 后端服务（Axum + SQLx），负责认证、设备探测、录制/推流任务调度、文件管理与系统配置。
- `web-ui/`：前端管理台与用户端界面（Vite + React + TypeScript），用于初始化向导、日常操作与用户配置。
- `cli-capture/`：基于 OBS libobs 的独立采集 CLI，提供硬件扫描与录制/推流能力。
- `server/migrations/`：数据库迁移脚本。
- `ARCHITECTURE.md`：架构与模块说明文档。

## 运行环境

- Rust（含 `cargo`）
- Node.js（含 `npm`）
- PostgreSQL

## 快速开始

### 1. 数据库

确保 PostgreSQL 可访问，并提供具备创建数据库权限的账号。初始化流程会在必要时自动创建数据库。

### 2. 后端（Server）

```bash
cd server
```

根据 `server/.env.example` 创建本地环境配置文件并填写数据库连接信息，随后启动：

```bash
cargo run
```

默认监听 `http://localhost:3000`。

### 3. 前端（Web UI）

```bash
cd web-ui
npm install
npm run dev
```

默认启动在 `http://localhost:5173`。

## 使用流程

1. 发现后端：前端发现页会列出可用后端，未初始化的后端会标记提示。
2. 初始化：选择未初始化后端后进入初始化向导，配置数据库、JWT_SECRET 并创建管理员账号；初始化会写入 server/.env 并立即生效。
3. 登录：初始化完成后登录系统。
4. 管理台常用操作：
   - 设置 `cli-capture` 路径与全局录制限制。
   - 执行硬件探测并保存结果。
   - 发布公告、管理用户与录制文件。
5. 用户侧常用操作：
   - 录制或推流。
   - 设置自己的分辨率/码率/FPS 等参数。
   - 查看在线用户并发起停止请求。

## 数据库迁移说明

- 新环境：使用 `server/migrations/20240216000000_init.sql` 初始化数据库即可。
- 既有环境：若已存在旧库，需要补上 `user_configs.monitor_id` 列：

```sql
ALTER TABLE user_configs ADD COLUMN IF NOT EXISTS monitor_id VARCHAR(50);
```

## 编译指南

### 后端（Server）

```bash
cd server
cargo build --release
```

### 前端（Web UI）

```bash
cd web-ui
npm install
npm run build
```

### 采集 CLI（cli-capture）

Windows：

```powershell
cd cli-capture
scripts\build_windows.bat
```
产物由构建脚本输出到默认目录。

## ~~Windows Service 配置~~

**尚未完成，未启用**

<del>

推荐使用 NSSM 注册服务，确保服务工作目录为 `server/`。服务模式需要传入 `--service` 或设置 `RUN_AS_SERVICE=1`。

1. 编译后端二进制：

```powershell
cd server
cargo build --release
```

2. 安装服务（以管理员权限运行）：

```powershell
nssm install AllsRecorder "C:\path\to\alls_recorder\server\target\release\server.exe"
nssm set AllsRecorder AppDirectory "C:\path\to\alls_recorder\server"
nssm set AllsRecorder AppParameters "--service"
nssm set AllsRecorder AppStdout "C:\path\to\alls_recorder\server\logs\service.out.log"
nssm set AllsRecorder AppStderr "C:\path\to\alls_recorder\server\logs\service.err.log"
nssm set AllsRecorder AppRotateFiles 1
nssm set AllsRecorder AppRotateBytes 10485760
nssm set AllsRecorder Start SERVICE_AUTO_START
nssm set AllsRecorder AppExit Default Restart
nssm set AllsRecorder AppRestartDelay 5000
```

3. 启动服务：

```powershell
nssm start AllsRecorder
```

说明：
- 服务账号需有数据库访问权限、录制目录读写权限、CLI 可执行文件访问权限。

### 纯 SC.exe 方式（无需 NSSM）

以管理员 PowerShell 运行：

```powershell
sc.exe create AllsRecorder binPath= "C:\path\to\alls_recorder\server\target\release\server.exe --service" start= auto
sc.exe failure AllsRecorder reset= 0 actions= restart/5000
sc.exe start AllsRecorder
```

### 卸载服务

```powershell
nssm stop AllsRecorder
nssm remove AllsRecorder confirm
```

```powershell
sc.exe stop AllsRecorder
sc.exe delete AllsRecorder
```
</del>

## CLI 采集工具

`cli-capture` 的构建与参数说明见：

- `cli-capture/README.md`
- `cli-capture/cli.md`

## 目录结构

```
.
├── server/          后端服务
├── web-ui/          前端界面
├── cli-capture/     采集 CLI
├── server/migrations/ 数据库迁移
├── ARCHITECTURE.md  架构说明
└── README.md
```
