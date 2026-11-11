@echo off
rem Start CLOUD-PROJECT with Docker.
rem   start.cmd        local: http://localhost:8080
rem   start.cmd prod   office server: HTTPS on cloud-project.online (docker-compose.prod.yml)
setlocal
cd /d "%~dp0.."
if not exist .env (
  echo .env not found. Copy .env.example to .env and set the secrets first.
  exit /b 1
)
set FILES=-f docker-compose.yml
if /i "%~1"=="prod" set FILES=-f docker-compose.yml -f docker-compose.prod.yml
docker compose %FILES% up -d --build
if errorlevel 1 exit /b 1
docker compose %FILES% ps
echo.
if /i "%~1"=="prod" (
  echo Portal: https://cloud-project.online   Documents: https://docs.cloud-project.online
) else (
  echo Portal: http://localhost:8080   Documents: http://localhost:8081
)
echo ONLYOFFICE needs 1-2 minutes after the first start.
