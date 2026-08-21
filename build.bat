@echo off
setlocal

cd /d "%~dp0"

rem 如果 Windows 的 Schannel 在沙箱/代理环境下报 SEC_E_NO_CREDENTIALS，
rem 可以改用 Git 自带的 OpenSSL 后端拉取 crates.io 索引。
set CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git
set CARGO_NET_GIT_FETCH_WITH_CLI=true
set GIT_CONFIG_COUNT=1
set GIT_CONFIG_KEY_0=http.sslBackend
set GIT_CONFIG_VALUE_0=openssl

cargo build --release
if errorlevel 1 (
    echo Build failed.
    pause
    exit /b 1
)

echo.
echo Build OK: target\release\ntfy-client-gui.exe
pause
