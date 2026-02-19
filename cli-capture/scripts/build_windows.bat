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

set "VS_PATH=C:\Program Files\Microsoft Visual Studio\18\Community"
set "CMAKE_PATH=C:\Program Files\Microsoft Visual Studio\18\Community\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin"

if exist "%VS_PATH%\VC\Auxiliary\Build\vcvars64.bat" (
  call "%VS_PATH%\VC\Auxiliary\Build\vcvars64.bat"
) else (
  echo Error: vcvars64.bat not found at expected location.
  exit /b 1
)

set "PATH=%CMAKE_PATH%;%PATH%"

if exist "build" rmdir /s /q build
mkdir build
cd build

cmake .. -A x64 -DENABLE_BROWSER=OFF -DENABLE_VST=OFF -DENABLE_SCRIPTING=OFF -DENABLE_UI=OFF -DENABLE_WEBSOCKET=OFF -DENABLE_AJA=OFF -DENABLE_DECKLINK=OFF -DENABLE_NEW_MPEGTS_OUTPUT=OFF
if %errorlevel% neq 0 exit /b 1

cmake --build . --config RelWithDebInfo
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

if exist "%OBS_DIR%\" rmdir /s /q "%OBS_DIR%"
