# ALLS Recorder

基于 `cli-capture` 的录制/推流管理平台，提供后端服务、前端管理界面与采集 CLI 统一协作的完整方案。

## 模块组成

- `server/`：Rust 后端服务（Axum + SQLx），负责认证、设备探测、录制/推流任务调度、文件管理与系统配置。
- `web-ui/`：前端管理台与用户端界面（Vite + React + TypeScript），用于初始化向导、日常操作与用户配置。
- `cli-capture/`：基于 OBS libobs 的独立采集 CLI，提供硬件扫描与录制/推流能力。
- `server/schema.sql`：幂等数据库 schema 脚本，每次启动自动检查并补全所有表和列。
- `ARCHITECTURE.md`：架构与模块说明文档。

## 运行环境

- Rust（含 `cargo`）
- Node.js（含 `npm`）
- PostgreSQL
- Windows（服务模式需要）

## 使用流程

1. 发现后端：前端发现页会列出可用后端，未初始化的后端会标记提示。
2. 初始化：选择未初始化后端后进入初始化向导，配置数据库、JWT_SECRET 并创建管理员账号；初始化会写入 server/.env 并立即生效。
3. 登录：初始化完成后登录系统。
4. 管理台常用操作：
   - 设置 `cli-capture` 路径与全局录制限制。
   - 执行硬件探测并保存结果。
   - 发布公告、管理用户与录制文件。
   - 安装/卸载 Windows 系统服务（可选）。
5. 用户侧常用操作：
   - 录制或推流。
   - 设置自己的分辨率/码率/FPS 等参数。
   - 选择采集模式（屏幕/窗口）和采集方法（auto/dxgi/wgc）。
   - 查看在线用户并发起停止请求。

## 安装指南（推荐使用服务模式）

### 方式一：Windows 系统服务（生产推荐）

#### 服务模式架构

系统服务模式采用 Service + Agent 架构：
- Service（Session 0）：以 Windows 服务运行，负责 HTTP 接口和业务逻辑
- Agent（用户会话）：以用户身份运行，负责调用 cli-capture 进行屏幕录制
- 通信方式：Service 通过 TCP（默认端口 3001）与 Agent 通信

#### 安装步骤

1. 编译项目（见下方"编译指南"）
2. 确保 PostgreSQL 可访问
3. 通过前端界面安装：管理员控制台 → 常规操作 → 安装为系统服务

或通过命令行安装（会自动弹出 UAC 提升权限）：

```powershell
.\server.exe --install-service
```

安装完成后：
- 服务名称：`AllsRecorder`
- 服务自动启动：是（延时启动，开机后约 2 分钟才能就绪）
- Agent 自动启动：是（用户登录时）
- 配置文件：`C:\ProgramData\AllsRecorder\agent.json`

#### 服务管理

```powershell
sc start AllsRecorder    # 启动
sc stop AllsRecorder     # 停止
sc query AllsRecorder    # 状态

# 卸载
.\server.exe --uninstall-service
```

#### 服务模式说明

- 工作目录：服务启动时自动设置为可执行文件所在目录
- 安装/卸载需要管理员权限，运行不需要
- Agent 以当前登录用户身份运行
- 非服务模式：后端直接调用 cli-capture
- 服务模式：后端 → Agent → cli-capture（跨会话）
- 服务设置为延时启动，开机后约 2 分钟才能就绪

### 方式二：直接运行（开发/测试）

1. 编译后端和前端（见下方"编译指南"）
2. 在 `server/` 目录下创建 `.env` 文件（或通过前端初始化向导配置）
3. 运行后端：
   ```bash
   cd server
   ./target/release/server
   ```
4. 使用 Nginx/Caddy 等反向代理服务前端静态文件（`web-ui/dist/`）

## 编译指南

### 后端（Server）

```bash
cd server
cargo build --release
```

编译产物：`server/target/release/server.exe`（Windows）

### 前端（Web UI）

```bash
cd web-ui
npm install
npm run build
```

编译产物：`web-ui/dist/`（静态文件，可部署到任意 Web 服务器）

### 采集 CLI（cli-capture）

```powershell
cd cli-capture
scripts\build_windows.bat
```

编译产物：`cli-capture/dist/`

详细参数说明见 `cli-capture/cli.md`

### 一键构建

```powershell
.\build_all.ps1
```

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
   - 安装/卸载 Windows 系统服务（可选）。
5. 用户侧常用操作：
   - 录制或推流。
   - 设置自己的分辨率/码率/FPS 等参数。
   - 选择采集模式（屏幕/窗口）和采集方法（auto/dxgi/wgc）。
   - 查看在线用户并发起停止请求。

## 数据库说明

服务端使用 `server/schema.sql` 管理数据库结构，每次启动时自动执行。脚本完全幂等（`CREATE TABLE IF NOT EXISTS` + `ALTER TABLE ADD COLUMN IF NOT EXISTS`），无论是全新数据库还是旧版本升级都能正确处理。

无需手动执行迁移脚本。

## 命令行参数

### 后端

- `--install-service`：安装为 Windows 系统服务（需要管理员权限）
- `--uninstall-service`：卸载 Windows 系统服务（需要管理员权限）
- `--service`：以服务模式运行（由服务管理器调用）
- `--agent`：以 Agent 模式运行（用户会话）

### cli-capture

详见 `cli-capture/cli.md`

## 目录结构

```
.
├── server/              后端服务
│   ├── src/            源代码
│   ├── schema.sql      数据库 schema（幂等，启动时自动执行）
│   ├── target/         编译产物
│   └── .env            配置文件（需创建）
├── web-ui/             前端界面
│   ├── src/           源代码
│   └── dist/          编译产物
├── cli-capture/        采集 CLI
│   ├── cli-capture/   源代码
│   ├── scripts/       构建脚本
│   └── dist/          编译产物
├── ARCHITECTURE.md     架构说明
└── README.md           本文件
```

## 常见问题

### 服务模式启动后无法立即访问？

服务设置为延时启动，开机后约 2 分钟才能就绪。这是为了确保系统资源和依赖服务（如网络、数据库）完全初始化后再启动录制服务。

### 如何修改服务端口？

编辑 `server/src/main.rs` 中的端口配置（默认 3000），重新编译。

### 如何配置 HTTPS？

推荐使用 Nginx/Caddy 等反向代理处理 HTTPS，后端保持 HTTP。

### 可以在 Linux 上运行吗？

你看看项目名开头四个字母呢

### 如何备份数据？

备份以下内容：
- PostgreSQL 数据库
- `server/.env` 配置文件
- 录制文件存储目录

## 许可证

本项目使用的第三方组件许可证见 `cli-capture/LICENSES/` 目录。
