@echo off
setlocal enabledelayedexpansion

set "ROOT=%~dp0.."
set "OBS_DIR=%ROOT%\obs-studio"

if not exist "%OBS_DIR%\" (
  git clone https://github.com/obsproject/obs-studio.git "%OBS_DIR%"
)

if exist "%OBS_DIR%\cli-capture" (
  rmdir /s /q "%OBS_DIR%\cli-capture"
)
mkdir "%OBS_DIR%\cli-capture"
xcopy /E /I /Y "%ROOT%\cli-capture\*" "%OBS_DIR%\cli-capture\" >nul

pushd "%OBS_DIR%"
git apply --reverse --check "%ROOT%\patches\0001-obs-build-flags.patch" >nul 2>&1
if %errorlevel% neq 0 (
  git apply "%ROOT%\patches\0001-obs-build-flags.patch"
)

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
