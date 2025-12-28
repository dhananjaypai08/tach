from __future__ import annotations

from pathlib import Path
from typing import Any, Generator, Protocol

import pytest
from pytest import Collector

from tach import filesystem as fs
from tach.errors import TachSetupError
from tach.extension import TachPytestPluginHandler
from tach.filesystem.git_ops import get_changed_files
from tach.parsing import parse_project_config


class TachConfig(Protocol):
    tach_handler: TachPytestPluginHandler

    def getoption(self, name: str) -> Any: ...


class HasTachConfig(Protocol):
    config: TachConfig


def pytest_addoption(parser: pytest.Parser):
    group = parser.getgroup("tach")
    group.addoption(
        "--tach-base",
        default="main",
        help="Base commit to compare against when determining affected tests [default: main]",
    )
    group.addoption(
        "--tach-head",
        default="",
        help="Head commit to compare against when determining affected tests [default: current filesystem]",
    )


@pytest.hookimpl(tryfirst=True)
def pytest_configure(config: TachConfig):
    project_root = fs.find_project_config_root() or Path.cwd()
    project_config = parse_project_config(root=project_root)
    if project_config is None:
        raise TachSetupError("In Tach pytest plugin: No project config found")

    base = config.getoption("--tach-base")
    head = config.getoption("--tach-head")

    kwargs: dict[str, Any] = {"project_root": project_root}
    if head:
        kwargs["head"] = head
    if base:
        kwargs["base"] = base
    changed_files = get_changed_files(**kwargs)

    # Store the handler instance on the config object so other hooks can access it
    config.tach_handler = TachPytestPluginHandler(
        project_root=project_root,
        project_config=project_config,
        changed_files=changed_files,
        all_affected_modules={changed_file.resolve() for changed_file in changed_files},
    )


def _count_items(collector: Collector) -> int:
    """Recursively count test items from a collector."""
    count = 0
    for item in collector.collect():
        if isinstance(item, Collector):
            # It's a collector (e.g., Class), recurse
            count += _count_items(item)
        else:
            # It's a test item
            count += 1
    return count


@pytest.hookimpl(wrapper=True)
def pytest_collect_file(
    file_path: Path, parent: HasTachConfig
) -> Generator[None, list[Collector], list[Collector]]:
    handler = parent.config.tach_handler
    # Skip any paths that already get filtered out by other hook impls
    result = yield
    if not result:
        return result

    resolved_path = file_path.resolve()

    # If this test file was changed, keep it
    if str(resolved_path) in handler.all_affected_modules:
        return result

    # Check if file should be removed based on its imports
    if handler.should_remove_items(file_path=resolved_path):
        # Recursively count all test items before discarding
        for collector in result:
            handler.num_removed_items += _count_items(collector)
        handler.remove_test_path(file_path)
        return []

    return result


def pytest_report_collectionfinish(
    config: TachConfig,
    start_path: Path,
    startdir: Any,
    items: list[pytest.Item],
) -> str | list[str]:
    handler = config.tach_handler
    return [
        f"[Tach] Skipped {len(handler.removed_test_paths)} test file{'s' if len(handler.removed_test_paths) > 1 else ''}"
        f" ({handler.num_removed_items} tests)"
        " since they were unaffected by current changes.",
        *(
            f"[Tach] > Skipped '{test_path}'"
            for test_path in handler.removed_test_paths
        ),
    ]


def pytest_terminal_summary(terminalreporter: Any, exitstatus: int, config: TachConfig):
    config.tach_handler.tests_ran_to_completion = True
