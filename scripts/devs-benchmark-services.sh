#!/usr/bin/env bash
# On-demand external services for CCE benchmarks on devs.
#
# Usage: bash scripts/devs-benchmark-services.sh {up|down|status|metrics|sample|env|clean|run} [target] [-- command ...]
#
# Services are started only for a benchmark run and stopped by PID file. The
# downloaded archives stay cached; `run` removes the data directories it created
# on exit, including on failure.
set -u

INSTALL_ROOT=${CCE_SERVICE_ROOT:-/opt/cce-services}
DATA_ROOT=${CCE_SERVICE_DATA:-/var/lib/cce-services}
CACHE="$INSTALL_ROOT/cache"
LOG_ROOT=${CCE_SERVICE_LOG:-/var/log/cce-services}
mkdir -p "$CACHE" "$LOG_ROOT"

QDRANT_VERSION=${QDRANT_VERSION:-1.15.5}
ES_VERSION=${ES_VERSION:-7.17.28}
PG_VERSION=${PG_VERSION:-10.23}
QDRANT_URL=${QDRANT_URL:-http://127.0.0.1:16333}
ES_URL=${ES_URL:-http://127.0.0.1:19200}
PG_URL=${PG_URL:-postgres://postgres@127.0.0.1:15432/postgres}
RUN_ID=${RUN_ID:-$(date +%s)}

log() { printf '[%s] %s\n' "$(date -u +%FT%TZ)" "$*"; }

qdrant_dir() { echo "$INSTALL_ROOT/qdrant"; }
es_dir() { echo "$INSTALL_ROOT/elasticsearch"; }
pg_dir() { echo "$INSTALL_ROOT/postgres"; }
qdrant_data() { echo "$DATA_ROOT/qdrant"; }
es_data() { echo "$DATA_ROOT/elasticsearch"; }
pg_data() { echo "$DATA_ROOT/postgres"; }
pg_bin() { echo "$(pg_dir)/usr/lib/postgresql/10/bin"; }
target_dir() {
    case "$1" in
        qdrant) qdrant_dir ;;
        elasticsearch) es_dir ;;
        postgres) pg_dir ;;
    esac
}
target_run_dir() {
    case "$1" in
        qdrant) echo "$(qdrant_data)/run" ;;
        elasticsearch) echo "$(es_data)/run" ;;
        postgres) echo "$(pg_data)/run" ;;
    esac
}

download() {
    local url=$1 path=$2
    if [[ -f "$path" ]]; then
        log "cache hit: $path ($(du -h "$path" | cut -f1))"
        return 0
    fi
    log "downloading $url"
    mkdir -p "$(dirname "$path")"
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

wait_pg() {
    local timeout=$1 deadline=$((SECONDS + 60))
    local pg_isready
    pg_isready="$(pg_bin)/pg_isready"
    while (( SECONDS < deadline )); do
        if "$pg_isready" -h 127.0.0.1 -p 15432 -q; then
            log "postgres healthy: 127.0.0.1:15432"
            return 0
        fi
        sleep 1
    done
    log "postgres failed health check: 127.0.0.1:15432"
    return 1
}

require_stopped() {
    local name=$1
    if [[ $(service_health "$name") == healthy ]]; then
        log "$name is already healthy; refusing to manage a service started elsewhere"
        return 1
    fi
}

record_run_data() {
    local name=$1 data=$2
    echo "$data" > "$(target_run_dir "$name")/$name.data"
}

start_qdrant() {
    local dir archive
    require_stopped qdrant || return 1
    dir=$(qdrant_dir)
    archive="$CACHE/qdrant-x86_64-unknown-linux-musl-$QDRANT_VERSION.tar.gz"
    download "https://github.com/qdrant/qdrant/releases/download/v$QDRANT_VERSION/qdrant-x86_64-unknown-linux-musl.tar.gz" "$archive"
    local data run
    data="$(qdrant_data)/$RUN_ID"
    run="$(target_run_dir qdrant)"
    mkdir -p "$dir/bin" "$data" "$run" "$LOG_ROOT/qdrant"
    record_run_data qdrant "$data"
    if [[ ! -x "$dir/bin/qdrant" ]]; then
        tar -xzf "$archive" -C "$dir/bin"
        chmod +x "$dir/bin/qdrant"
    fi
    QDRANT__SERVICE__HTTP_PORT=16333 \
    QDRANT__STORAGE__STORAGE_PATH="$data" \
    QDRANT__STORAGE__SNAPSHOTS_PATH="$data/snapshots" \
        nohup "$dir/bin/qdrant" >> "$LOG_ROOT/qdrant/qdrant-$RUN_ID.log" 2>&1 &
    echo $! > "$run/qdrant.pid"
    log "qdrant started: pid $(cat "$run/qdrant.pid"), data=$data"
    wait_http qdrant "$QDRANT_URL/healthz" 60
}

start_es() {
    local dir archive
    require_stopped elasticsearch || return 1
    dir=$(es_dir)
    archive="$CACHE/elasticsearch-$ES_VERSION-linux-x86_64.tar.gz"
    download "https://artifacts.elastic.co/downloads/elasticsearch/elasticsearch-$ES_VERSION-linux-x86_64.tar.gz" "$archive"
    local data run
    data="$(es_data)/$RUN_ID"
    run="$(target_run_dir elasticsearch)"
    mkdir -p "$data" "$run" "$LOG_ROOT/elasticsearch"
    record_run_data elasticsearch "$data"
    if [[ ! -x "$dir/bin/elasticsearch" ]]; then
        tar -xzf "$archive" -C "$dir" --strip-components=1
    fi
    ES_JAVA_OPTS="-Xms1g -Xmx1g" \
    ES_JAVA_HOME="$dir/jdk" \
    ES_PATH_CONF="$dir/config" \
        nohup "$dir/bin/elasticsearch" \
            -Ediscovery.type=single-node \
            -Enetwork.host=127.0.0.1 \
            -Ehttp.port=19200 \
            -Etransport.port=19300 \
            -Epath.data="$data" \
            -Epath.logs="$LOG_ROOT/elasticsearch" \
            -Expack.security.enabled=false \
            -Eingest.geoip.downloader.enabled=false \
            >> "$LOG_ROOT/elasticsearch/elasticsearch-$RUN_ID.log" 2>&1 &
    echo $! > "$run/elasticsearch.pid"
    log "elasticsearch started: pid $(cat "$run/elasticsearch.pid"), data=$data"
    # Cluster boot includes JVM and shard recovery; keep this above the
    # per-request timeout so benchmarks never start against a half-ready node.
    wait_http elasticsearch "$ES_URL/" 240
}

prepare_pg() {
    local dir bin deb
    dir=$(pg_dir)
    bin="$(pg_bin)"
    if [[ -x "$bin/postgres" ]]; then
        return 0
    fi
    mkdir -p "$CACHE/pg-debs" "$dir/usr"
    for package in libpq5 postgresql-client-10 postgresql-10; do
        deb=$(find "$CACHE/pg-debs" -name "${package}_*_amd64.deb" -print -quit 2>/dev/null || true)
        if [[ -z "$deb" ]]; then
            deb="$CACHE/pg-debs/${package}_${PG_VERSION}-0ubuntu0.18.04.2_amd64.deb"
            pool=main
            [[ "$package" == postgresql-* ]] && pool=universe
            download "http://archive.ubuntu.com/ubuntu/pool/$pool/p/postgresql-10/$(basename "$deb")" "$deb"
        fi
        dpkg-deb -x "$deb" "$dir/usr"
    done
}

start_pg() {
    local dir bin data run pid
    require_stopped postgres || return 1
    prepare_pg
    dir=$(pg_dir)
    bin="$(pg_bin)"
    data="$(pg_data)/data"
    run="$(target_run_dir postgres)"
    mkdir -p "$data" "$run" "$LOG_ROOT/postgres"
    if "$bin/pg_isready" -h 127.0.0.1 -p 15432 -q; then
        log "postgres already healthy: 127.0.0.1:15432"
        return 0
    fi
    if [[ ! -f "$data/PG_VERSION" ]]; then
        mkdir -p "$data"
        "$bin/initdb" -D "$data" -A trust -U postgres \
            > "$LOG_ROOT/postgres/pg-initdb-$RUN_ID.log" 2>&1
    fi
    "$bin/pg_ctl" -D "$data" \
        -o "-h 127.0.0.1 -p 15432 -k $run" \
        -l "$LOG_ROOT/postgres/postgres-$RUN_ID.log" -w start
    pid=$(head -n 1 "$data/postmaster.pid")
    echo "$pid" > "$run/postgres.pid"
    echo "$data" > "$run/postgres.data"
    log "postgres started: pid $pid, data=$data"
    wait_pg 60
}

stop_pid() {
    local name=$1 run pid
    run=$(target_run_dir "$name")
    if [[ ! -f "$run/$name.pid" ]]; then
        log "$name: no pid file"
        return 0
    fi
    pid=$(cat "$run/$name.pid")
    if kill -0 "$pid" 2>/dev/null; then
        kill "$pid" && log "$name: stopped pid $pid"
    else
        log "$name: pid $pid already exited"
    fi
    rm -f "$run/$name.pid"
}

stop_pg() {
    local run data pid
    run="$(target_run_dir postgres)"
    if [[ -f "$run/postgres.pid" ]]; then
        pid=$(cat "$run/postgres.pid")
        data=$(cat "$run/postgres.data" 2>/dev/null || true)
        if [[ -n "$data" && -x "$(pg_bin)/pg_ctl" ]]; then
            "$(pg_bin)/pg_ctl" -D "$data" -m fast -w stop \
                || stop_pid postgres
        elif kill -0 "$pid" 2>/dev/null; then
            kill "$pid" && log "postgres: stopped pid $pid"
        fi
        rm -f "$run/postgres.pid"
    else
        log "postgres: no pid file"
    fi
}

stop_service() {
    case "$1" in
        qdrant|elasticsearch) stop_pid "$1" ;;
        postgres) stop_pg ;;
    esac
}

wait_stopped() {
    local name=$1 deadline=$((SECONDS + 60))
    while (( SECONDS < deadline )); do
        [[ $(service_health "$name") == stopped ]] && return 0
        sleep 1
    done
    log "$name still healthy after stop request"
    return 1
}

prepare_services() {
    local service
    for service in $services; do
        stop_service "$service"
        wait_stopped "$service" || return 1
    done
}

status_service() {
    local name=$1 run pid health
    run=$(target_run_dir "$name")
    pid=$(cat "$run/$name.pid" 2>/dev/null || true)
    health=stopped
    case "$name" in
        qdrant)
            curl -fsS --max-time 3 "$QDRANT_URL/healthz" >/dev/null 2>&1 && health=healthy
            ;;
        elasticsearch)
            curl -fsS --max-time 3 "$ES_URL/" >/dev/null 2>&1 && health=healthy
            ;;
        postgres)
            [[ -x "$(pg_bin)/pg_isready" ]] \
                && "$(pg_bin)/pg_isready" -h 127.0.0.1 -p 15432 -q \
                && health=healthy
            ;;
    esac
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
        log "$name: running pid $pid, health=$health"
    else
        log "$name: no live pid, health=$health"
    fi
}

process_stats() {
    local pid=$1 utime stime rss hwm
    [[ -r "/proc/$pid/stat" ]] || { printf '%s\t%s\t%s\n' 0 0 0; return 0; }
    read -r utime stime < <(awk '{print $14, $15}' "/proc/$pid/stat")
    rss=$(awk '/^VmRSS:/ {print $2}' "/proc/$pid/status" 2>/dev/null || echo 0)
    hwm=$(awk '/^VmHWM:/ {print $2}' "/proc/$pid/status" 2>/dev/null || echo 0)
    awk -v u="${utime:-0}" -v s="${stime:-0}" -v r="${rss:-0}" -v h="${hwm:-0}" \
        'BEGIN {printf "%.2f\t%d\t%d\n", (u+s)/100, r, h}'
}

service_health() {
    local name=$1
    case "$name" in
        qdrant) curl -fsS --max-time 3 "$QDRANT_URL/healthz" >/dev/null 2>&1 && echo healthy || echo stopped ;;
        elasticsearch) curl -fsS --max-time 3 "$ES_URL/" >/dev/null 2>&1 && echo healthy || echo stopped ;;
        postgres) [[ -x "$(pg_bin)/pg_isready" ]] \
            && "$(pg_bin)/pg_isready" -h 127.0.0.1 -p 15432 -q && echo healthy || echo stopped ;;
    esac
}

metrics_service() {
    local name=$1 run pid stats
    run=$(target_run_dir "$name")
    pid=$(cat "$run/$name.pid" 2>/dev/null || true)
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
        stats=$(process_stats "$pid")
        awk -v n="$name" -v p="$pid" -v s="$stats" \
            'BEGIN {split(s, a, "\t"); printf "%s pid=%s cpu_s=%s rss_kb=%s peak_rss_kb=%s\n", n, p, a[1], a[2], a[3]}'
    else
        log "$name not running"
    fi
}

sample_services() {
    local name run pid stats health stamp
    stamp=$(date -u +%FT%TZ)
    for name in $services; do
        run=$(target_run_dir "$name")
        pid=$(cat "$run/$name.pid" 2>/dev/null || true)
        if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
            stats=$(process_stats "$pid")
            health=$(service_health "$name")
            awk -v t="$stamp" -v n="$name" -v p="$pid" -v s="$stats" -v h="$health" \
                'BEGIN {split(s, a, "\t"); printf "%s\t%s\t%s\t%s\t%s\t%s\t%s\n", t, n, p, a[1], a[2], a[3], h}'
        else
            printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$stamp" "$name" 0 0 0 0 stopped
        fi
    done
}

clean_current_service() {
    local name=$1 run data
    run=$(target_run_dir "$name")
    data=$(cat "$run/$name.data" 2>/dev/null || true)
    if [[ -n "$data" && "$data" == "$DATA_ROOT/$name/"* ]]; then
        rm -rf -- "$data"
        log "$name: removed run data $data"
    fi
    rm -f "$run/$name.data" "$run/$name.pid"
}

clean_service() {
    local name=$1
    stop_service "$name"
    if ! wait_stopped "$name"; then
        log "$name: refusing to delete data of a running service"
        return 1
    fi
    case "$name" in
        qdrant) rm -rf "$(qdrant_data)" ;;
        elasticsearch) rm -rf "$(es_data)" ;;
        postgres) rm -rf "$(pg_data)" ;;
    esac
    mkdir -p "$(target_run_dir "$name")"
    rm -f "$(target_run_dir "$name")/$name.data"
    log "$name: removed all run data and pid files (installation and cache kept)"
}

print_env() {
    cat <<EOF
export CCE_TEST_QDRANT_URL=$QDRANT_URL
export CCE_TEST_ES_URL=$ES_URL
export CCE_TEST_PG_URL=$PG_URL
export PATH="$HOME/.cargo/bin:\$PATH"
EOF
}

target=${2:-all}
services="qdrant elasticsearch postgres"
[[ "$target" == all ]] || services="$target"
case "$services" in
    *qdrant*|*elasticsearch*|*postgres*) ;;
    *) echo "unknown service: $target" >&2; exit 2 ;;
esac
case "$1" in
    up)
        prepare_services || exit 1
        for service in $services; do
            case "$service" in
                qdrant) start_qdrant ;;
                elasticsearch) start_es ;;
                postgres) start_pg ;;
            esac || exit 1
        done
        ;;
    down)
        for service in $services; do
            stop_service "$service"
            wait_stopped "$service" || true
        done
        ;;
    status)
        for service in $services; do status_service "$service"; done
        ;;
    metrics)
        for service in $services; do metrics_service "$service"; done
        ;;
    sample)
        sample_services
        ;;
    env)
        print_env
        ;;
    clean)
        for service in $services; do clean_service "$service" || exit 1; done
        ;;
    run)
        shift 2
        [[ "${1:-}" == -- ]] && shift
        if (( $# == 0 )); then
            echo "usage: $0 run [target] -- command [args ...]" >&2
            exit 2
        fi
        export CCE_TEST_QDRANT_URL=$QDRANT_URL
        export CCE_TEST_ES_URL=$ES_URL
        export CCE_TEST_PG_URL=$PG_URL
        export PATH="$HOME/.cargo/bin:$PATH"
        started=()
        cleanup_started() {
            local status=$? service
            for service in "${started[@]}"; do
                stop_service "$service"
                wait_stopped "$service" || true
                clean_current_service "$service"
            done
            return "$status"
        }
        trap cleanup_started EXIT
        prepare_services || exit 1
        for service in $services; do
            case "$service" in
                qdrant) start_qdrant ;;
                elasticsearch) start_es ;;
                postgres) start_pg ;;
            esac || exit 1
            started+=("$service")
        done
        log "running: $*"
        "$@"
        exit $?
        ;;
    *)
        echo "usage: $0 {up|down|status|metrics|sample|env|clean|run} [qdrant|elasticsearch|postgres|all]" >&2
        exit 2
        ;;
esac
true
