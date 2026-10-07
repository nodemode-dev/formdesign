@echo off
cd /d "%~dp0.."
python setup\line_counts.py "%cd%" %*
