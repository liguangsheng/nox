#ifndef NOX_CORE_H
#define NOX_CORE_H

#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum NoxCoreStatus {
    NOX_CORE_OK = 0,
    NOX_CORE_NULL_POINTER = 1,
    NOX_CORE_INVALID_UTF8 = 2,
    NOX_CORE_ERROR = 3,
} NoxCoreStatus;

typedef enum NoxCoreValueKind {
    NOX_CORE_VALUE_NULL = 0,
    NOX_CORE_VALUE_BOOL = 1,
    NOX_CORE_VALUE_INT = 2,
    NOX_CORE_VALUE_FLOAT = 3,
    NOX_CORE_VALUE_STRING = 4,
    NOX_CORE_VALUE_FUNCTION = 5,
    NOX_CORE_VALUE_ARRAY = 6,
    NOX_CORE_VALUE_MAP = 7,
    NOX_CORE_VALUE_RECORD = 8,
    NOX_CORE_VALUE_OPTION = 9,
    NOX_CORE_VALUE_RESULT = 10,
    NOX_CORE_VALUE_JSON = 11,
    NOX_CORE_VALUE_TUPLE = 12,
    NOX_CORE_VALUE_ENUM = 13,
} NoxCoreValueKind;

typedef struct NoxCoreValue {
    NoxCoreValueKind kind;
    bool bool_value;
    int64_t int_value;
    double float_value;
    /* Owned string result. Free exactly once with nox_core_string_free. */
    char *string_value;
    /* Owned compound handles. Free exactly once with the matching free function. */
    struct NoxCoreArrayHandle *array_handle;
    struct NoxCoreMapHandle *map_handle;
    struct NoxCoreRecordHandle *record_handle;
    struct NoxCoreOptionHandle *option_handle;
    struct NoxCoreResultHandle *result_handle;
} NoxCoreValue;

typedef struct NoxCoreEngine NoxCoreEngine;
typedef struct NoxCoreArrayHandle NoxCoreArrayHandle;
typedef struct NoxCoreMapHandle NoxCoreMapHandle;
typedef struct NoxCoreRecordHandle NoxCoreRecordHandle;
typedef struct NoxCoreOptionHandle NoxCoreOptionHandle;
typedef struct NoxCoreResultHandle NoxCoreResultHandle;

typedef NoxCoreStatus (*NoxCoreHostCallback)(
    void *ctx,
    const NoxCoreValue *args,
    size_t arg_count,
    NoxCoreValue *out_value
);

const char *nox_core_version(void);
void nox_core_string_free(char *value);
void nox_core_array_free(NoxCoreArrayHandle *handle);
void nox_core_map_free(NoxCoreMapHandle *handle);
void nox_core_record_free(NoxCoreRecordHandle *handle);
void nox_core_option_free(NoxCoreOptionHandle *handle);
void nox_core_result_free(NoxCoreResultHandle *handle);
size_t nox_core_array_len(const NoxCoreArrayHandle *handle);
NoxCoreStatus nox_core_array_get(
    const NoxCoreArrayHandle *handle,
    size_t index,
    NoxCoreValue *out_value
);
size_t nox_core_map_len(const NoxCoreMapHandle *handle);
NoxCoreStatus nox_core_map_keys(
    const NoxCoreMapHandle *handle,
    NoxCoreValue *out_values,
    size_t capacity,
    size_t *written
);
NoxCoreStatus nox_core_map_get(
    const NoxCoreMapHandle *handle,
    const char *key,
    NoxCoreValue *out_value
);
NoxCoreStatus nox_core_record_field(
    const NoxCoreRecordHandle *handle,
    const char *name,
    NoxCoreValue *out_value
);
bool nox_core_option_is_some(const NoxCoreOptionHandle *handle);
NoxCoreStatus nox_core_option_payload(
    const NoxCoreOptionHandle *handle,
    NoxCoreValue *out_value
);
bool nox_core_result_is_ok(const NoxCoreResultHandle *handle);
NoxCoreStatus nox_core_result_payload(
    const NoxCoreResultHandle *handle,
    NoxCoreValue *out_value
);
NoxCoreEngine *nox_core_engine_new(void);
void nox_core_engine_free(NoxCoreEngine *engine);
/*
 * Store or read engine-level userdata. Nox stores this pointer verbatim and
 * never dereferences or frees it. If a host function was registered with
 * ctx == NULL, callbacks receive the engine userdata value that is current at
 * call time. A non-NULL ctx passed during registration still takes precedence.
 */
NoxCoreStatus nox_core_engine_set_userdata(
    NoxCoreEngine *engine,
    void *userdata
);
void *nox_core_engine_userdata(const NoxCoreEngine *engine);
/*
 * Registers a synchronous host callback.
 *
 * name and param_types are copied during registration. callback must remain
 * callable until it is replaced or the engine is freed. ctx is owned by the
 * host; Nox does not dereference or free it. The callback runs on the same
 * thread that is evaluating the script. Nox does not make the engine
 * thread-safe or reentrant.
 */
NoxCoreStatus nox_core_engine_register_host_function(
    NoxCoreEngine *engine,
    const char *name,
    const NoxCoreValueKind *param_types,
    size_t param_count,
    NoxCoreValueKind return_type,
    NoxCoreHostCallback callback,
    void *ctx
);
/*
 * Extended host callback registration with metadata.
 *
 * docstring may be NULL. capabilities may be NULL only when capability_count
 * is zero; otherwise it must point to capability_count NUL-terminated UTF-8
 * strings. All strings are copied during registration. Callback lifetime,
 * ctx ownership, same-thread execution, and non-reentrant engine rules are the
 * same as nox_core_engine_register_host_function.
 */
NoxCoreStatus nox_core_engine_register_host_function_ex(
    NoxCoreEngine *engine,
    const char *name,
    const NoxCoreValueKind *param_types,
    size_t param_count,
    NoxCoreValueKind return_type,
    NoxCoreHostCallback callback,
    void *ctx,
    const char *docstring,
    const char *const *capabilities,
    size_t capability_count
);
NoxCoreStatus nox_core_engine_eval(
    NoxCoreEngine *engine,
    const char *source,
    NoxCoreValue *out_value
);
NoxCoreStatus nox_core_engine_check(
    NoxCoreEngine *engine,
    const char *source
);
const char *nox_core_engine_last_error(const NoxCoreEngine *engine);
void nox_core_engine_clear_error(NoxCoreEngine *engine);

#ifdef __cplusplus
}
#endif

#endif
