"""扫描引擎单元测试"""

import os
import sys
import tempfile
import unittest

# 确保能找到 src
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from src.utils import fmt_size, fmt_gb, path_to_file_url


class TestUtils(unittest.TestCase):

    def test_fmt_size_bytes(self):
        self.assertEqual(fmt_size(100), "100.0 B")

    def test_fmt_size_kb(self):
        self.assertEqual(fmt_size(2048), "2.0 KB")

    def test_fmt_size_mb(self):
        self.assertEqual(fmt_size(1048576 * 5), "5.0 MB")

    def test_fmt_size_gb(self):
        self.assertEqual(fmt_size(1073741824), "1.0 GB")

    def test_fmt_gb(self):
        self.assertAlmostEqual(fmt_gb(2147483648), 2.0, places=1)

    def test_fmt_gb_zero(self):
        self.assertEqual(fmt_gb(0), 0)

    def test_path_to_file_url(self):
        url = path_to_file_url(r"C:\Users\test\file.txt")
        self.assertEqual(url, "file:///C:/Users/test/file.txt")

    def test_path_to_file_url_backslash(self):
        url = path_to_file_url(r"D:\data\folder")
        self.assertEqual(url, "file:///D:/data/folder")


class TestFormatEdgeCases(unittest.TestCase):

    def test_fmt_size_zero(self):
        self.assertEqual(fmt_size(0), "0.0 B")

    def test_fmt_size_large(self):
        # 1 TB
        self.assertEqual(fmt_size(1099511627776), "1.0 TB")

    def test_fmt_size_negative(self):
        # 不应该有负数，但确保不崩溃
        result = fmt_size(-1)
        self.assertIn("B", result)


if __name__ == "__main__":
    unittest.main()
