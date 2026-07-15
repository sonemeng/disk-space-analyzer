#!/usr/bin/env python3
"""磁盘空间分析器 — 入口"""

import sys
import os

# 确保能找到 src 包
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from src.app import main

if __name__ == "__main__":
    sys.exit(main())
