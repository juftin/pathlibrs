"""
Test Fixtures
"""

from pathlib import Path

import pytest
from pathlibrs import Path as PathRS


@pytest.fixture
def test_path() -> Path:
    """
    Return a path for the test directory
    """
    return Path(__file__).resolve().parent


@pytest.fixture
def project_path(test_path: Path) -> Path:
    """
    Return a string for the project directory
    """
    return test_path.parent


@pytest.fixture
def readme_path(project_path: Path) -> Path:
    """
    Return a path for the README file
    """
    return project_path / "README.md"


@pytest.fixture
def string_test_file() -> str:
    """
    Return a string for a test file
    """
    return "test.txt"


@pytest.fixture
def pathlib_test_file(string_test_file: str) -> Path:
    """
    Return a Path object for a test file
    """
    return Path(string_test_file)


@pytest.fixture
def rs_test_file(string_test_file: str) -> PathRS:
    """
    Return a Path object for a test file
    """
    return PathRS(string_test_file)
