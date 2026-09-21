@echo off
setlocal enabledelayedexpansion

set "ROOT=%~dp0.."
set "OBS_DIR=%ROOT%\obs-studio"

rem 固定 obs-studio 版本：patches\0001-obs-build-flags.patch 是针对该 tag 生成的。
rem 跟随默认分支会在上游改动 CMakeLists.txt / plugins\CMakeLists.txt 后静默失配，
rem 表现为 check_obs_browser() 没被注释掉，CMake 报
rem "Required submodule 'obs-browser' not available"。
rem 升级 OBS 时：把 OBS_REF 换成新 tag，检出新版本后按同样意图改这两个文件，
rem 再执行 git diff > patches\0001-obs-build-flags.patch 重新生成补丁。
set "OBS_REF=32.1.0-rc3"

rem 注意：cmd 在**双层嵌套**的 if 块里执行 exit /b 会丢掉退出码，
rem 所以这里的错误检查都写成单层，不要往 if 里再套 if。
if exist "%OBS_DIR%\" goto :obs_ready
git clone --branch %OBS_REF% https://github.com/obsproject/obs-studio.git "%OBS_DIR%"
if errorlevel 1 (
  echo Error: failed to clone obs-studio.
  exit /b 1
)
:obs_ready

if exist "%OBS_DIR%\cli-capture" (
  rmdir /s /q "%OBS_DIR%\cli-capture"
)
mkdir "%OBS_DIR%\cli-capture"
xcopy /E /I /Y "%ROOT%\cli-capture\*" "%OBS_DIR%\cli-capture\" >nul

pushd "%OBS_DIR%"
for /f "delims=" %%i in ('git describe --tags --exact-match 2^>nul') do set "OBS_HEAD=%%i"
if not "%OBS_HEAD%"=="%OBS_REF%" (
  echo Error: obs-studio is at "%OBS_HEAD%", but the patch targets "%OBS_REF%".
  echo        Run: git -C "%OBS_DIR%" checkout %OBS_REF%
  popd
  exit /b 1
)
set "PATCH_MISSING=0"
git apply --reverse --check "%ROOT%\patches\0001-obs-build-flags.patch" >nul 2>&1
if errorlevel 1 set "PATCH_MISSING=1"
if "%PATCH_MISSING%"=="1" git apply "%ROOT%\patches\0001-obs-build-flags.patch"
if errorlevel 1 (
  echo Error: failed to apply patches\0001-obs-build-flags.patch - obs-studio version mismatch?
  popd
  exit /b 1
)
if "%PATCH_MISSING%"=="1" echo applied obs-studio patch

set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if exist "%VSWHERE%" (
  for /f "usebackq delims=" %%i in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VS_PATH=%%i"
)
if not defined VS_PATH set "VS_PATH=C:\Program Files\Microsoft Visual Studio\18\Community"
set "CMAKE_PATH=%VS_PATH%\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin"

if exist "%VS_PATH%\VC\Auxiliary\Build\vcvars64.bat" (
  call "%VS_PATH%\VC\Auxiliary\Build\vcvars64.bat"
) else (
  echo Error: vcvars64.bat not found at expected location.
  exit /b 1
)

set "PATH=%CMAKE_PATH%;%PATH%"

rem 某些精简环境（如 AI 代理的托管 shell）缺少 PROCESSOR_ARCHITECTURE，
rem 会导致 CMake 检测不到主机架构并在 Qt 检查处报莫名其妙的错误。
if not defined PROCESSOR_ARCHITECTURE set "PROCESSOR_ARCHITECTURE=AMD64"

if exist "build" rmdir /s /q build
if exist "build_x86" rmdir /s /q build_x86
if exist "build_x64" rmdir /s /q build_x64
mkdir build
cd build

rem 显式指定 Visual Studio 生成器：vcvars64 之后 cl.exe 在 PATH 中，
rem CMake 会默认选择 NMake Makefiles，而 -A x64 仅 VS 生成器支持。
cmake .. -G "Visual Studio 18 2026" -A x64 "-DCMAKE_CXX_FLAGS=/D_SILENCE_EXPERIMENTAL_COROUTINE_DEPRECATION_WARNINGS /GR /EHsc" -DENABLE_BROWSER=OFF -DENABLE_VST=OFF -DENABLE_SCRIPTING=OFF -DENABLE_UI=OFF -DENABLE_WEBSOCKET=OFF -DENABLE_AJA=OFF -DENABLE_DECKLINK=OFF -DENABLE_NEW_MPEGTS_OUTPUT=OFF -DCMAKE_TLS_VERIFY=0 -DGPU_PRIORITY_VAL=7
if %errorlevel% neq 0 (
  echo.
  echo CMake configure failed. If the error mentions that no instance of Visual
  echo Studio could be found, the VS installation is incomplete ^(e.g. pending
  echo reboot^). Reboot the machine to finish the VS setup, then re-run this script.
  exit /b 1
)

rem 关闭 vcpkg 用户级 MSBuild 集成：若机器装了 vcpkg 并启用了全局集成，
rem 其旧版 AMF 头文件会插到 include 路径最前面，导致 .deps 中的新版 AMF 宏被遮蔽。
cmake --build . --config RelWithDebInfo -- /p:VcpkgEnabled=false
if %errorlevel% neq 0 exit /b 1

if exist "cli-capture\RelWithDebInfo\cli-capture.exe" (
  copy /Y "cli-capture\RelWithDebInfo\cli-capture.exe" "rundir\RelWithDebInfo\bin\64bit\" >nul
)

set "DEPS_ROOT=%OBS_DIR%\.deps"
if exist "%DEPS_ROOT%\" (
  for /d %%D in ("%DEPS_ROOT%\obs-deps-*-x64") do (
    if exist "%%D\bin\" (
      copy /Y "%%D\bin\*.dll" "rundir\RelWithDebInfo\bin\64bit\" >nul
    )
  )
)

if exist "..\..\dist" rmdir /s /q "..\..\dist"
mkdir "..\..\dist"
xcopy /E /I /Y "rundir\RelWithDebInfo\*" "..\..\dist\" >nul

popd
