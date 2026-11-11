@echo off
rem Point cloud-project.online and docs.cloud-project.online to this PC (127.0.0.1)
rem for "start.cmd local". Asks for administrator rights itself (UAC prompt).
rem   hosts-local.cmd          add the two lines to the hosts file
rem   hosts-local.cmd remove   remove them (before using the real office server)
setlocal
net session >nul 2>&1
if errorlevel 1 (
  echo Administrator rights are needed, Windows will ask for them...
  if "%~1"=="" (
    powershell -NoProfile -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
  ) else (
    powershell -NoProfile -Command "Start-Process -FilePath '%~f0' -ArgumentList '%~1' -Verb RunAs"
  )
  exit /b 0
)
set "HOSTS=%SystemRoot%\System32\drivers\etc\hosts"
set "MARK=# cloud-project-local"
set RC=0
if /i "%~1"=="remove" goto remove
findstr /c:"%MARK%" "%HOSTS%" >nul
if not errorlevel 1 (
  echo The hosts file already points cloud-project.online to this PC.
  goto done
)
>>"%HOSTS%" echo.
if errorlevel 1 goto denied
>>"%HOSTS%" echo 127.0.0.1 cloud-project.online %MARK%
>>"%HOSTS%" echo 127.0.0.1 docs.cloud-project.online %MARK%
findstr /c:"%MARK%" "%HOSTS%" >nul
if errorlevel 1 goto denied
ipconfig /flushdns >nul
echo Added: cloud-project.online and docs.cloud-project.online -^> 127.0.0.1
goto done

:remove
findstr /v /c:"%MARK%" "%HOSTS%" > "%TEMP%\hosts.cloud-project"
copy /y "%TEMP%\hosts.cloud-project" "%HOSTS%" >nul
if errorlevel 1 goto denied
del "%TEMP%\hosts.cloud-project"
ipconfig /flushdns >nul
echo Removed cloud-project.online from the hosts file.
goto done

:denied
set RC=1
echo Could not write %HOSTS%.
echo An antivirus may protect the hosts file: allow the change or add the lines by hand:
echo   127.0.0.1 cloud-project.online
echo   127.0.0.1 docs.cloud-project.online

:done
echo.
pause
exit /b %RC%
