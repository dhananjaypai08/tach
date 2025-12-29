from __future__ import annotations

import pytest

pytest_plugins = ["pytester"]


@pytest.fixture
def tach_project(pytester: pytest.Pytester):
    """Create a basic tach project structure."""
    _ = pytester.makefile(".toml", tach='source_roots = ["."]')
    _ = pytester.makepyfile(
        src_module="""
def add(a, b):
    return a + b

def subtract(a, b):
    return a - b
""",
        test_with_import="""
from src_module import add

def test_add_basic():
    assert add(1, 2) == 3

def test_add_zero():
    assert add(0, 0) == 0

def test_add_negative():
    assert add(-1, 1) == 0
""",
        test_no_import="""
def test_standalone_1():
    assert True

def test_standalone_2():
    assert 1 + 1 == 2
""",
    )
    # Initialize git repo
    _ = pytester.run("git", "init")
    _ = pytester.run("git", "config", "user.email", "test@test.com")
    _ = pytester.run("git", "config", "user.name", "Test")
    _ = pytester.run("git", "add", "-A")
    _ = pytester.run("git", "commit", "-m", "initial")
    return pytester


def run_pytest(pytester: pytest.Pytester, *args: str) -> pytest.RunResult:
    """Run pytest in subprocess to avoid PyO3 reinitialization issues."""
    return pytester.runpytest_subprocess("-p", "tach.pytest_plugin", *args)


class TestPytestPluginSkipping:
    def test_no_changes_skips_all_tests(self, tach_project: pytest.Pytester):
        """When there are no changes, all tests should be skipped."""
        result = run_pytest(tach_project, "--tach-base", "HEAD")
        result.assert_outcomes(passed=0)
        result.stdout.fnmatch_lines(["*Skipped 2 test file*"])

    def test_source_change_runs_dependent_tests(self, tach_project: pytest.Pytester):
        """When a source file changes, only tests that import it should run."""
        # Modify the source file
        _ = tach_project.makepyfile(
            src_module="""
def add(a, b):
    return a + b

def subtract(a, b):
    return a - b

# Modified
"""
        )
        _ = tach_project.run("git", "add", "src_module.py")
        _ = tach_project.run("git", "commit", "-m", "modify source")

        result = run_pytest(tach_project, "--tach-base", "HEAD~1")
        result.assert_outcomes(passed=3)
        result.stdout.fnmatch_lines(
            [
                "*Skipped 1 test file*",
                "*test_no_import.py*",
            ]
        )

    def test_test_file_change_runs_that_file(self, tach_project: pytest.Pytester):
        """When a test file is directly modified, it should run."""
        # Modify a test file
        _ = tach_project.makepyfile(
            test_no_import="""
def test_standalone_1():
    assert True

def test_standalone_2():
    assert 1 + 1 == 2

def test_standalone_3():
    assert "new test"
"""
        )
        _ = tach_project.run("git", "add", "test_no_import.py")
        _ = tach_project.run("git", "commit", "-m", "add test")

        result = run_pytest(tach_project, "--tach-base", "HEAD~1")
        result.assert_outcomes(passed=3)
        result.stdout.fnmatch_lines(["*Skipped 1 test file*"])


class TestPytestPluginCounting:
    def test_counts_all_tests_in_file(self, tach_project: pytest.Pytester):
        """Should correctly count all tests including parametrized ones."""
        _ = tach_project.makepyfile(
            test_parametrized="""
import pytest

@pytest.mark.parametrize("x,y,expected", [
    (1, 2, 3),
    (2, 3, 5),
    (10, 20, 30),
])
def test_param_add(x, y, expected):
    assert x + y == expected

def test_regular():
    assert True
"""
        )
        _ = tach_project.run("git", "add", "test_parametrized.py")
        _ = tach_project.run("git", "commit", "--amend", "--no-edit")

        result = run_pytest(tach_project, "--tach-base", "HEAD")
        result.assert_outcomes(passed=0)
        # 3 (test_with_import) + 2 (test_no_import) + 4 (test_parametrized) = 9
        result.stdout.fnmatch_lines(["*Skipped 3 test file* (9 tests)*"])

    def test_counts_tests_in_classes(self, tach_project: pytest.Pytester):
        """Should correctly count tests inside test classes."""
        _ = tach_project.makepyfile(
            test_class="""
class TestGroup:
    def test_one(self):
        assert True

    def test_two(self):
        assert True

class TestAnotherGroup:
    def test_three(self):
        assert True
"""
        )
        _ = tach_project.run("git", "add", "test_class.py")
        _ = tach_project.run("git", "commit", "--amend", "--no-edit")

        result = run_pytest(tach_project, "--tach-base", "HEAD")
        result.assert_outcomes(passed=0)
        # 3 (test_with_import) + 2 (test_no_import) + 3 (test_class) = 8
        result.stdout.fnmatch_lines(["*Skipped 3 test file* (8 tests)*"])
