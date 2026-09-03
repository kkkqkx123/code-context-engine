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


@dataclass
class MetricSnapshot:
    """A single point-in-time snapshot of server metrics.

    Fields are designed to align with the CCE /api/metrics/json response.
    Unknown/missing fields are safely handled.
    """
    timestamp: float = 0.0
    timestamp_iso: str = ""

    indexing_duration_ms: Optional[float] = None
    files_scanned_total: Optional[int] = None
    files_indexed_total: Optional[int] = None
    entities_extracted_total: Optional[int] = None
    relations_extracted_total: Optional[int] = None
    indexing_errors_total: Optional[int] = None

    search_latency_p50_ms: Optional[float] = None
    search_latency_p95_ms: Optional[float] = None
    search_latency_p99_ms: Optional[float] = None
    search_total: Optional[int] = None

    rerank_latency_p50_ms: Optional[float] = None
    rerank_latency_p95_ms: Optional[float] = None
    rerank_total: Optional[int] = None

    memory_resident_bytes: Optional[int] = None
    memory_allocated_bytes: Optional[int] = None

    vector_count: Optional[int] = None
    bm25_doc_count: Optional[int] = None

    @classmethod
    def from_metrics_response(cls, metrics: Dict[str, Any]) -> "MetricSnapshot":
        now = time.time()
        snapshot = cls(
            timestamp=now,
            timestamp_iso=datetime.fromtimestamp(now, tz=timezone.utc).isoformat(),
        )

        snapshot.indexing_duration_ms = _safe_float(metrics, "indexing_duration_ms")
        snapshot.files_scanned_total = _safe_int(metrics, "files_scanned_total")
        snapshot.files_indexed_total = _safe_int(metrics, "files_indexed_total")
        snapshot.entities_extracted_total = _safe_int(metrics, "entities_extracted_total")
        snapshot.relations_extracted_total = _safe_int(metrics, "relations_extracted_total")
        snapshot.indexing_errors_total = _safe_int(metrics, "indexing_errors_total")

        search_latency = metrics.get("search_latency_ms", {})
        if isinstance(search_latency, dict):
            snapshot.search_latency_p50_ms = _safe_float(search_latency, "p50")
            snapshot.search_latency_p95_ms = _safe_float(search_latency, "p95")
            snapshot.search_latency_p99_ms = _safe_float(search_latency, "p99")
        snapshot.search_total = _safe_int(metrics, "search_total")

        rerank_latency = metrics.get("rerank_latency_ms", {})
        if isinstance(rerank_latency, dict):
            snapshot.rerank_latency_p50_ms = _safe_float(rerank_latency, "p50")
            snapshot.rerank_latency_p95_ms = _safe_float(rerank_latency, "p95")
        snapshot.rerank_total = _safe_int(metrics, "rerank_total")

        snapshot.memory_resident_bytes = _safe_int(metrics, "memory_resident_bytes")
        snapshot.memory_allocated_bytes = _safe_int(metrics, "memory_allocated_bytes")

        snapshot.vector_count = _safe_int(metrics, "vector_count")
        snapshot.bm25_doc_count = _safe_int(metrics, "bm25_doc_count")

        return snapshot

    def to_json(self) -> str:
        return json.dumps(asdict(self), indent=2, ensure_ascii=False)


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