#!/usr/bin/env python3
"""
CCE End-to-End Test & Benchmark Runner — once_cell Fixture

Usage:
    # Run functional tests only
    python benches/scripts/main.py functional

    # Run performance benchmarks (default: 5 iterations)
    python benches/scripts/main.py benchmark --iterations 5

    # Run both
    python benches/scripts/main.py all --iterations 3

    # Skip service management (connect to already-running services)
    python benches/scripts/main.py functional --no-service-mgmt

    # Run functional tests with a specific server URL
    python benches/scripts/main.py functional --server-url http://localhost:9001

Environment:
    CCE_LLM_API_KEY_SILICONFLOW    Required for SiliconFlow LLM API
    QDRANT_PATH                     Optional: path to Qdrant executable
"""

import argparse
import json
import logging
import os
import sys
import time
from pathlib import Path

from service_manager import ServiceManager
from test_runner import CceApiClient, TestRunner
from collect_metrics import MetricsCollector
from analyze_results import ResultsAnalyzer, BenchmarkResult

logger = logging.getLogger(__name__)

BASE_DIR = Path(__file__).resolve().parent.parent
RESULTS_DIR = BASE_DIR / "results"
SCRIPTS_DIR = BASE_DIR / "scripts"

CCE_BIN = BASE_DIR / "bin" / "cce.exe"
CONFIG_PATH = BASE_DIR / "config.toml"
DEFAULT_SERVER_URL = "http://127.0.0.1:9001"
DEFAULT_PROJECT = "once_cell"

# Persistent data directories that need cleanup between E2E runs
#
# IMPORTANT: The .env file (benches/bin/.env) overrides the config file's
# database path via CCE_DB_SQLITE_PATH=./data/cce.db. The cleanup must use
# the effective path, which is what the CCE binary actually creates.
SQLITE_DB_PATH = Path("data/cce.db")
QDRANT_DATA_DIR = BASE_DIR / "data" / "qdrant"


def _cleanup_databases():
    """Remove persistent data from previous runs to ensure a clean state.

    This is critical for E2E testing because:
    - The SQLite database persists project records across runs
    - Qdrant data directory holds vector index state
    - Without cleanup, tests fail with 'Project already exists at this path'

    The effective SQLite path is determined by .env (CCE_DB_SQLITE_PATH),
    which overrides the config file setting. See benches/bin/.env.
    """
    import shutil

    db_path = SQLITE_DB_PATH
    if db_path.exists():
        db_path.unlink()
        logger.info("Cleaned up SQLite database: %s", db_path)
    # Also clean WAL and SHM files
    for ext in ["-wal", "-shm"]:
        wal_path = db_path.parent / (db_path.name + ext)
        if wal_path.exists():
            wal_path.unlink()

    qdrant_dir = QDRANT_DATA_DIR
    if qdrant_dir.exists():
        shutil.rmtree(qdrant_dir)
        logger.info("Cleaned up Qdrant data directory: %s", qdrant_dir)


def setup_logging(verbose: bool = False):
    level = logging.DEBUG if verbose else logging.INFO
    logging.basicConfig(
        level=level,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
        datefmt="%H:%M:%S",
        stream=sys.stdout,
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="CCE End-to-End Test & Benchmark Runner",
    )
    parser.add_argument(
        "mode",
        choices=["functional", "benchmark", "all"],
        help="Test mode: functional, benchmark, or all",
    )
    parser.add_argument(
        "--project",
        default=DEFAULT_PROJECT,
        help=f"Target project name for result scoping (default: {DEFAULT_PROJECT})",
    )
    parser.add_argument(
        "--iterations",
        type=int,
        default=5,
        help="Number of benchmark iterations (default: 5)",
    )
    parser.add_argument(
        "--server-url",
        default=DEFAULT_SERVER_URL,
        help=f"CCE server URL (default: {DEFAULT_SERVER_URL})",
    )
    parser.add_argument(
        "--no-service-mgmt",
        action="store_true",
        help="Skip starting/stopping services (connect to already-running)",
    )
    parser.add_argument(
        "--verbose", "-v",
        action="store_true",
        help="Enable debug logging",
    )
    return parser.parse_args()


def run_functional_tests(
    client: CceApiClient,
    analyzer: ResultsAnalyzer,
    project_name: str,
) -> bool:
    logger.info("=" * 60)
    logger.info("Starting functional tests against %s fixture", project_name)
    logger.info("=" * 60)

    runner = TestRunner(client)

    start = time.time()
    test_results = runner.run_all_functional_tests()
    elapsed = time.time() - start

    passed = sum(1 for r in test_results if r.get("passed", False))
    total = len(test_results)

    logger.info("-" * 60)
    logger.info("Functional tests complete: %d/%d passed (%.1f%%) in %.2fs",
                passed, total, passed / total * 100 if total else 0, elapsed)
    for r in test_results:
        status = "PASS" if r.get("passed", False) else "FAIL"
        logger.info("  [%s] %s", status, r["name"])
    logger.info("-" * 60)

    # Save results
    func_path = analyzer.save_functional_results(test_results)
    logger.info("Detailed results saved to %s", func_path)

    return passed == total


def run_benchmarks(
    client: CceApiClient,
    collector: MetricsCollector,
    analyzer: ResultsAnalyzer,
    project_name: str,
    iterations: int = 5,
):
    from test_runner import TestRunner

    logger.info("=" * 60)
    logger.info("Starting performance benchmarks (%d iterations) for %s", iterations, project_name)
    logger.info("=" * 60)

    runner = TestRunner(client)
    records = []

    for i in range(iterations):
        logger.info("--- Benchmark iteration %d/%d ---", i + 1, iterations)

        metrics_before = collector.snapshot()
        project_id = runner.setup_once_cell_project()

        idx_start = time.time()
        index_result = runner.test_full_index(project_id)
        idx_elapsed = (time.time() - idx_start) * 1000

        search_start = time.time()
        search_result = runner.test_hybrid_search(project_id)
        search_elapsed = (time.time() - search_start) * 1000

        metrics_after = collector.snapshot()

        record = BenchmarkResult(
            iteration=i + 1,
            index_elapsed_ms=index_result.get("elapsed_ms", idx_elapsed),
            search_elapsed_ms=search_result.get("elapsed_ms", search_elapsed),
            search_results_count=len(search_result.get("items", [])),
            metrics_before=metrics_before,
            metrics_after=metrics_after,
        )
        records.append(record)

        logger.info(
            "  Iter %d: index=%dms search=%dms results=%d",
            i + 1,
            record.index_elapsed_ms or 0,
            record.search_elapsed_ms or 0,
            record.search_results_count or 0,
        )

        # Save raw metrics to results/raw/{project_name}/
        raw_dir = analyzer._raw_dir()
        raw_dir.mkdir(parents=True, exist_ok=True)
        collector.save(metrics_before, raw_dir / f"metric_before_{i:04d}.json")
        collector.save(metrics_after, raw_dir / f"metric_after_{i:04d}.json")

        try:
            client.clear_index()
        except Exception as e:
            logger.warning("Failed to clear index (iteration %d): %s", i + 1, e)

    record_dicts = [r.to_dict() for r in records]
    bench_path = analyzer.save_benchmark_results(record_dicts)

    index_timings = [r.index_elapsed_ms for r in records if r.index_elapsed_ms]
    search_timings = [r.search_elapsed_ms for r in records if r.search_elapsed_ms]
    if index_timings:
        logger.info("Index timing: min=%.0fms max=%.0fms avg=%.0fms",
                    min(index_timings), max(index_timings),
                    sum(index_timings) / len(index_timings))
    if search_timings:
        logger.info("Search timing: min=%.0fms max=%.0fms avg=%.0fms",
                    min(search_timings), max(search_timings),
                    sum(search_timings) / len(search_timings))

    return bench_path


def main():
    args = parse_args()
    setup_logging(args.verbose)

    # Clean up persistent data from previous runs
    _cleanup_databases()

    project_name = args.project
    results_dir = RESULTS_DIR

    # Validate API key
    if not os.environ.get("CCE_LLM_API_KEY_SILICONFLOW"):
        logger.warning(
            "CCE_LLM_API_KEY_SILICONFLOW not set. "
            "LLM API calls may fail."
        )

    # Service management — Qdrant is internally managed by the Rust process
    manager = ServiceManager(
        cce_bin=CCE_BIN,
        config_path=CONFIG_PATH,
    )

    if not args.no_service_mgmt:
        logger.info("Starting CCE server...")
        if not manager.start_cce_server():
            logger.error("Failed to start CCE server. Aborting.")
            sys.exit(1)
    else:
        logger.info("Service management disabled. Using already-running services.")

    # Initialize clients
    client = CceApiClient(base_url=args.server_url)
    collector = MetricsCollector(client)
    analyzer = ResultsAnalyzer(results_dir, project_name=project_name)

    # Verify server is responding
    if not client.health_check():
        logger.error("CCE server not reachable at %s. Aborting.", args.server_url)
        manager.cleanup()
        sys.exit(1)
    logger.info("CCE server reachable at %s", args.server_url)

    func_ok = True
    bench_path = None

    try:
        if args.mode in ("functional", "all"):
            func_ok = run_functional_tests(client, analyzer, project_name)

        if args.mode in ("benchmark", "all"):
            bench_path = run_benchmarks(
                client, collector, analyzer, project_name,
                iterations=args.iterations,
            )

        # Generate combined final report
        func_path = analyzer._processed_dir() / "test_results.json"
        if func_path.exists():
            analyzer.save_final_report(func_path, bench_path)

        logger.info("=" * 60)
        logger.info("Project: %s", project_name)
        if args.mode in ("functional", "all"):
            logger.info("Functional tests: %s", "ALL PASSED" if func_ok else "SOME FAILED")
        if args.mode in ("benchmark", "all"):
            logger.info("Benchmark results saved to %s", bench_path)
        logger.info("Results directory: %s", results_dir / "reports" / project_name)
        logger.info("=" * 60)

    except Exception as e:
        logger.exception("Test run failed: %s", e)
        sys.exit(1)
    finally:
        if not args.no_service_mgmt:
            manager.cleanup()

    if not func_ok:
        sys.exit(1)


if __name__ == "__main__":
    main()