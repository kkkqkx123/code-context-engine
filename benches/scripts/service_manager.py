import os
import sys
import time
import signal
import subprocess
import logging
import socket
from pathlib import Path
from typing import Optional

logger = logging.getLogger(__name__)


class ServiceManager:
    """Manages the CCE server process lifecycle.

    Responsibilities:
    - Background startup of CCE server (port 9001 per benches/config.toml)
    - Health check via HTTP port polling
    - Graceful shutdown on exit

    Qdrant is now managed internally by the CCE Rust process (auto_start),
    so this class no longer handles Qdrant lifecycle.
    """

    CCE_PORT = 9001
    CCE_HEALTH_ENDPOINT = "/api/health"

    def __init__(self, cce_bin: str = "bin/cce.exe", config_path: str = "benches/config.toml"):
        self.cce_bin = Path(cce_bin)
        self.config_path = Path(config_path)
        self.cce_proc: Optional[subprocess.Popen] = None
        self._started_services = []

    # ------------------------------------------------------------------
    # CCE server management
    # ------------------------------------------------------------------

    def start_cce_server(self) -> bool:
        if not self.cce_bin.is_file():
            logger.error("CCE server binary not found: %s", self.cce_bin)
            logger.error("Build first: cargo build --release --bin cce")
            logger.error("Then copy to: %s", self.cce_bin)
            return False

        if not self.config_path.is_file():
            logger.error("Config file not found: %s", self.config_path)
            return False

        if self._is_port_open(self.CCE_PORT):
            logger.info("CCE server already running on port %d", self.CCE_PORT)
            # Check Qdrant readiness even if server is already running
            if self._wait_for_qdrant_health(timeout=120):
                return True
            return False

        logger.info("Starting CCE server from: %s", self.cce_bin)
        logger.info("Using config: %s", self.config_path)

        env = os.environ.copy()
        env["CCE_CONFIG"] = str(self.config_path)

        # Write server logs to a file to avoid pipe buffer deadlock
        cce_log_path = self.cce_bin.parent / "cce_server.log"

        startupinfo = None
        if sys.platform == "win32":
            startupinfo = subprocess.STARTUPINFO()
            startupinfo.dwFlags |= subprocess.STARTF_USESHOWWINDOW

        try:
            self.cce_proc = subprocess.Popen(
                [str(self.cce_bin)],
                stdout=open(cce_log_path, "w", encoding="utf-8"),
                stderr=subprocess.STDOUT,
                env=env,
                startupinfo=startupinfo,
                creationflags=subprocess.CREATE_NO_WINDOW if sys.platform == "win32" else 0,
            )
            self._started_services.append("cce")

            # Phase 1: Wait for the HTTP server to start accepting connections
            if not self._wait_for_health(self.CCE_PORT, self.CCE_HEALTH_ENDPOINT, timeout=120):
                logger.error("CCE server failed to start within timeout")
                self.stop_cce_server()
                return False

            # Phase 2: Wait for Qdrant to become healthy (the server manages Qdrant
            # as a subprocess, which takes extra time to start up)
            logger.info("CCE HTTP server is up, waiting for Qdrant to become ready...")
            if not self._wait_for_qdrant_health(timeout=120):
                logger.error("Qdrant failed to become healthy within timeout")
                self.stop_cce_server()
                return False

            logger.info("CCE server started successfully on port %d", self.CCE_PORT)
            return True
        except FileNotFoundError:
            logger.error("CCE server binary not found: %s", self.cce_bin)
            return False

    def stop_cce_server(self):
        if self.cce_proc:
            logger.info("Stopping CCE server (PID %d)", self.cce_proc.pid)
            self._terminate_process(self.cce_proc)
            self.cce_proc = None

    # ------------------------------------------------------------------
    # Unified cleanup
    # ------------------------------------------------------------------

    def cleanup(self):
        logger.info("Cleaning up all services...")
        self.stop_cce_server()
        logger.info("Cleanup complete")

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        self.cleanup()

    # ------------------------------------------------------------------
    # Health check helpers
    # ------------------------------------------------------------------

    @staticmethod
    def _is_port_open(port: int, host: str = "127.0.0.1") -> bool:
        try:
            with socket.create_connection((host, port), timeout=2):
                return True
        except (socket.timeout, ConnectionRefusedError, OSError):
            return False

    @staticmethod
    def _wait_for_health(port: int, endpoint: str, host: str = "127.0.0.1", timeout: int = 30) -> bool:
        import urllib.request
        import urllib.error

        deadline = time.time() + timeout
        url = f"http://{host}:{port}{endpoint}"
        while time.time() < deadline:
            try:
                resp = urllib.request.urlopen(url, timeout=5)
                if resp.status == 200:
                    return True
            except (urllib.error.URLError, urllib.error.HTTPError, ConnectionResetError, OSError):
                pass
            time.sleep(1)
        return False

    @staticmethod
    def _wait_for_qdrant_health(host: str = "127.0.0.1", port: int = 9001, timeout: int = 120) -> bool:
        """Wait for the Qdrant service to report healthy via the CCE API.

        The CCE server manages Qdrant as a subprocess. Even after the HTTP
        server starts, Qdrant may still be starting up. This method polls
        the /api/health endpoint and checks the 'qdrant.reachable' field.
        """
        import json
        import urllib.request
        import urllib.error

        deadline = time.time() + timeout
        url = f"http://{host}:{port}/api/health"
        while time.time() < deadline:
            try:
                resp = urllib.request.urlopen(url, timeout=5)
                if resp.status == 200:
                    body = json.loads(resp.read().decode("utf-8"))
                    if body.get("qdrant", {}).get("reachable", False):
                        return True
                    logger.debug("Qdrant not ready yet, waiting...")
                else:
                    logger.debug("Health endpoint returned %d, retrying...", resp.status)
            except (urllib.error.URLError, urllib.error.HTTPError, ConnectionResetError, OSError, json.JSONDecodeError) as e:
                logger.debug("Qdrant health check failed: %s, retrying...", e)
            time.sleep(2)
        return False

    @staticmethod
    def _terminate_process(proc: subprocess.Popen):
        if proc.poll() is not None:
            return
        try:
            if sys.platform == "win32":
                proc.terminate()
            else:
                os.kill(proc.pid, signal.SIGTERM)
            try:
                proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                logger.warning("Process PID %d did not exit, killing forcefully", proc.pid)
                proc.kill()
                proc.wait(timeout=5)
        except (ProcessLookupError, OSError):
            pass

    @staticmethod
    def _log_subprocess_output(proc: subprocess.Popen):
        try:
            stdout, stderr = proc.communicate(timeout=3)
            if stdout:
                for line in stdout.decode("utf-8", errors="replace").splitlines():
                    logger.info("  [stdout] %s", line)
            if stderr:
                for line in stderr.decode("utf-8", errors="replace").splitlines():
                    logger.warning("  [stderr] %s", line)
        except subprocess.TimeoutExpired:
            pass