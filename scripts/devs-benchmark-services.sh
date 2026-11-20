#!/usr/bin/env bash
# On-demand external services for CCE benchmarks on devs.
#
# Usage: bash scripts/devs-benchmark-services.sh {up|down|status|clean} [qdrant|elasticsearch|all]
#
# Services are started only for a benchmark run and stopped by PID file. The
# downloaded archives stay cached; each run gets its own data directory.
set -u

ROOT=/data/kkkqkx/cce/services
CACHE="$ROOT/cache"
LOG_ROOT=/data/kkkqkx/cce/service-logs
mkdir -p "$CACHE" "$LOG_ROOT"

QDRANT_VERSION=${QDRANT_VERSION:-1.15.5}
ES_VERSION=${ES_VERSION:-7.17.28}
QDRANT_URL=${QDRANT_URL:-http://127.0.0.1:16333}
ES_URL=${ES_URL:-http://127.0.0.1:19200}
RUN_ID=${RUN_ID:-$(date +%s)}

log() { printf '[%s] %s\n' "$(date -u +%FT%TZ)" "$*"; }

qdrant_dir() { echo "$ROOT/qdrant"; }
es_dir() { echo "$ROOT/elasticsearch"; }

download() {
    local url=$1 path=$2
    if [[ -f "$path" ]]; then
        log "cache hit: $path ($(du -h "$path" | cut -f1))"
        return 0
    fi
    log "downloading $url"
    curl -fL --retry 3 --continue-at - -o "$path" "$url"
}

wait_http() {
    local name=$1 url=$2 timeout=$3
    local deadline=$((SECONDS + timeout))
    while (( SECONDS < deadline )); do
        local code
        code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 3 "$url" || true)
        if [[ "$code" =~ ^2 ]]; then
            log "$name healthy: $url ($code)"
            return 0
        fi
        sleep 2
    done
    log "$name failed health check: $url"
    return 1
}

start_qdrant() {
    local dir base archive
    dir=$(qdrant_dir); base="qdrant-$QDRANT_VERSION"
    archive="$CACHE/qdrant-x86_64-unknown-linux-musl-$QDRANT_VERSION.tar.gz"
    download "https://github.com/qdrant/qdrant/releases/download/v$QDRANT_VERSION/qdrant-x86_64-unknown-linux-musl.tar.gz" "$archive"
    mkdir -p "$dir/bin" "$dir/data/$RUN_ID" "$dir/logs" "$dir/run"
    if [[ ! -x "$dir/bin/qdrant" ]]; then
        tar -xzf "$archive" -C "$dir/bin"
        chmod +x "$dir/bin/qdrant"
    fi
    if [[ -f "$dir/run/qdrant.pid" ]] && kill -0 "$(cat "$dir/run/qdrant.pid")" 2>/dev/null; then
        log "qdrant already running: pid $(cat "$dir/run/qdrant.pid")"
        return 0
    fi
    QDRANT__SERVICE__HTTP_PORT=16333 \
    QDRANT__STORAGE__STORAGE_PATH="$dir/data/$RUN_ID" \
    QDRANT__STORAGE__SNAPSHOTS_PATH="$dir/data/$RUN_ID/snapshots" \
        nohup "$dir/bin/qdrant" >> "$LOG_ROOT/qdrant-$RUN_ID.log" 2>&1 &
    echo $! > "$dir/run/qdrant.pid"
    log "qdrant started: pid $(cat "$dir/run/qdrant.pid"), data=$dir/data/$RUN_ID"
    wait_http qdrant "$QDRANT_URL/healthz" 60
}

start_es() {
    local dir base archive
    dir=$(es_dir); base="elasticsearch-$ES_VERSION"
    archive="$CACHE/$base-linux-x86_64.tar.gz"
    download "https://artifacts.elastic.co/downloads/elasticsearch/$base-linux-x86_64.tar.gz" "$archive"
    mkdir -p "$dir/bin" "$dir/data/$RUN_ID" "$dir/logs" "$dir/run"
    if [[ ! -x "$dir/bin/elasticsearch/bin/elasticsearch" ]]; then
        mkdir -p "$dir/bin/elasticsearch"
        tar -xzf "$archive" -C "$dir/bin/elasticsearch" --strip-components=1
    fi
    if [[ -f "$dir/run/elasticsearch.pid" ]] && kill -0 "$(cat "$dir/run/elasticsearch.pid")" 2>/dev/null; then
        log "elasticsearch already running: pid $(cat "$dir/run/elasticsearch.pid")"
        return 0
    fi
    ES_JAVA_OPTS="-Xms1g -Xmx1g" \
    ES_PATH_CONF="$dir/bin/elasticsearch/config" \
        nohup "$dir/bin/elasticsearch/bin/elasticsearch" \
            -Ediscovery.type=single-node \
            -Enetwork.host=127.0.0.1 \
            -Ehttp.port=19200 \
            -Etransport.port=19300 \
            -Epath.data="$dir/data/$RUN_ID" \
            -Epath.logs="$LOG_ROOT" \
            -Expack.security.enabled=false \
            -Eingest.geoip.downloader.enabled=false \
            >> "$LOG_ROOT/elasticsearch-$RUN_ID.log" 2>&1 &
    echo $! > "$dir/run/elasticsearch.pid"
    log "elasticsearch started: pid $(cat "$dir/run/elasticsearch.pid"), data=$dir/data/$RUN_ID"
    # Cluster boot includes JVM and shard recovery; keep this above the
    # per-request timeout so benchmarks never start against a half-ready node.
    wait_http elasticsearch "$ES_URL/" 240
}

stop_service() {
    local name=$1 dir pid
    if [[ "$name" == qdrant ]]; then dir=$(qdrant_dir); else dir=$(es_dir); fi
    if [[ ! -f "$dir/run/$name.pid" ]]; then
        log "$name: no pid file"
        return 0
    fi
    pid=$(cat "$dir/run/$name.pid")
    if kill -0 "$pid" 2>/dev/null; then
        kill "$pid" && log "$name: stopped pid $pid"
    else
        log "$name: pid $pid already exited"
    fi
    rm -f "$dir/run/$name.pid"
}

status_service() {
    local name=$1 dir pid
    if [[ "$name" == qdrant ]]; then dir=$(qdrant_dir); else dir=$(es_dir); fi
    pid=$(cat "$dir/run/$name.pid" 2>/dev/null || true)
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
        log "$name: running pid $pid"
    else
        log "$name: not running"
    fi
}

clean_service() {
    local name=$1 dir
    if [[ "$name" == qdrant ]]; then dir=$(qdrant_dir); else dir=$(es_dir); fi
    rm -rf "$dir/data" "$dir/run"
    log "$name: removed run data and pid files (cache kept)"
}

target=${2:-all}
case "${1:-}" in
    up)
        [[ "$target" == all || "$target" == qdrant ]] && start_qdrant
        [[ "$target" == all || "$target" == elasticsearch ]] && start_es
        ;;
    down)
        [[ "$target" == all || "$target" == qdrant ]] && stop_service qdrant
        [[ "$target" == all || "$target" == elasticsearch ]] && stop_service elasticsearch
        ;;
    status)
        [[ "$target" == all || "$target" == qdrant ]] && status_service qdrant
        [[ "$target" == all || "$target" == elasticsearch ]] && status_service elasticsearch
        ;;
    clean)
        [[ "$target" == all || "$target" == qdrant ]] && clean_service qdrant
        [[ "$target" == all || "$target" == elasticsearch ]] && clean_service elasticsearch
        ;;
    *)
        echo "usage: $0 {up|down|status|clean} [qdrant|elasticsearch|all]" >&2
        exit 2
        ;;
esac
true
