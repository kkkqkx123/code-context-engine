import json
import logging
from dataclasses import dataclass, field, asdict
from datetime import datetime
from pathlib import Path
from typing import Any, Dict, List, Optional

from collect_metrics import MetricSnapshot

logger = logging.getLogger(__name__)


@dataclass
class TestCaseResult:
    """Record of a single functional test case execution."""
    name: str
    passed: bool
    checks: List[tuple] = field(default_factory=list)
    data: Dict[str, Any] = field(default_factory=dict)
    elapsed_ms: Optional[float] = None

    def to_dict(self) -> Dict[str, Any]:
        return {
            "name": self.name,
            "passed": self.passed,
            "checks": [
                {"name": c[0], "passed": c[1], "detail": c[2] if len(c) > 2 else ""}
                for c in self.checks
            ],
            "elapsed_ms": self.elapsed_ms,
        }


@dataclass
class BenchmarkResult:
    """Record of a benchmark run with timing and metric snapshots."""
    iteration: int
    index_elapsed_ms: Optional[float] = None
    search_elapsed_ms: Optional[float] = None
    search_results_count: Optional[int] = None
    metrics_before: Optional[MetricSnapshot] = None
    metrics_after: Optional[MetricSnapshot] = None

    def to_dict(self) -> Dict[str, Any]:
        return {
            "iteration": self.iteration,
            "index_elapsed_ms": self.index_elapsed_ms,
            "search_elapsed_ms": self.search_elapsed_ms,
            "search_results_count": self.search_results_count,
            "metrics_before": asdict(self.metrics_before) if self.metrics_before else None,
            "metrics_after": asdict(self.metrics_after) if self.metrics_after else None,
        }


class ResultsAnalyzer:
    """Analyzes test and benchmark results, generates structured reports.

    Directory structure (all scoped by project_name):
      results/
        raw/                          -- raw metric snapshots (per project)
          {project_name}/
            metric_before_0000.json
            metric_after_0000.json
            ...
        processed/                    -- aggregated/processed data (per project)
          {project_name}/
            test_results.json         -- all functional test outcomes
            benchmark_results.json    -- all benchmark iteration records
        reports/                      -- summary reports (per project)
          {project_name}/
            final_report.json         -- combined top-level report
    """

    def __init__(self, results_dir: Path, project_name: str = "once_cell"):
        self.results_dir = results_dir.resolve()
        self.project_name = project_name

    def _raw_dir(self) -> Path:
        return self.results_dir / "raw" / self.project_name

    def _processed_dir(self) -> Path:
        return self.results_dir / "processed" / self.project_name

    def _reports_dir(self) -> Path:
        return self.results_dir / "reports" / self.project_name

    # ------------------------------------------------------------------
    # Functional test analysis
    # ------------------------------------------------------------------

    def save_functional_results(
        self,
        test_results: List[Dict[str, Any]],
    ) -> Path:
        proc_dir = self._processed_dir()
        proc_dir.mkdir(parents=True, exist_ok=True)

        total = len(test_results)
        passed = sum(1 for r in test_results if r.get("passed", False))
        failed_tests = [r for r in test_results if not r.get("passed", False)]

        report = {
            "test_suite": f"{self.project_name} functional tests",
            "project": self.project_name,
            "timestamp": datetime.utcnow().isoformat() + "Z",
            "summary": {
                "total": total,
                "passed": passed,
                "failed": total - passed,
                "pass_rate": round(passed / total * 100, 1) if total > 0 else 0,
            },
            "results": test_results,
            "failed_tests": [
                {
                    "name": r["name"],
                    "checks": [
                        {"name": c[0], "passed": c[1], "detail": c[2] if len(c) > 2 else ""}
                        for c in r.get("checks", [])
                    ],
                }
                for r in failed_tests
            ],
        }

        results_path = proc_dir / "test_results.json"
        results_path.write_text(
            json.dumps(report, indent=2, ensure_ascii=False),
            encoding="utf-8",
        )
        logger.info(
            "Functional results saved: %d/%d passed (%.1f%%) -> %s",
            passed, total, report["summary"]["pass_rate"], results_path,
        )
        return results_path

    # ------------------------------------------------------------------
    # Benchmark analysis
    # ------------------------------------------------------------------

    def save_benchmark_results(
        self,
        benchmark_records: List[Dict[str, Any]],
    ) -> Path:
        proc_dir = self._processed_dir()
        proc_dir.mkdir(parents=True, exist_ok=True)

        index_timings = [
            r.get("index_elapsed_ms")
            for r in benchmark_records
            if r.get("index_elapsed_ms") is not None
        ]
        search_timings = [
            r.get("search_elapsed_ms")
            for r in benchmark_records
            if r.get("search_elapsed_ms") is not None
        ]

        stats = {}
        if index_timings:
            stats["index_elapsed_ms"] = _compute_stats(index_timings)
        if search_timings:
            stats["search_elapsed_ms"] = _compute_stats(search_timings)

        memory_before = []
        memory_after = []
        for r in benchmark_records:
            mb = r.get("metrics_before") or {}
            ma = r.get("metrics_after") or {}
            if mb.get("memory_resident_bytes"):
                memory_before.append(mb["memory_resident_bytes"])
            if ma.get("memory_resident_bytes"):
                memory_after.append(ma["memory_resident_bytes"])

        if memory_before:
            stats["memory_resident_before_bytes"] = _compute_stats(memory_before)
        if memory_after:
            stats["memory_resident_after_bytes"] = _compute_stats(memory_after)

        vector_before = []
        vector_after = []
        for r in benchmark_records:
            mb = r.get("metrics_before") or {}
            ma = r.get("metrics_after") or {}
            if mb.get("vector_count"):
                vector_before.append(mb["vector_count"])
            if ma.get("vector_count"):
                vector_after.append(ma["vector_count"])

        if vector_before:
            stats["vector_count_before"] = _compute_stats(vector_before)
        if vector_after:
            stats["vector_count_after"] = _compute_stats(vector_after)

        report = {
            "benchmark_suite": f"{self.project_name} performance benchmark",
            "project": self.project_name,
            "timestamp": datetime.utcnow().isoformat() + "Z",
            "iterations": len(benchmark_records),
            "statistics": stats,
            "records": benchmark_records,
        }

        results_path = proc_dir / "benchmark_results.json"
        results_path.write_text(
            json.dumps(report, indent=2, ensure_ascii=False),
            encoding="utf-8",
        )
        logger.info("Benchmark results saved to %s", results_path)
        return results_path

    # ------------------------------------------------------------------
    # Combined final report
    # ------------------------------------------------------------------

    def save_final_report(
        self,
        functional_path: Path,
        benchmark_path: Optional[Path] = None,
    ):
        func_data = json.loads(functional_path.read_text(encoding="utf-8"))
        bench_data = None
        if benchmark_path and benchmark_path.is_file():
            bench_data = json.loads(benchmark_path.read_text(encoding="utf-8"))

        report = {
            "report_title": f"CCE End-to-End Test Report — {self.project_name}",
            "project": self.project_name,
            "generated_at": datetime.utcnow().isoformat() + "Z",
            "functional": {
                "summary": func_data["summary"],
                "results_path": str(functional_path),
            },
        }

        if bench_data:
            report["benchmark"] = {
                "iterations": bench_data["iterations"],
                "statistics": bench_data["statistics"],
                "results_path": str(benchmark_path),
            }

        report_dir = self._reports_dir()
        report_dir.mkdir(parents=True, exist_ok=True)
        report_path = report_dir / "final_report.json"
        report_path.write_text(
            json.dumps(report, indent=2, ensure_ascii=False),
            encoding="utf-8",
        )
        logger.info("Final report saved to %s", report_path)


def _compute_stats(values: List[float]) -> Dict[str, float]:
    n = len(values)
    if n == 0:
        return {}
    sorted_v = sorted(values)
    mean = sum(sorted_v) / n
    variance = sum((v - mean) ** 2 for v in sorted_v) / n
    return {
        "count": n,
        "min": round(sorted_v[0], 2),
        "max": round(sorted_v[-1], 2),
        "mean": round(mean, 2),
        "median": round(_percentile(sorted_v, 50), 2),
        "p95": round(_percentile(sorted_v, 95), 2),
        "p99": round(_percentile(sorted_v, 99), 2),
        "stddev": round(variance ** 0.5, 2),
    }


def _percentile(sorted_data: List[float], p: float) -> float:
    if not sorted_data:
        return 0.0
    k = (len(sorted_data) - 1) * p / 100.0
    f = int(k)
    c = f + 1
    if c >= len(sorted_data):
        return sorted_data[-1]
    return sorted_data[f] + (k - f) * (sorted_data[c] - sorted_data[f])