# CLAUDE.md

本文件只是入口。**项目指南以 [`AGENTS.md`](./AGENTS.md) 为准，动手前先读它**；架构看 [`ARCHITECTURE.md`](./ARCHITECTURE.md)，安装/环境变量/排错看 [`README.md`](./README.md)，CLI 参数看 [`cli-capture/cli.md`](./cli-capture/cli.md)。

本项目是面向 **SEGA Amusement Linkage Live System（ALLS）** 的专用软件，目标平台即 ALLS 所基于的 Windows 10 1809；不考虑非 Windows 或非预期 Windows 版本的兼容性，这类 issue 与 PR 不受理。

## 开工前先记住这几条

- **结构**：`server/`（Rust + Axum + SQLx，同时支持 SQLite 与 PostgreSQL）、`web-ui/`（Vite + React）、`cli-capture/`（OBS libobs 内核的控制台 CLI）。
- **命令**：`cd server && cargo test`、`cargo build --release`；`cd web-ui && npm run build`；一键 `.\build_all.ps1 server`。
- **改表结构**：`server/schema.sql` 与 `server/schema_sqlite.sql` **两份都要改**；查询用 `dbq!` 系列宏，不要直接 `sqlx::query`。
- **别用 `taskkill /F` 停采集进程**：会让 MP4 缺 `moov` 打不开；停止要走会话内 helper（`server.exe --stop-capture <pid>`）投递 `CTRL_BREAK`。
- **服务模式在 Session 0**：采集必须经会话注入（`server/src/core/session_launch.rs`），不能由服务直接拉起。
- **cli-capture 的 OBS 版本与补丁绑定**：`build_windows.bat` 里的 `OBS_REF` 与 `patches/0001-obs-build-flags.patch` 是一对，改一个必须重新生成另一个。
- **CI 只构建不跑测试**：本地 `cargo test` + 真机演练才是验证；涉及采集的改动要确认录出来的 MP4 有 `moov`。
