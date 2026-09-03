/*
 * A minimal CCE native plugin written in C, compiled against
 * cce_plugin.h. Used by the host-side end-to-end test to verify that the
 * host loader consumes plugins that do NOT use the Rust SDK.
 *
 * Capabilities:
 *   - BM25 single generation: returns "C-plugin-bm25", or "none" when the
 *     group name contains "skip".
 *   - BM25 batch generation: returns per-group texts, null for the second
 *     element (exercises the host's null/skip handling).
 *   - Embedding single/batch: always returns an error with
 *     error_type "logic" (exercises error_type restoration on the host).
 */

#include "cce_plugin.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static const char META[] =
    "{\"id\":\"e2e/c-plugin\",\"name\":\"C E2E Plugin\",\"version\":\"0.1.0\","
    "\"priority\":7,\"description\":\"C plugin e2e test\"}";

static char *dup_str(const char *s) {
    char *p = (char *)malloc(strlen(s) + 1);
    if (p) {
        strcpy(p, s);
    }
    return p;
}

uint32_t cce_plugin_abi_version(void) {
    return CCE_PLUGIN_ABI_VERSION;
}

char *cce_plugin_metadata(void) {
    return dup_str(META);
}

bool cce_plugin_has_bm25_generation(void) {
    return true;
}

bool cce_plugin_has_embedding_generation(void) {
    return true;
}

bool cce_plugin_has_lifecycle(void) {
    return false;
}

void *cce_plugin_create(void) {
    return NULL;
}

void cce_plugin_destroy(void *ctx) {
    (void)ctx;
}

void cce_plugin_free_string(char *ptr) {
    free(ptr);
}

/* Count entity groups in a JSON array by counting "group_id" keys. */
static long count_groups(const char *json) {
    const char *needle = "\"group_id\":";
    long n = 0;
    const char *p = json;
    size_t needle_len = strlen(needle);
    while ((p = strstr(p, needle)) != NULL) {
        n++;
        p += needle_len;
    }
    return n;
}

char *cce_plugin_generate_bm25(void *ctx, const char *group_json) {
    (void)ctx;
    if (strstr(group_json, "skip") != NULL) {
        return dup_str("{\"result\":\"none\"}");
    }
    return dup_str("{\"result\":\"ok\",\"value\":\"C-plugin-bm25\"}");
}

char *cce_plugin_generate_embedding(void *ctx, const char *group_json) {
    (void)ctx;
    (void)group_json;
    return dup_str(
        "{\"result\":\"error\",\"message\":\"embedding unsupported by C plugin\","
        "\"error_type\":\"logic\"}");
}

char *cce_plugin_generate_bm25_batch(void *ctx, const char *groups_json) {
    (void)ctx;
    long n = count_groups(groups_json);
    size_t cap = 128 + (size_t)n * 32;
    char *buf = (char *)malloc(cap);
    if (!buf) {
        return NULL;
    }
    strcpy(buf, "{\"result\":\"ok\",\"value\":[");
    for (long i = 0; i < n; i++) {
        if (i > 0) {
            strcat(buf, ",");
        }
        if (i == 1) {
            strcat(buf, "null");
        } else {
            char item[32];
            snprintf(item, sizeof(item), "\"batch-bm25-%ld\"", i);
            strcat(buf, item);
        }
    }
    strcat(buf, "]}");
    return buf;
}

char *cce_plugin_generate_embedding_batch(void *ctx, const char *groups_json) {
    (void)ctx;
    (void)groups_json;
    return dup_str(
        "{\"result\":\"error\",\"message\":\"embedding batch unsupported by C plugin\","
        "\"error_type\":\"logic\"}");
}
