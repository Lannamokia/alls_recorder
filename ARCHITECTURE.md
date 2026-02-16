# 技术架构文档：ALLS Recorder 

## 1. 概述
本项目旨在为 `cli-capture` 工具提供一个基于 Web 的管理界面。系统将分为现代化的前端、健壮的后端以及现有的采集 CLI 工具。

## 2. 技术栈

### 前端 (Frontend)
- **框架**: Vite + React + TypeScript
- **UI 组件库**: TailwindCSS + Shadcn/UI (或类似组件库)
- **状态管理**: Zustand
- **数据请求**: TanStack Query (React Query)
- **路由**: React Router

### 后端 (Backend)
- **语言**: Rust
- **Web 框架**: Axum
- **数据库 ORM**: SQLx
- **数据库**: PostgreSQL
- **运行时**: Tokio
- **进程管理**: `std::process::Command` (用于管理 `cli-capture` 子进程)

### CLI (命令行工具)
- **核心**: 现有的 `cli-capture.exe` (基于 OBS)

## 3. 系统架构

```mermaid
graph TD
    Client[Web 客户端 (浏览器)] <-->|HTTP/REST/WS| Server[Rust 后端]
    Server <-->|SQL| DB[(PostgreSQL)]
    Server -->|Spawn/Kill| CLI[cli-capture 进程]
    CLI -->|Write| FS[文件系统 (录像文件)]
    Server -->|Read/Manage| FS
```

## 4. 数据库设计 (Schema)

### 数据表

1.  **`users` (用户表)**
    -   `id`: UUID (主键)
    -   `username`: VARCHAR (唯一)
    -   `password_hash`: VARCHAR
    -   `role`: VARCHAR ('admin', 'user')
    -   `created_at`: TIMESTAMP

2.  **`system_config` (系统配置表)**
    -   用于存储全局设置的键值对。
    -   `key`: VARCHAR (主键)
    -   `value`: JSONB
    -   *键示例: `db_configured`, `admin_created`, `max_bitrate` (最大码率), `max_fps` (最大帧率), `max_res` (最大分辨率), `video_encoder` (视频编码器), `hardware_info` (硬件信息)*

3.  **`user_configs` (用户配置表)**
    -   `user_id`: UUID (外键)
    -   `max_bitrate`: INT (最大码率)
    -   `max_fps`: INT (最大帧率)
    -   `resolution`: VARCHAR (分辨率)
    -   `audio_channel`: VARCHAR (音频通道)
    -   `rtmp_url`: VARCHAR (推流地址)
    -   `rtmp_key`: VARCHAR (推流密钥)

4.  **`announcements` (公告表)**
    -   `id`: UUID (主键)
    -   `content`: TEXT (内容)
    -   `created_at`: TIMESTAMP
    -   `created_by`: UUID (外键 -> users.id)

5.  **`user_read_announcements` (用户已读公告表)**
    -   `user_id`: UUID
    -   `announcement_id`: UUID
    -   `read_at`: TIMESTAMP

6.  **`recordings` (录像记录表)**
    -   用于存储文件的元数据。
    -   `id`: UUID
    -   `user_id`: UUID
    -   `filename`: VARCHAR (文件名)
    -   `filepath`: VARCHAR (文件路径)
    -   `status`: VARCHAR ('recording', 'stopped', 'saved') (状态：录制中/已停止/已保存)
    -   `created_at`: TIMESTAMP

## 5. 关键模块与逻辑

### 5.1 初始化流程 (Initialization)
1.  **检测阶段**: 服务器启动时检测本地是否存在 `init.lock` 文件。
2.  **无锁 (未初始化)**:
    -   对所有请求返回初始化引导页面 (`InitPage`)。
    -   **步骤 1**: 配置数据库连接 (主机, 端口, 用户名, 密码, 数据库名)。后端尝试连接测试。
    -   **步骤 2**: 设置管理员账号 (用户名, 密码)。**密码必须输入两次并确认一致。**
    -   **步骤 3**: 创建 `init.lock` 文件，将数据库配置写入环境变量或配置文件，并在数据库中创建管理员用户。
3.  **有锁 (已初始化)**: 进入正常运行模式，显示登录页面。

### 5.2 认证与注册 (Authentication)
-   采用基于 JWT 的身份验证。
-   **登录**: 管理员和普通用户均通过登录页面进入。
-   **普通用户注册**:
    -   未登录状态下访问主页，显示“登录/注册”选项。
    -   用户点击注册，填写用户名、密码及确认密码。
    -   **密码规则**: 前端校验两次输入的密码是否一致，一致后才提交给后端。
    -   后端创建用户记录，默认角色为 `user` (普通用户)。
-   **管理员管理**: 管理员也可在后台手动添加管理员账户。

### 5.3 进程管理 (Process Management)
-   **全局进程映射**: `Arc<RwLock<HashMap<UserId, ChildProcess>>>`。
-   **开始录制**:
    -   检查用户是否已有运行中的进程。
    -   获取用户配置（若未配置则使用全局限制）。
    -   构建 CLI 参数: `cli-capture.exe --bitrate ... --encoder ...`。
    -   生成子进程 (Spawn)，保存句柄。
-   **停止录制**:
    -   根据 UserId 查找进程。
    -   发送 SIGTERM/Kill 信号 (或通过 stdin 交互停止)。
    -   等待进程退出，更新数据库中的状态。

### 5.4 硬件探测 (Hardware Probe)
-   管理员触发“硬件探测”操作。
-   后端运行 `cli-capture.exe --scan` 。
-   探测结果 (JSON) 保存到 `system_config` 表的 `hardware_info` 字段中。
-   每次运行探测都会覆盖旧数据。
-   **参数使用规则**:
    -   **屏幕或音频通道**: 前端显示使用 `name` 字段 (便于用户识别)，后端调用 CLI 时使用 `id` 字段。
    -   **编码器参数**: 前端显示和后端调用 CLI 均统一使用 `id` 字段。

### 5.5 停止请求流程 (Stop Request - Inter-User)
1.  **发起请求**: 普通用户 A 请求停止用户 B 的录制。
2.  **后端处理**: 创建一个“待处理请求” (内存或数据库中)。
3.  **通知**: 用户 B (若在线) 收到网页通知 (轮询或 WebSocket)。
4.  **用户 B 允许**:
    -   前端发送“接受”指令给后端。
    -   后端停止用户 B 的进程。
    -   后端启动用户 A 的进程。
5.  **用户 B 拒绝**:
    -   清除请求，不执行任何操作。

## 6. 目录结构

```
/
├── ARCHITECTURE.md     (本文档)
├── server/             (Rust 后端代码)
│   ├── src/
│   │   ├── api/        (API 路由与处理函数)
│   │   ├── core/       (核心逻辑：进程管理, 硬件探测)
│   │   ├── db/         (数据库模型与操作)
│   │   └── main.rs
│   ├── migrations/     (数据库迁移脚本)
│   └── Cargo.toml
├── web-ui/             (Vite + React 前端代码)
│   ├── src/
│   │   ├── components/ (UI 组件)
│   │   ├── pages/      (页面视图)
│   │   └── hooks/      (自定义 Hooks)
│   └── package.json
└── cli-capture/        (现有的 CLI 工具)
```

## 7. 后续步骤 (TODO)
请参考 Todo List 查看详细的实施步骤。
