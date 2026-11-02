#ifndef VERITAS_H
#define VERITAS_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct VeritasEngine VeritasEngine;

typedef void (*VeritasProgressCb)(const char* json_event, void* user_data);

VeritasEngine* veritas_init(const char* config_json);
void          veritas_free(VeritasEngine* engine);

char* veritas_analyze(
    VeritasEngine* engine,
    const char* request_json,
    VeritasProgressCb progress_cb,
    void* user_data
);

void veritas_cancel(VeritasEngine* engine, uint64_t request_id);
void veritas_free_string(char* s);

char* veritas_health(VeritasEngine* engine);
char* veritas_list_models(VeritasEngine* engine);

/* Reads GGUF metadata without loading weights. Returns owned JSON. */
char* veritas_inspect_model(const char* path_utf8);

/* Loads the configured model and validates its weights. Returns owned JSON. */
char* veritas_preload_model(
    VeritasEngine* engine,
    VeritasProgressCb progress_cb,
    void* user_data
);

#ifdef __cplusplus
}
#endif

#endif /* VERITAS_H */
