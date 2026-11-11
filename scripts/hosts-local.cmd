@echo off
rem Point cloud-project.online and docs.cloud-project.online to this PC (127.0.0.1)
rem for "start.cmd local". Run as administrator.
rem   hosts-local.cmd          add the two lines to the hosts file
rem   hosts-local.cmd remove   remove them (before using the real office server)
setlocal
net session >nul 2>&1
if errorlevel 1 (
  echo Run this script as administrator ^(right click - Run as administrator^).
  exit /b 1
)
set "HOSTS=%SystemRoot%\System32\drivers\etc\hosts"
set "MARK=# cloud-project-local"
if /i "%~1"=="remove" (
  findstr /v /c:"%MARK%" "%HOSTS%" > "%TEMP%\hosts.cloud-project"
  copy /y "%TEMP%\hosts.cloud-project" "%HOSTS%" >nul
  del "%TEMP%\hosts.cloud-project"
  ipconfig /flushdns >nul
  echo Removed cloud-project.online from the hosts file.
  exit /b 0
)
findstr /c:"%MARK%" "%HOSTS%" >nul
if not errorlevel 1 (
  echo The hosts file already points cloud-project.online to this PC.
  exit /b 0
)
>>"%HOSTS%" echo.
>>"%HOSTS%" echo 127.0.0.1 cloud-project.online %MARK%
>>"%HOSTS%" echo 127.0.0.1 docs.cloud-project.online %MARK%
ipconfig /flushdns >nul
echo Added: cloud-project.online and docs.cloud-project.online -^> 127.0.0.1
