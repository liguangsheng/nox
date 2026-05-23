#include <stdio.h>
#include <string.h>

#include "../../crates/nox_core/include/nox_core.h"

typedef struct HostState {
    int64_t offset;
} HostState;

static NoxCoreStatus add_offset(
    void *ctx,
    const NoxCoreValue *args,
    size_t arg_count,
    NoxCoreValue *out_value
) {
    if (ctx == NULL || arg_count != 1 || args[0].kind != NOX_CORE_VALUE_INT) {
        return NOX_CORE_ERROR;
    }

    const HostState *state = (const HostState *)ctx;
    out_value->kind = NOX_CORE_VALUE_INT;
    out_value->bool_value = false;
    out_value->int_value = args[0].int_value + state->offset;
    out_value->float_value = 0.0;
    out_value->string_value = NULL;
    out_value->array_handle = NULL;
    out_value->map_handle = NULL;
    out_value->record_handle = NULL;
    out_value->option_handle = NULL;
    out_value->result_handle = NULL;
    return NOX_CORE_OK;
}

static NoxCoreStatus fail_callback(
    void *ctx,
    const NoxCoreValue *args,
    size_t arg_count,
    NoxCoreValue *out_value
) {
    (void)ctx;
    (void)args;
    (void)arg_count;
    (void)out_value;
    return NOX_CORE_ERROR;
}

int main(void) {
    printf("nox_core %s\n", nox_core_version());

    NoxCoreEngine *engine = nox_core_engine_new();
    if (engine == NULL) {
        return 1;
    }

    NoxCoreStatus status = NOX_CORE_OK;
    HostState state = {21};
    if (nox_core_engine_set_userdata(engine, &state) != NOX_CORE_OK) {
        nox_core_engine_free(engine);
        return 1;
    }

    NoxCoreValueKind params[] = {NOX_CORE_VALUE_INT};
    const char *host_caps[] = {"host.math"};
    status = nox_core_engine_register_host_function_ex(
        engine,
        "math__add_offset",
        params,
        1,
        NOX_CORE_VALUE_INT,
        add_offset,
        NULL,
        "Adds the engine userdata offset to an integer.",
        host_caps,
        1
    );
    if (status != NOX_CORE_OK) {
        const char *error = nox_core_engine_last_error(engine);
        fprintf(stderr, "%s\n", error != NULL ? error : "host registration failed");
        nox_core_engine_free(engine);
        return 1;
    }

    NoxCoreValue host_value = {0};
    status = nox_core_engine_eval(engine, "math__add_offset(21);", &host_value);
    if (status != NOX_CORE_OK || host_value.kind != NOX_CORE_VALUE_INT || host_value.int_value != 42) {
        const char *error = nox_core_engine_last_error(engine);
        fprintf(stderr, "%s\n", error != NULL ? error : "host callback failed");
        nox_core_engine_free(engine);
        return 1;
    }

    status = nox_core_engine_register_host_function(
        engine,
        "fail_callback",
        NULL,
        0,
        NOX_CORE_VALUE_NULL,
        fail_callback,
        NULL
    );
    if (status != NOX_CORE_OK) {
        const char *error = nox_core_engine_last_error(engine);
        fprintf(stderr, "%s\n", error != NULL ? error : "fail_callback registration failed");
        nox_core_engine_free(engine);
        return 1;
    }
    NoxCoreValue failed_value = {0};
    status = nox_core_engine_eval(engine, "fail_callback();", &failed_value);
    if (status == NOX_CORE_OK) {
        fprintf(stderr, "fail_callback unexpectedly succeeded\n");
        nox_core_engine_free(engine);
        return 1;
    }
    const char *callback_error = nox_core_engine_last_error(engine);
    if (callback_error == NULL || strstr(callback_error, "fail_callback") == NULL) {
        fprintf(stderr, "%s\n", callback_error != NULL ? callback_error : "missing callback error");
        nox_core_engine_free(engine);
        return 1;
    }
    nox_core_engine_clear_error(engine);
    if (nox_core_engine_last_error(engine) != NULL) {
        fprintf(stderr, "last_error was not cleared\n");
        nox_core_engine_free(engine);
        return 1;
    }

    NoxCoreValue value = {0};
    status = nox_core_engine_eval(engine, "\"hello\" + \" c\";", &value);
    if (status != NOX_CORE_OK) {
        const char *error = nox_core_engine_last_error(engine);
        fprintf(stderr, "%s\n", error != NULL ? error : "unknown nox_core error");
        nox_core_engine_free(engine);
        return 1;
    }

    if (value.kind != NOX_CORE_VALUE_STRING || value.string_value == NULL) {
        nox_core_engine_free(engine);
        return 1;
    }

    printf("%s\n", value.string_value);
    nox_core_string_free(value.string_value);

    NoxCoreValue array = {0};
    status = nox_core_engine_eval(engine, "[10, 20];", &array);
    if (status != NOX_CORE_OK || array.kind != NOX_CORE_VALUE_ARRAY || array.array_handle == NULL) {
        const char *error = nox_core_engine_last_error(engine);
        fprintf(stderr, "%s\n", error != NULL ? error : "array eval failed");
        nox_core_engine_free(engine);
        return 1;
    }
    if (nox_core_array_len(array.array_handle) != 2) {
        nox_core_array_free(array.array_handle);
        nox_core_engine_free(engine);
        return 1;
    }
    NoxCoreValue element = {0};
    status = nox_core_array_get(array.array_handle, 1, &element);
    if (status != NOX_CORE_OK || element.kind != NOX_CORE_VALUE_INT || element.int_value != 20) {
        nox_core_array_free(array.array_handle);
        nox_core_engine_free(engine);
        return 1;
    }
    nox_core_array_free(array.array_handle);

    NoxCoreValue map = {0};
    status = nox_core_engine_eval(engine, "let scores: map[str, int] = {\"core\": 42}; scores;", &map);
    if (status != NOX_CORE_OK || map.kind != NOX_CORE_VALUE_MAP || map.map_handle == NULL) {
        const char *error = nox_core_engine_last_error(engine);
        fprintf(stderr, "%s\n", error != NULL ? error : "map eval failed");
        nox_core_engine_free(engine);
        return 1;
    }
    NoxCoreValue map_value = {0};
    status = nox_core_map_get(map.map_handle, "core", &map_value);
    if (status != NOX_CORE_OK || map_value.kind != NOX_CORE_VALUE_INT || map_value.int_value != 42) {
        nox_core_map_free(map.map_handle);
        nox_core_engine_free(engine);
        return 1;
    }
    nox_core_map_free(map.map_handle);

    NoxCoreValue record = {0};
    status = nox_core_engine_eval(
        engine,
        "record User { name: str, score: int, } let user: User = User { name: \"nox\", score: 7 }; user;",
        &record
    );
    if (status != NOX_CORE_OK || record.kind != NOX_CORE_VALUE_RECORD || record.record_handle == NULL) {
        const char *error = nox_core_engine_last_error(engine);
        fprintf(stderr, "%s\n", error != NULL ? error : "record eval failed");
        nox_core_engine_free(engine);
        return 1;
    }
    NoxCoreValue field = {0};
    status = nox_core_record_field(record.record_handle, "name", &field);
    if (status != NOX_CORE_OK || field.kind != NOX_CORE_VALUE_STRING || field.string_value == NULL) {
        nox_core_record_free(record.record_handle);
        nox_core_engine_free(engine);
        return 1;
    }
    printf("%s\n", field.string_value);
    nox_core_string_free(field.string_value);
    nox_core_record_free(record.record_handle);

    NoxCoreValue option = {0};
    status = nox_core_engine_eval(engine, "let value: option[int] = some(9); value;", &option);
    if (status != NOX_CORE_OK || option.kind != NOX_CORE_VALUE_OPTION || option.option_handle == NULL) {
        const char *error = nox_core_engine_last_error(engine);
        fprintf(stderr, "%s\n", error != NULL ? error : "option eval failed");
        nox_core_engine_free(engine);
        return 1;
    }
    NoxCoreValue option_payload = {0};
    status = nox_core_option_payload(option.option_handle, &option_payload);
    if (!nox_core_option_is_some(option.option_handle)
        || status != NOX_CORE_OK
        || option_payload.kind != NOX_CORE_VALUE_INT
        || option_payload.int_value != 9) {
        nox_core_option_free(option.option_handle);
        nox_core_engine_free(engine);
        return 1;
    }
    nox_core_option_free(option.option_handle);

    NoxCoreValue result = {0};
    status = nox_core_engine_eval(engine, "let value: result[int, str] = ok(11); value;", &result);
    if (status != NOX_CORE_OK || result.kind != NOX_CORE_VALUE_RESULT || result.result_handle == NULL) {
        const char *error = nox_core_engine_last_error(engine);
        fprintf(stderr, "%s\n", error != NULL ? error : "result eval failed");
        nox_core_engine_free(engine);
        return 1;
    }
    NoxCoreValue result_payload = {0};
    status = nox_core_result_payload(result.result_handle, &result_payload);
    if (!nox_core_result_is_ok(result.result_handle)
        || status != NOX_CORE_OK
        || result_payload.kind != NOX_CORE_VALUE_INT
        || result_payload.int_value != 11) {
        nox_core_result_free(result.result_handle);
        nox_core_engine_free(engine);
        return 1;
    }
    nox_core_result_free(result.result_handle);

    nox_core_engine_free(engine);
    return 0;
}
