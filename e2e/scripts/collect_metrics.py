import json
import time
import logging
from dataclasses import dataclass, field, asdict
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional

import requests

from test_runner import CceApiClient

logger = logging.getLogger(__name__)

# Metrics that must be present in a healthy /api/metrics/json snapshot.
# Used to detect server/benchmark contract drift early: missing entries are
# warned about instead of being silently recorded as None.
REQUIRED_SCALARS = [
    "scanner_files_scanned_total",
    "files_processed_total",
    "files_failed_total",
    "relations_extracted_total",
    "search_queries_total",
    "query_executions_total",
    "rerank_requests_total",
    "process_rss_bytes",
    "process_open_fds",
    "qdrant_collection_size",
]

REQUIRED_HISTOGRAMS = [
    "search_query_latency_ms",
    "rerank_latency_ms",
]


@dataclass
class MetricSnapshot:
    """A single point-in-time snapshot of server metrics.

    Parsed from the CCE /api/metrics/json response, whose shape is::

        {"timestamp": ..., "metrics": [{"name", "labels", "value"}], "summary"}

    where ``value`` is ``{"type": "Counter"|"Gauge"|"FloatGauge"|"Histogram",
    "value": ...}``. Unknown/missing metrics are safely handled.
    """
    timestamp: float = 0.0
    timestamp_iso: str = ""

    files_scanned_total: Optional[int] = None
    files_processed_total: Optional[int] = None
    files_failed_total: Optional[int] = None
    relations_extracted_total: Optional[int] = None

    search_latency_p50_ms: Optional[float] = None
    search_latency_p95_ms: Optional[float] = None
    search_latency_p99_ms: Optional[float] = None
    search_total: Optional[int] = None
    query_executions_total: Optional[int] = None

    rerank_latency_p50_ms: Optional[float] = None
    rerank_latency_p95_ms: Optional[float] = None
    rerank_total: Optional[int] = None

    process_rss_bytes: Optional[int] = None
    process_open_fds: Optional[int] = None

    vector_count: Optional[int] = None

    missing_metrics: List[str] = field(default_factory=list)

    @classmethod
    def from_metrics_response(cls, metrics: Dict[str, Any]) -> "MetricSnapshot":
        now = time.time()
        snapshot = cls(
            timestamp=now,
            timestamp_iso=datetime.fromtimestamp(now, tz=timezone.utc).isoformat(),
        )

        series = metrics.get("metrics")
        if not isinstance(series, list):
            logger.warning(
                "Metric snapshot has no 'metrics' list (got %s); "
                "all fields will be None and reported as missing",
                type(series).__name__,
            )
            series = []

        snapshot.files_scanned_total = _scalar_int(series, "scanner_files_scanned_total")
        snapshot.files_processed_total = _scalar_int(series, "files_processed_total")
        snapshot.files_failed_total = _scalar_int(series, "files_failed_total")
        snapshot.relations_extracted_total = _scalar_int(series, "relations_extracted_total")

        search_latency = _histogram(series, "search_query_latency_ms")
        snapshot.search_latency_p50_ms = _safe_float(search_latency, "p50")
        snapshot.search_latency_p95_ms = _safe_float(search_latency, "p95")
        snapshot.search_latency_p99_ms = _safe_float(search_latency, "p99")
        snapshot.search_total = _scalar_int(series, "search_queries_total")
        snapshot.query_executions_total = _scalar_int(series, "query_executions_total")

        rerank_latency = _histogram(series, "rerank_latency_ms")
        snapshot.rerank_latency_p50_ms = _safe_float(rerank_latency, "p50")
        snapshot.rerank_latency_p95_ms = _safe_float(rerank_latency, "p95")
        snapshot.rerank_total = _scalar_int(series, "rerank_requests_total")

        snapshot.process_rss_bytes = _scalar_int(series, "process_rss_bytes")
        snapshot.process_open_fds = _scalar_int(series, "process_open_fds")

        snapshot.vector_count = _scalar_int(series, "qdrant_collection_size")

        snapshot.missing_metrics = snapshot.validate(series)
        if snapshot.missing_metrics:
            logger.warning(
                "Metric snapshot missing %d expected metrics: %s",
                len(snapshot.missing_metrics),
                ", ".join(snapshot.missing_metrics),
            )

        return snapshot

    def validate(self, series: list) -> List[str]:
        """Return names of required metrics absent from the snapshot series."""
        missing = []
        for name in REQUIRED_SCALARS:
            if _scalar_int(series, name) is None:
                missing.append(name)
        for name in REQUIRED_HISTOGRAMS:
            if not _histogram(series, name):
                missing.append(name)
        return missing

    def to_json(self) -> str:
        return json.dumps(asdict(self), indent=2, ensure_ascii=False)


def _find_series(series: list, name: str) -> Optional[dict]:
    """Return the ``value`` payload of the metric with the given name, if any."""
    for entry in series:
        if isinstance(entry, dict) and entry.get("name") == name:
            value = entry.get("value")
            return value if isinstance(value, dict) else None
    return None


def _scalar_int(series: list, name: str) -> Optional[int]:
    value = _find_series(series, name)
    if value is not None:
        return _safe_int(value, "value")
    return None


def _histogram(series: list, name: str) -> dict:
    """Return the HistogramStats dict for the metric, or {} when absent."""
    value = _find_series(series, name)
    if value is not None and value.get("type") == "Histogram":
        stats = value.get("value")
        return stats if isinstance(stats, dict) else {}
    return {}


def _safe_float(d: dict, key: str) -> Optional[float]:
    v = d.get(key)
    if v is not None:
        try:
            return float(v)
        except (TypeError, ValueError):
            return None
    return None


def _safe_int(d: dict, key: str) -> Optional[int]:
    v = d.get(key)
    if v is not None:
        try:
            return int(v)
        except (TypeError, ValueError):
            return None
    return None


class MetricsCollector:
    """Periodic collector of CCE server performance metrics.

    Usage:
        collector = MetricsCollector(client)
        snapshot = collector.snapshot()
        collector.save(snapshot, Path("results/raw/metric_001.json"))

    Batch collection:
        snapshots = collector.collect_batch(interval_sec=5, count=6)
    """

    def __init__(self, client: CceApiClient):
        self.client = client

    def snapshot(self) -> MetricSnapshot:
        raw = self.client.get_metrics_json()
        return MetricSnapshot.from_metrics_response(raw)

    def collect_batch(
        self,
        interval_sec: float = 5.0,
        count: int = 1,
    ) -> List[MetricSnapshot]:
        snapshots = []
        for i in range(count):
            snapshots.append(self.snapshot())
            if i < count - 1:
                time.sleep(interval_sec)
        return snapshots

    @staticmethod
    def save(snapshot: MetricSnapshot, path: Path):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(snapshot.to_json(), encoding="utf-8")
        logger.info("Saved metric snapshot to %s", path)

    @staticmethod
    def save_batch(snapshots: List[MetricSnapshot], output_dir: Path):
        output_dir.mkdir(parents=True, exist_ok=True)
        # Save individual files
        for i, snap in enumerate(snapshots):
            path = output_dir / f"metric_{i:04d}.json"
            path.write_text(snap.to_json(), encoding="utf-8")

        # Save aggregate summary
        summary = {
            "count": len(snapshots),
            "time_range": {
                "start": snapshots[0].timestamp_iso if snapshots else None,
                "end": snapshots[-1].timestamp_iso if snapshots else None,
            },
            "fields": list(asdict(snapshots[0]).keys()) if snapshots else [],
        }
        summary_path = output_dir / "_summary.json"
        summary_path.write_text(
            json.dumps(summary, indent=2, ensure_ascii=False),
            encoding="utf-8",
        )
        logger.info(
            "Saved %d metric snapshots to %s",
            len(snapshots),
            output_dir,
        )