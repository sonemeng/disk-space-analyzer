@echo off
chcp 65001 >nul
title 打包磁盘空间分析器

echo ====================================
echo  打包磁盘空间分析器 v3.0
echo ====================================
echo.

cd /d "%~dp0.."

echo [1/3] 检查依赖...
pip show pyinstaller >nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo 正在安装 pyinstaller...
    pip install pyinstaller
)

echo [2/3] 打包中...
pyinstaller --onefile --windowed --icon=assets\icon.ico --name "磁盘空间分析器" --distpath dist --workpath build_tmp src\__main__.py

echo [3/3] 清理临时文件...
if exist build_tmp rmdir /s /q build_tmp
if exist 磁盘空间分析器.spec del /q 磁盘空间分析器.spec

echo.
echo ✅ 打包完成！
echo   输出: dist\磁盘空间分析器.exe
echo.
pause
