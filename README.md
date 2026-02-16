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

创建数据库（名称可自定义）：

```sql
CREATE DATABASE alls_recorder;
```

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

1. 初始化：首次访问前端会进入初始化向导，配置数据库并创建管理员账号。
2. 登录：使用管理员或普通用户账号登录。
3. 管理台常用操作：
   - 设置 `cli-capture` 路径与全局录制限制。
   - 执行硬件探测并保存结果。
   - 发布公告、管理用户与录制文件。
4. 用户侧常用操作：
   - 录制或推流。
   - 设置自己的分辨率/码率/FPS 等参数。
   - 查看在线用户并发起停止请求。

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
