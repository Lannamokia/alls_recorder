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
- Windows（服务模式需要）

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

编译产物：`server/target/release/server.exe`（Windows）或 `server/target/release/server`（Linux）

### 前端（Web UI）

```bash
cd web-ui
npm install
npm run build
```

编译产物：`web-ui/dist/`（静态文件，可部署到任意 Web 服务器）

### 采集 CLI（cli-capture）

Windows：

```powershell
cd cli-capture
.\build_all.ps1
```

编译产物：`cli-capture/dist/cli-capture.exe`

详细说明见 `cli-capture/cli.md`

## 部署指南

### 方式一：直接运行（开发/测试）

1. 编译后端和前端
2. 在 `server/` 目录下创建 `.env` 文件（或通过前端初始化向导配置）
3. 运行后端：
   ```bash
   cd server
   ./target/release/server
   ```
4. 使用 Nginx/Caddy 等反向代理服务前端静态文件（`web-ui/dist/`）

### 方式二：Windows 系统服务（生产推荐）

#### 服务模式架构

系统服务模式采用 Service + Agent 架构：
- **Service（Session 0）**：以 Windows 服务运行，负责 HTTP 接口和业务逻辑
- **Agent（用户会话）**：以用户身份运行，负责调用 cli-capture 进行屏幕录制
- **通信方式**：Service 通过 TCP（默认端口 3001）与 Agent 通信

#### 自动安装（推荐）

**方式 1：通过前端界面**

1. 以管理员权限运行后端
2. 登录管理员账号
3. 进入"管理员控制台" → "常规操作"标签页
4. 点击"安装为系统服务"按钮
5. 如果后端没有管理员权限，会提示使用命令行方式

**方式 2：通过命令行**

```powershell
# 直接运行（会自动弹出 UAC 提升权限）
.\server.exe --install-service
```

安装完成后：
- 服务名称：`AllsRecorder`
- 服务自动启动：是
- Agent 自动启动：是（用户登录时）
- 配置文件：`C:\ProgramData\AllsRecorder\agent.json`

#### 手动配置

如果需要自定义配置：

1. 编辑 Agent 配置文件：`C:\ProgramData\AllsRecorder\agent.json`
   ```json
   {
     "auto_start": true,
     "service_url": "http://localhost:3000",
     "agent_port": 3001,
     "log_level": "info"
   }
   ```

2. 设置环境变量（可选）：
   - `AGENT_PORT`：Agent 监听端口（默认 3001）
   - `RUN_AS_SERVICE`：设置为 `1` 强制服务模式

#### 服务管理

```powershell
# 启动服务
sc start AllsRecorder

# 停止服务
sc stop AllsRecorder

# 查看服务状态
sc query AllsRecorder

# 卸载服务（方式 1：通过前端）
# 在管理员控制台点击"卸载系统服务"

# 卸载服务（方式 2：通过命令行）
.\server.exe --uninstall-service

# 卸载服务（方式 3：手动）
sc stop AllsRecorder
sc delete AllsRecorder
reg delete "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" /v AllsRecorderAgent /f
```

#### 服务模式说明

- **工作目录**：服务启动时自动设置为可执行文件所在目录，确保正确读取 `.env` 等配置文件
- **权限要求**：
  - 安装/卸载服务需要管理员权限
  - 服务运行不需要管理员权限
  - Agent 以当前登录用户身份运行
- **日志位置**：
  - 服务日志：根据 `RUST_LOG` 环境变量配置
  - Agent 日志：根据 `agent.json` 中的 `log_level` 配置
- **录制模式**：
  - 非服务模式：后端直接调用 cli-capture
  - 服务模式：后端 → Agent → cli-capture（跨会话）

#### 故障排查

1. **服务无法启动**
   - 检查 `.env` 文件是否存在于 `server.exe` 所在目录
   - 检查数据库连接配置是否正确
   - 查看 Windows 事件查看器中的服务日志

2. **Agent 无法连接**
   - 检查 Agent 是否在运行（任务管理器）
   - 检查端口 3001 是否被占用
   - 检查防火墙设置

3. **录制失败**
   - 确认 cli-capture 路径配置正确
   - 确认 Agent 以正确的用户身份运行
   - 检查用户是否有桌面会话



## 命令行参数

后端支持以下命令行参数：

- `--install-service`：安装为 Windows 系统服务（需要管理员权限）
- `--uninstall-service`：卸载 Windows 系统服务（需要管理员权限）
- `--service`：以服务模式运行（由服务管理器调用）
- `--agent`：以 Agent 模式运行（用户会话）

## CLI 采集工具

`cli-capture` 的构建与参数说明见：

- `cli-capture/cli.md`

## 目录结构

```
.
├── server/              后端服务
│   ├── src/            源代码
│   ├── migrations/     数据库迁移
│   ├── target/         编译产物
│   └── .env            配置文件（需创建）
├── web-ui/             前端界面
│   ├── src/           源代码
│   └── dist/          编译产物
├── cli-capture/        采集 CLI
│   ├── cli-capture/   源代码
│   └── dist/          编译产物
├── ARCHITECTURE.md     架构说明
├── service.md          服务模式技术文档
└── README.md           本文件
```

## 常见问题

### 1. 如何修改服务端口？

编辑 `server/src/main.rs` 中的端口配置（默认 3000），重新编译。

### 2. 如何配置 HTTPS？

推荐使用 Nginx/Caddy 等反向代理处理 HTTPS，后端保持 HTTP。

### 3. 服务模式和非服务模式有什么区别？

- **非服务模式**：后端直接调用 cli-capture，适合开发和测试
- **服务模式**：后端通过 Agent 调用 cli-capture，适合生产环境，支持跨会话录制

### 4. 可以在 Linux 上运行吗？

你看看项目名开头四个字母呢

### 5. 如何备份数据？

备份以下内容：
- PostgreSQL 数据库
- `server/.env` 配置文件
- 录制文件存储目录

## 许可证

本项目使用的第三方组件许可证见 `cli-capture/LICENSES/` 目录。
